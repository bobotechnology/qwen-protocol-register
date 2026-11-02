use anyhow::{Result, bail};
use chrono::Utc;
use image::{GrayImage, ImageBuffer, Luma, RgbaImage};
use imageproc::distance_transform::Norm;
use imageproc::edges::canny;
use imageproc::morphology::erode;
use imageproc::template_matching::{MatchTemplateMethod, match_template_with_mask};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SolverWeights {
    pub legacy: f64,
    #[serde(rename = "alphaGradient")]
    pub alpha_gradient: f64,
    #[serde(rename = "edgeDistance")]
    pub edge_distance: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Candidate {
    pub x: u32,
    #[serde(rename = "combinedScore")]
    pub combined_score: f64,
    #[serde(rename = "legacyScore")]
    pub legacy_score: f64,
    #[serde(rename = "gradientScore")]
    pub gradient_score: f64,
    #[serde(rename = "edgeDistanceScore")]
    pub edge_distance_score: f64,
    #[serde(rename = "edgeHits")]
    pub edge_hits: usize,
    #[serde(rename = "edgeScore")]
    pub edge_score: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SliderSolution {
    pub algorithm: String,
    pub weights: SolverWeights,
    #[serde(rename = "sourceOffset")]
    pub source_offset: u32,
    pub score: f64,
    #[serde(rename = "legacyScore")]
    pub legacy_score: f64,
    #[serde(rename = "gradientScore")]
    pub gradient_score: f64,
    #[serde(rename = "edgeDistanceScore")]
    pub edge_distance_score: f64,
    #[serde(rename = "templateOffset")]
    pub template_offset: Option<u32>,
    #[serde(rename = "templateScore")]
    pub template_score: Option<f64>,
    #[serde(rename = "diagnosticDelta")]
    pub diagnostic_delta: Option<u32>,
    #[serde(rename = "minOffset")]
    pub min_offset: u32,
    #[serde(rename = "maxOffset")]
    pub max_offset: u32,
    #[serde(rename = "edgeHits")]
    pub edge_hits: usize,
    #[serde(rename = "edgeScore")]
    pub edge_score: f64,
    pub brightness: f64,
    pub saturation: f64,
    pub variation: f64,
    #[serde(rename = "backgroundWidth")]
    pub background_width: u32,
    #[serde(rename = "pieceWidth")]
    pub piece_width: u32,
    #[serde(rename = "topCandidates")]
    pub top_candidates: Vec<Candidate>,
}

#[derive(Clone)]
struct Score {
    legacy: f64,
    x: u32,
    edge_hits: usize,
    edge_score: f64,
    brightness: f64,
    saturation: f64,
    variation: f64,
    gradient: f64,
    distance: f64,
}

fn grayscale(image: &RgbaImage) -> GrayImage {
    ImageBuffer::from_fn(image.width(), image.height(), |x, y| {
        let pixel = image.get_pixel(x, y).0;
        let value = 0.114 * pixel[2] as f64 + 0.587 * pixel[1] as f64 + 0.299 * pixel[0] as f64;
        Luma([value.round().clamp(0.0, 255.0) as u8])
    })
}

fn boundary(mask: &GrayImage) -> Vec<(u32, u32)> {
    let mut output = Vec::new();
    for y in 0..mask.height() {
        for x in 0..mask.width() {
            let mut minimum = 255u8;
            let mut maximum = 0u8;
            for dy in y.saturating_sub(1)..=(y + 1).min(mask.height() - 1) {
                for dx in x.saturating_sub(1)..=(x + 1).min(mask.width() - 1) {
                    let value = mask.get_pixel(dx, dy)[0];
                    minimum = minimum.min(value);
                    maximum = maximum.max(value);
                }
            }
            if maximum > minimum {
                output.push((x, y));
            }
        }
    }
    output
}

fn saturation(image: &RgbaImage) -> GrayImage {
    ImageBuffer::from_fn(image.width(), image.height(), |x, y| {
        let pixel = image.get_pixel(x, y).0;
        let maximum = pixel[0].max(pixel[1]).max(pixel[2]);
        let minimum = pixel[0].min(pixel[1]).min(pixel[2]);
        let value = if maximum == 0 {
            0.0
        } else {
            (maximum - minimum) as f64 * 255.0 / maximum as f64
        };
        Luma([value.round() as u8])
    })
}

fn gradient(gray: &GrayImage) -> Vec<f64> {
    let width = gray.width() as usize;
    let height = gray.height() as usize;
    let mut output = vec![0.0; width * height];
    for y in 0..height {
        for x in 0..width {
            let previous_x = if x == 0 { width - 1 } else { x - 1 };
            let next_x = if x + 1 == width { 0 } else { x + 1 };
            let previous_y = if y == 0 { height - 1 } else { y - 1 };
            let next_y = if y + 1 == height { 0 } else { y + 1 };
            let horizontal = gray.get_pixel(next_x as u32, y as u32)[0] as f64
                - gray.get_pixel(previous_x as u32, y as u32)[0] as f64;
            let vertical = gray.get_pixel(x as u32, next_y as u32)[0] as f64
                - gray.get_pixel(x as u32, previous_y as u32)[0] as f64;
            output[y * width + x] = horizontal.abs() + vertical.abs();
        }
    }
    output
}

fn distance_transform_l2(edges: &GrayImage) -> Vec<f64> {
    let width = edges.width() as usize;
    let height = edges.height() as usize;
    let infinity = 1e20;
    let mut squared = vec![0.0; width * height];
    for y in 0..height {
        for x in 0..width {
            squared[y * width + x] = if edges.get_pixel(x as u32, y as u32)[0] == 0 {
                infinity
            } else {
                0.0
            };
        }
    }
    for row in squared.chunks_exact_mut(width) {
        distance_1d(row);
    }
    let mut column = vec![0.0; height];
    for x in 0..width {
        for y in 0..height {
            column[y] = squared[y * width + x];
        }
        distance_1d(&mut column);
        for y in 0..height {
            squared[y * width + x] = column[y];
        }
    }
    squared.into_iter().map(f64::sqrt).collect()
}

fn distance_1d(values: &mut [f64]) {
    let original = values.to_vec();
    let mut sites = vec![0usize; values.len()];
    let mut boundaries = vec![0.0; values.len() + 1];
    let mut count = 0usize;
    boundaries[0] = f64::NEG_INFINITY;
    boundaries[1] = f64::INFINITY;
    for index in 1..values.len() {
        let mut intersection;
        loop {
            let site = sites[count];
            intersection =
                (original[index] + (index * index) as f64 - original[site] - (site * site) as f64)
                    / (2 * index - 2 * site) as f64;
            if intersection <= boundaries[count] && count > 0 {
                count -= 1;
            } else {
                break;
            }
        }
        count += 1;
        sites[count] = index;
        boundaries[count] = intersection;
        boundaries[count + 1] = f64::INFINITY;
    }
    let mut current = 0usize;
    for (index, output) in values.iter_mut().enumerate() {
        while boundaries[current + 1] < index as f64 {
            current += 1;
        }
        let site = sites[current];
        *output = ((index as isize - site as isize).pow(2)) as f64 + original[site];
    }
}

fn mean(values: impl Iterator<Item = f64>, count: usize) -> f64 {
    if count == 0 {
        0.0
    } else {
        values.sum::<f64>() / count as f64
    }
}

pub fn solve_images(
    background: &RgbaImage,
    piece: &RgbaImage,
    min_offset: u32,
) -> Result<SliderSolution> {
    if background.width() == 0
        || background.height() == 0
        || piece.width() == 0
        || piece.height() == 0
    {
        bail!("RGBA background/piece images required");
    }
    if piece.width() > background.width() || piece.height() > background.height() {
        bail!("piece image must fit inside background");
    }
    let max_offset = background.width() - piece.width();
    if min_offset > max_offset {
        bail!("minimum slider offset exceeds search range");
    }
    let mask = GrayImage::from_fn(piece.width(), piece.height(), |x, y| {
        Luma([if piece.get_pixel(x, y)[3] > 128 { 1 } else { 0 }])
    });
    let boundary_points = boundary(&mask);
    if boundary_points.is_empty() {
        bail!("piece image has no alpha boundary");
    }
    let gray = grayscale(background);
    let piece_gray = grayscale(piece);
    let edges = canny(&gray, 50.0, 150.0);
    let saturation = saturation(background);
    let gradient = gradient(&gray);
    let distances = distance_transform_l2(&edges);
    let mask_binary = GrayImage::from_fn(piece.width(), piece.height(), |x, y| {
        Luma([if mask.get_pixel(x, y)[0] > 0 { 255 } else { 0 }])
    });
    let interior = erode(&mask_binary, Norm::LInf, 2);
    let interior_points = interior
        .enumerate_pixels()
        .filter(|(_, _, pixel)| pixel[0] > 0)
        .map(|(x, y, _)| (x, y))
        .collect::<Vec<_>>();
    let interior_count = interior_points.len() * 3;
    let mut scores = Vec::new();
    for x in min_offset..=max_offset {
        let edge_values = boundary_points
            .iter()
            .filter_map(|(bx, by)| {
                let sample_x = bx + x;
                (sample_x < background.width() && *by < background.height())
                    .then(|| edges.get_pixel(sample_x, *by)[0] as f64)
            })
            .collect::<Vec<_>>();
        let edge_score = mean(edge_values.iter().copied(), edge_values.len());
        let edge_hits = edge_values.iter().filter(|value| **value > 0.0).count();
        let brightness = mean(
            interior_points
                .iter()
                .map(|(px, py)| gray.get_pixel(px + x, *py)[0] as f64),
            interior_points.len(),
        );
        let saturation_mean = mean(
            interior_points
                .iter()
                .map(|(px, py)| saturation.get_pixel(px + x, *py)[0] as f64),
            interior_points.len(),
        );
        let patch_mean = mean(
            interior_points.iter().flat_map(|(px, py)| {
                let pixel = background.get_pixel(px + x, *py).0;
                [pixel[2] as f64, pixel[1] as f64, pixel[0] as f64]
            }),
            interior_count,
        );
        let variation = if interior_count == 0 {
            0.0
        } else {
            (interior_points
                .iter()
                .flat_map(|(px, py)| {
                    let pixel = background.get_pixel(px + x, *py).0;
                    [pixel[2] as f64, pixel[1] as f64, pixel[0] as f64]
                })
                .map(|value| (value - patch_mean).powi(2))
                .sum::<f64>()
                / interior_count as f64)
                .sqrt()
        };
        let gradient_score = mean(
            boundary_points.iter().map(|(bx, by)| {
                (-1..=1)
                    .flat_map(|dy| (-1..=1).map(move |dx| (dx, dy)))
                    .map(|(dx, dy)| {
                        let sample_x = (*bx as i64 + x as i64 + dx)
                            .clamp(0, background.width() as i64 - 1)
                            as usize;
                        let sample_y =
                            (*by as i64 + dy).clamp(0, background.height() as i64 - 1) as usize;
                        gradient[sample_y * background.width() as usize + sample_x]
                    })
                    .fold(0.0f64, f64::max)
            }),
            boundary_points.len(),
        );
        let distance_score = mean(
            boundary_points.iter().map(|(bx, by)| {
                let sample_x = (*bx + x).min(background.width() - 1) as usize;
                distances[*by as usize * background.width() as usize + sample_x]
            }),
            boundary_points.len(),
        );
        let legacy = edge_score + 0.8 * brightness - 0.5 * saturation_mean - 0.3 * variation;
        scores.push(Score {
            legacy,
            x,
            edge_hits,
            edge_score,
            brightness,
            saturation: saturation_mean,
            variation,
            gradient: gradient_score,
            distance: distance_score,
        });
    }
    scores.sort_by(|left, right| {
        right
            .legacy
            .total_cmp(&left.legacy)
            .then_with(|| right.x.cmp(&left.x))
    });
    let legacy_min = scores
        .iter()
        .map(|score| score.legacy)
        .fold(f64::INFINITY, f64::min);
    let legacy_max = scores
        .iter()
        .map(|score| score.legacy)
        .fold(f64::NEG_INFINITY, f64::max);
    let gradient_min = scores
        .iter()
        .map(|score| score.gradient)
        .fold(f64::INFINITY, f64::min);
    let gradient_max = scores
        .iter()
        .map(|score| score.gradient)
        .fold(f64::NEG_INFINITY, f64::max);
    let distance_min = scores
        .iter()
        .map(|score| score.distance)
        .fold(f64::INFINITY, f64::min);
    let distance_max = scores
        .iter()
        .map(|score| score.distance)
        .fold(f64::NEG_INFINITY, f64::max);
    let mut ranked = scores
        .into_iter()
        .map(|score| {
            let normalize = |value: f64, minimum: f64, maximum: f64| {
                (value - minimum)
                    / if maximum - minimum == 0.0 {
                        1.0
                    } else {
                        maximum - minimum
                    }
            };
            let combined = 0.35 * normalize(score.legacy, legacy_min, legacy_max)
                + 0.50 * normalize(score.gradient, gradient_min, gradient_max)
                + 0.15 * (1.0 - normalize(score.distance, distance_min, distance_max));
            (combined, score)
        })
        .collect::<Vec<_>>();
    ranked.sort_by(|left, right| {
        right
            .0
            .total_cmp(&left.0)
            .then_with(|| right.1.legacy.total_cmp(&left.1.legacy))
    });
    let (best_combined, best) = ranked.first().cloned().expect("nonempty score range");
    let (template_offset, template_score) = template_match(&gray, &piece_gray, &mask_binary)
        .map_or((None, None), |(offset, score)| (Some(offset), Some(score)));
    let diagnostic_delta = template_offset.map(|offset| best.x.abs_diff(offset));
    let top_candidates = ranked
        .iter()
        .take(8)
        .map(|(combined, score)| Candidate {
            x: score.x,
            combined_score: *combined,
            legacy_score: score.legacy,
            gradient_score: score.gradient,
            edge_distance_score: score.distance,
            edge_hits: score.edge_hits,
            edge_score: score.edge_score,
        })
        .collect();
    Ok(SliderSolution {
        algorithm: "boundary_fusion_v1".into(),
        weights: SolverWeights {
            legacy: 0.35,
            alpha_gradient: 0.50,
            edge_distance: 0.15,
        },
        source_offset: best.x,
        score: best_combined,
        legacy_score: best.legacy,
        gradient_score: best.gradient,
        edge_distance_score: best.distance,
        template_offset,
        template_score,
        diagnostic_delta,
        min_offset,
        max_offset,
        edge_hits: best.edge_hits,
        edge_score: best.edge_score,
        brightness: best.brightness,
        saturation: best.saturation,
        variation: best.variation,
        background_width: background.width(),
        piece_width: piece.width(),
        top_candidates,
    })
}

fn template_match(
    background: &GrayImage,
    piece: &GrayImage,
    mask: &GrayImage,
) -> Option<(u32, f64)> {
    if piece.width() >= background.width() || piece.height() >= background.height() {
        return None;
    }
    let result = match_template_with_mask(
        background,
        piece,
        MatchTemplateMethod::CrossCorrelationNormalized,
        mask,
    );
    let mut best: Option<(u32, f64)> = None;
    for (x, _, pixel) in result.enumerate_pixels() {
        let score = pixel[0] as f64;
        if score.is_finite() && best.is_none_or(|(_, best_score)| score > best_score) {
            best = Some((x, score));
        }
    }
    best
}

pub fn solve_slider(background_path: &Path, piece_path: &Path) -> Result<SliderSolution> {
    let background = image::open(background_path)?.to_rgba8();
    let piece = image::open(piece_path)?.to_rgba8();
    let min_offset = std::env::var("SOLVER_MIN_OFFSET")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(120);
    let solution = solve_images(&background, &piece, min_offset)?;
    let stamp = Utc::now().format("%Y%m%dT%H%M%S%.6fZ").to_string();
    if let Ok(directory) = std::env::var("SOLVER_CAPTURE_DIR") {
        archive_images(
            directory.as_ref(),
            &stamp,
            background_path,
            piece_path,
            &solution,
            "solver.json",
        )?;
    }
    let threshold = std::env::var("SOLVER_ANOMALY_THRESHOLD")
        .ok()
        .and_then(|value| value.parse::<u32>().ok())
        .unwrap_or(30);
    if let (Ok(directory), Some(delta)) = (
        std::env::var("SOLVER_ANOMALY_DIR"),
        solution.diagnostic_delta,
    ) && delta > threshold
    {
        archive_images(
            directory.as_ref(),
            &stamp,
            background_path,
            piece_path,
            &solution,
            "diagnostic.json",
        )?;
    }
    Ok(solution)
}

fn archive_images(
    directory: &Path,
    stamp: &str,
    background: &Path,
    piece: &Path,
    solution: &SliderSolution,
    report_name: &str,
) -> Result<()> {
    let target = directory.join(stamp);
    std::fs::create_dir_all(&target)?;
    std::fs::copy(background, target.join("background.png"))?;
    std::fs::copy(piece, target.join("piece.png"))?;
    std::fs::write(
        target.join(report_name),
        serde_json::to_vec_pretty(solution)?,
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;
    use std::path::PathBuf;

    fn synthetic_images() -> (RgbaImage, RgbaImage) {
        let background = RgbaImage::from_fn(140, 40, |x, y| {
            let value = if (125..135).contains(&x) {
                230
            } else {
                (x + y) as u8
            };
            Rgba([value, value, value, 255])
        });
        let piece = RgbaImage::from_fn(10, 20, |x, y| {
            if (2..8).contains(&x) && (2..18).contains(&y) {
                Rgba([120, 120, 120, 255])
            } else {
                Rgba([0, 0, 0, 0])
            }
        });
        (background, piece)
    }

    #[test]
    fn returns_legacy_output_shape_and_offset_range() {
        let (background, piece) = synthetic_images();
        let solution = solve_images(&background, &piece, 100).unwrap();
        assert_eq!(solution.algorithm, "boundary_fusion_v1");
        assert_eq!(solution.min_offset, 100);
        assert_eq!(solution.max_offset, 130);
        assert!(solution.source_offset >= 100 && solution.source_offset <= 130);
        assert!(!solution.top_candidates.is_empty());
        assert_eq!(solution.weights.legacy, 0.35);
    }

    #[test]
    fn rejects_piece_larger_than_background() {
        let background = RgbaImage::new(10, 10);
        let piece = RgbaImage::new(20, 20);
        assert!(solve_images(&background, &piece, 0).is_err());
    }

    #[test]
    fn rejects_invalid_offset_range() {
        let (background, piece) = synthetic_images();
        assert!(solve_images(&background, &piece, 131).is_err());
    }

    #[test]
    #[ignore = "opt-in comparison against captured live images"]
    fn live_slider_replay() {
        let Some(directory) = std::env::var_os("QWEN_REPLAY_DIR").map(PathBuf::from) else {
            return;
        };
        let solution = solve_slider(
            &directory.join("fresh_back.png"),
            &directory.join("fresh_shadow.png"),
        )
        .unwrap();
        println!("live slider offset: {}", solution.source_offset);
        if let Some(expected) = std::env::var("QWEN_EXPECTED_OFFSET")
            .ok()
            .and_then(|value| value.parse::<u32>().ok())
        {
            assert_eq!(solution.source_offset, expected);
        }
    }

    #[test]
    fn euclidean_distance_finds_nearest_edge() {
        let mut edges = GrayImage::new(5, 1);
        edges.put_pixel(4, 0, Luma([255]));
        assert_eq!(distance_transform_l2(&edges)[0], 4.0);
    }
}
