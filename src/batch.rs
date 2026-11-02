use crate::config::{BatchOptions, STAGGER_MS, absolute_path, load_proxies};
use crate::pipeline::{PipelineConfig, run_pipeline};
use crate::report::{BatchReport, TaskResult};
use anyhow::Result;
use chrono::{SecondsFormat, Utc};
use rand::{RngCore, thread_rng};
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

fn random_hex(length: usize) -> String {
    let mut bytes = vec![0u8; length.div_ceil(2)];
    thread_rng().fill_bytes(&mut bytes);
    hex::encode(bytes)[..length].to_string()
}

fn output_directory(index: usize) -> PathBuf {
    PathBuf::from("src").join("output").join(format!(
        "worker_{}_{}_{}",
        index + 1,
        Utc::now().timestamp_millis(),
        random_hex(6)
    ))
}

async fn run_task(
    index: usize,
    options: &BatchOptions,
    proxies: &[String],
) -> (TaskResult, PathBuf) {
    let name = format!("sandbox_{}", random_hex(8));
    let password = format!("Qw9!{}#R7", random_hex(20));
    let email = if options.guerrilla_mail {
        None
    } else {
        Some(
            options
                .emails
                .get(index)
                .cloned()
                .unwrap_or_else(|| format!("reg_{}@{}", random_hex(10), options.domain)),
        )
    };
    let proxy = if proxies.is_empty() {
        std::env::var("PIPELINE_PROXY")
            .ok()
            .filter(|value| !value.is_empty())
    } else {
        Some(proxies[index % proxies.len()].clone())
    };
    let output_dir = output_directory(index);
    let run = run_pipeline(PipelineConfig {
        fixtures_dir: std::env::var_os("FIXTURES_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("fixtures")),
        output_dir: output_dir.clone(),
        name,
        email,
        password,
        guerrilla_mail: options.guerrilla_mail,
        proxy,
        transport: None,
        mail_transport: None,
        timeout_ms: options.timeout_ms,
    })
    .await;
    (run.result, output_dir)
}

pub async fn run_batch(options: BatchOptions) -> Result<BatchReport> {
    if options.emails.len() > 1 {
        anyhow::bail!("manual OTP supports one --emails entry per run");
    }
    let proxies = load_proxies(&options)?;
    let started = Instant::now();
    let next_task = Arc::new(AtomicUsize::new(0));
    let completed = Arc::new(AtomicUsize::new(0));
    let results = Arc::new(Mutex::new(vec![None; options.total]));
    let output_directories = Arc::new(Mutex::new(Vec::new()));
    tracing::info!(
        total = options.total,
        workers = options.workers,
        "starting batch registration"
    );
    if !proxies.is_empty() {
        tracing::info!(count = proxies.len(), "loaded proxy endpoints");
    }
    let options = Arc::new(options);
    let proxies = Arc::new(proxies);
    let mut workers = Vec::new();
    for slot in 0..options.workers {
        let options = options.clone();
        let proxies = proxies.clone();
        let next_task = next_task.clone();
        let completed = completed.clone();
        let results = results.clone();
        let output_directories = output_directories.clone();
        workers.push(tokio::spawn(async move {
            if slot > 0 {
                tokio::time::sleep(Duration::from_millis(slot as u64 * STAGGER_MS)).await;
            }
            loop {
                let index = next_task.fetch_add(1, Ordering::Relaxed);
                if index >= options.total {
                    break;
                }
                let (result, output_dir) = run_task(index, &options, &proxies).await;
                let success = result.success();
                let finished = completed.fetch_add(1, Ordering::Relaxed) + 1;
                let identity = result
                    .email
                    .clone()
                    .unwrap_or_else(|| format!("task-{}", index + 1));
                let status = if result.error.is_none() { "OK" } else { "FAIL" };
                let verify = result.verify_code.as_deref().unwrap_or("N/A");
                tracing::info!(
                    progress = %format!("{}/{}", finished, options.total),
                    status,
                    email = %identity,
                    verify,
                    "task completed"
                );
                results.lock().expect("batch result lock")[index] = Some(result);
                output_directories
                    .lock()
                    .expect("batch output lock")
                    .push((output_dir, success));
            }
        }));
    }
    for worker in workers {
        worker.await?;
    }
    let results = results
        .lock()
        .expect("batch result lock")
        .iter()
        .map(|result| {
            result.clone().unwrap_or_else(|| TaskResult {
                error: Some("worker did not return a result".into()),
                ..TaskResult::default()
            })
        })
        .collect::<Vec<_>>();
    let report = BatchReport::new(
        Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true),
        started.elapsed().as_secs_f64(),
        results,
    );
    let output = absolute_path(&options.output)?;
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut bytes = serde_json::to_vec_pretty(&report)?;
    bytes.push(b'\n');
    std::fs::write(&output, bytes)?;
    tracing::info!(
        elapsed = %report.elapsed,
        total = report.total,
        success = report.success,
        failed = report.failed,
        "batch results"
    );
    for result in &report.results {
        let status = if result.error.is_none() { "OK" } else { "FAIL" };
        let email = result.email.as_deref().unwrap_or("(email unavailable)");
        let verify = result.verify_code.as_deref().unwrap_or("N/A");
        tracing::info!(status, email, verify, "result detail");
    }
    tracing::info!(path = %output.display(), "results written");
    if options.cleanup {
        for (directory, success) in output_directories.lock().expect("batch output lock").iter() {
            if *success && directory.exists() {
                std::fs::remove_dir_all(directory)?;
            }
        }
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn manual_otp_rejects_multiple_emails_before_network() {
        let options = BatchOptions {
            total: 2,
            emails: vec!["a@example.test".into(), "b@example.test".into()],
            ..BatchOptions::default()
        };
        assert!(run_batch(options).await.is_err());
    }

    #[test]
    fn generated_names_and_passwords_have_legacy_shape() {
        let name = format!("sandbox_{}", random_hex(8));
        let password = format!("Qw9!{}#R7", random_hex(20));
        assert_eq!(name.len(), 16);
        assert!(password.starts_with("Qw9!"));
        assert!(password.ends_with("#R7"));
        assert_eq!(password.len(), 27);
    }
}
