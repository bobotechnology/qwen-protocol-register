use qwen_protocol_register::batch::run_batch;
use qwen_protocol_register::config::{AppConfig, parse_args, usage};
use qwen_protocol_register::inspector::{inspect_capture, inspect_probe, inspect_sparse};
use qwen_protocol_register::logging::init_logging;
use std::path::Path;

fn inspect(command: &str, path: &Path) -> Result<String, String> {
    let text =
        std::fs::read_to_string(path).map_err(|error| format!("{}: {error}", path.display()))?;
    let result = match command {
        "capture" => inspect_capture(&text),
        "probe" => inspect_probe(&text),
        "sparse" => inspect_sparse(&text),
        _ => Err(format!("unknown inspector command: {command}")),
    }?;
    serde_json::to_string_pretty(&result).map_err(|error| error.to_string())
}

fn inspector_usage() -> &'static str {
    "Usage: qwen-protocol-register <capture|probe|sparse> <input-file>"
}

async fn run(args: &[String]) -> Result<Option<String>, String> {
    if let Some(command) = args.get(1).map(String::as_str)
        && matches!(command, "capture" | "probe" | "sparse")
    {
        if args.len() != 3 {
            return Err(inspector_usage().into());
        }
        return inspect(command, Path::new(&args[2])).map(Some);
    }
    let options = parse_args(&args[1..]).map_err(|error| error.to_string())?;
    if options.help {
        return Ok(Some(usage().to_string()));
    }

    // Load config from optional YAML file, then init logging
    let config = AppConfig::load(options.config_file.as_deref())
        .map_err(|error| error.to_string())?;
    init_logging(&config.logging).map_err(|error| error.to_string())?;

    tracing::info!(?config, "configuration loaded");

    let report = run_batch(options)
        .await
        .map_err(|error| error.to_string())?;
    if report.failed > 0 {
        return Err(format!(
            "batch completed with {} failed task(s)",
            report.failed
        ));
    }
    Ok(None)
}

#[tokio::main]
async fn main() {
    let args = std::env::args().collect::<Vec<_>>();
    let inspector = args
        .get(1)
        .is_some_and(|command| matches!(command.as_str(), "capture" | "probe" | "sparse"));
    match run(&args).await {
        Ok(Some(output)) => println!("{output}"),
        Ok(None) => {}
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(if inspector { 2 } else { 1 });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    #[tokio::test]
    async fn inspector_reports_usage_for_missing_path() {
        let args = vec!["program".into(), "capture".into()];
        assert_eq!(run(&args).await.unwrap_err(), inspector_usage());
    }

    #[tokio::test]
    async fn help_uses_registration_cli() {
        let args = vec!["program".into(), "--help".into()];
        assert!(
            run(&args)
                .await
                .unwrap()
                .unwrap()
                .contains("--guerrilla-mail")
        );
    }

    #[test]
    fn inspector_returns_json_without_raw_input() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("capture.jsonl");
        std::fs::write(
            &path,
            r#"{"type":"request","method":"POST","url":"https://example.test/private"}"#,
        )
        .unwrap();
        let output = inspect("capture", &path).unwrap();
        let value: Value = serde_json::from_str(&output).unwrap();
        assert_eq!(value["records"], 1);
        assert!(!output.contains("private"));
    }
}
