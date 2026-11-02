use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const DEFAULT_TIMEOUT_MS: u64 = 180_000;
pub const STAGGER_MS: u64 = 500;

/// Runtime configuration loaded from config.yaml + environment overrides.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub registration: RegistrationConfig,
    pub network: NetworkConfig,
    pub mail: MailConfig,
    pub captcha: CaptchaConfig,
    pub output: OutputConfig,
    pub logging: LoggingConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistrationConfig {
    pub default_workers: usize,
    pub default_total: usize,
    pub timeout_ms: u64,
    pub stagger_ms: u64,
    pub cleanup_on_success: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkConfig {
    pub proxy: Option<String>,
    pub proxy_file: Option<PathBuf>,
    pub request_timeout_secs: u64,
    pub max_retries: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MailConfig {
    pub guerrilla_enabled: bool,
    pub default_domain: String,
    pub otp_timeout_ms: u64,
    pub otp_poll_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaptchaConfig {
    pub max_attempts: u32,
    pub feilin_wait_ms: u64,
    pub fingerprint_randomize: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutputConfig {
    pub results_file: PathBuf,
    pub artifact_dir: PathBuf,
    pub save_raw_email: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingConfig {
    pub level: String,
    pub file: Option<PathBuf>,
    pub json: bool,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            registration: RegistrationConfig {
                default_workers: 2,
                default_total: 1,
                timeout_ms: DEFAULT_TIMEOUT_MS,
                stagger_ms: STAGGER_MS,
                cleanup_on_success: false,
            },
            network: NetworkConfig {
                proxy: None,
                proxy_file: None,
                request_timeout_secs: 60,
                max_retries: 3,
            },
            mail: MailConfig {
                guerrilla_enabled: true,
                default_domain: "luckfeed.com".into(),
                otp_timeout_ms: 120_000,
                otp_poll_ms: 5_000,
            },
            captcha: CaptchaConfig {
                max_attempts: 3,
                feilin_wait_ms: 8000,
                fingerprint_randomize: true,
            },
            output: OutputConfig {
                results_file: PathBuf::from("batch_results.json"),
                artifact_dir: PathBuf::from("src/output"),
                save_raw_email: true,
            },
            logging: LoggingConfig {
                level: "info".into(),
                file: None,
                json: false,
            },
        }
    }
}

impl AppConfig {
    /// Load config from optional YAML file, then apply environment overrides.
    pub fn load(path: Option<&Path>) -> Result<Self> {
        let mut config = if let Some(path) = path {
            let text = std::fs::read_to_string(path)
                .map_err(|e| anyhow::anyhow!("read config {}: {e}", path.display()))?;
            serde_yaml::from_str(&text)
                .map_err(|e| anyhow::anyhow!("parse config {}: {e}", path.display()))?
        } else {
            Self::default()
        };
        config.apply_env_overrides();
        config.validate()?;
        Ok(config)
    }

    fn apply_env_overrides(&mut self) {
        // Network
        if let Ok(v) = std::env::var("PIPELINE_PROXY")
            && !v.is_empty()
        {
            self.network.proxy = Some(v);
        }
        if let Ok(v) = std::env::var("PIPELINE_PROXY_FILE")
            && !v.is_empty()
        {
            self.network.proxy_file = Some(PathBuf::from(v));
        }
        // Mail
        if let Ok(v) = std::env::var("TEMP_MAIL_TIMEOUT_MS")
            && let Ok(ms) = v.parse()
        {
            self.mail.otp_timeout_ms = ms;
        }
        if let Ok(v) = std::env::var("TEMP_MAIL_POLL_MS")
            && let Ok(ms) = v.parse()
        {
            self.mail.otp_poll_ms = ms;
        }
        // Captcha
        if let Ok(v) = std::env::var("PROBE_FEILIN_WAIT_MS")
            && let Ok(ms) = v.parse()
        {
            self.captcha.feilin_wait_ms = ms;
        }
        // Aliyun credentials are read directly from env in pipeline.rs
        // (not stored in config file for security).
        // Logging
        if let Ok(v) = std::env::var("RUST_LOG") {
            self.logging.level = v;
        }
    }

    fn validate(&self) -> Result<()> {
        if self.registration.default_workers == 0 {
            bail!("registration.default_workers must be >= 1");
        }
        if self.registration.timeout_ms < 1000 {
            bail!("registration.timeout_ms must be >= 1000");
        }
        if self.mail.otp_poll_ms < 100 {
            bail!("mail.otp_poll_ms must be >= 100");
        }
        if self.captcha.max_attempts == 0 {
            bail!("captcha.max_attempts must be >= 1");
        }
        if !matches!(
            self.logging.level.to_ascii_lowercase().as_str(),
            "trace" | "debug" | "info" | "warn" | "error"
        ) {
            bail!(
                "logging.level must be one of trace/debug/info/warn/error, got: {}",
                self.logging.level
            );
        }
        Ok(())
    }
}

/// CLI options parsed from command-line arguments.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BatchOptions {
    pub total: usize,
    pub total_explicit: bool,
    pub workers: usize,
    pub guerrilla_mail: bool,
    pub proxy: Option<String>,
    pub proxy_file: Option<PathBuf>,
    pub emails: Vec<String>,
    pub domain: String,
    pub output: PathBuf,
    pub timeout_ms: u64,
    pub cleanup: bool,
    pub help: bool,
    pub config_file: Option<PathBuf>,
}

impl Default for BatchOptions {
    fn default() -> Self {
        Self {
            total: 1,
            total_explicit: false,
            workers: 1,
            guerrilla_mail: false,
            proxy: None,
            proxy_file: None,
            emails: Vec::new(),
            domain: "luckfeed.com".into(),
            output: std::env::current_dir()
                .unwrap_or_else(|_| PathBuf::from("."))
                .join("batch_results.json"),
            timeout_ms: DEFAULT_TIMEOUT_MS,
            cleanup: false,
            help: false,
            config_file: None,
        }
    }
}

pub fn usage() -> &'static str {
    r#"Usage: qwen-protocol-register [options]

Registration:
  -n, --total N            Total accounts to register (default: 1)
  -w, -c, --workers N      Concurrent workers (default: 1)
      --emails A,B,C       Explicit email list (comma-separated)
      --domain DOMAIN      Generated email domain (default: luckfeed.com)

Network:
      --proxy URL          Use one proxy for all workers
      --proxy-file FILE    Use proxy URLs from a file, round-robin

Runtime:
      --guerrilla-mail     Create disposable mailboxes and read OTP codes
      --output FILE        Results file (default: batch_results.json)
      --timeout MS         Per-task timeout (default: 180000)
      --cleanup            Remove per-worker temp directories after success
      --config FILE        Load YAML config file (default: none)
  -h, --help               Show this help"#
}

fn required_value<'a>(args: &'a [String], index: usize, option: &str) -> Result<&'a str> {
    let value = args
        .get(index + 1)
        .ok_or_else(|| anyhow::anyhow!("{option} requires a value"))?;
    if value.starts_with('-') {
        bail!("{option} requires a value");
    }
    Ok(value)
}

fn positive_integer(value: &str, option: &str) -> Result<u64> {
    let number = value
        .parse::<f64>()
        .ok()
        .filter(|number| number.is_finite() && number.fract() == 0.0 && *number >= 1.0)
        .filter(|number| *number <= 9_007_199_254_740_991.0)
        .ok_or_else(|| anyhow::anyhow!("{option} must be a positive integer"))?;
    Ok(number as u64)
}

pub fn parse_args(args: &[String]) -> Result<BatchOptions> {
    let mut options = BatchOptions::default();
    let mut index = 0;
    while index < args.len() {
        let arg = args[index].as_str();
        match arg {
            "-h" | "--help" => options.help = true,
            "-n" | "--total" => {
                options.total = positive_integer(required_value(args, index, arg)?, arg)? as usize;
                options.total_explicit = true;
                index += 1;
            }
            "-w" | "-c" | "--workers" => {
                options.workers =
                    positive_integer(required_value(args, index, arg)?, arg)? as usize;
                index += 1;
            }
            "--guerrilla-mail" => options.guerrilla_mail = true,
            "--proxy" => {
                options.proxy = Some(required_value(args, index, arg)?.to_string());
                index += 1;
            }
            "--proxy-file" => {
                options.proxy_file = Some(PathBuf::from(required_value(args, index, arg)?));
                index += 1;
            }
            "--emails" => {
                options.emails = required_value(args, index, arg)?
                    .split(',')
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(str::to_string)
                    .collect();
                index += 1;
            }
            "--domain" => {
                options.domain = required_value(args, index, arg)?
                    .trim()
                    .trim_start_matches('@')
                    .to_string();
                index += 1;
            }
            "--output" => {
                options.output = PathBuf::from(required_value(args, index, arg)?);
                index += 1;
            }
            "--timeout" => {
                options.timeout_ms = positive_integer(required_value(args, index, arg)?, arg)?;
                index += 1;
            }
            "--cleanup" => options.cleanup = true,
            "--config" => {
                options.config_file = Some(PathBuf::from(required_value(args, index, arg)?));
                index += 1;
            }
            _ => bail!("unknown option: {arg}"),
        }
        index += 1;
    }

    if options.proxy.is_some() && options.proxy_file.is_some() {
        bail!("--proxy and --proxy-file cannot be used together");
    }
    if options.guerrilla_mail && !options.emails.is_empty() {
        bail!("--guerrilla-mail and --emails cannot be used together");
    }
    if options.domain.is_empty()
        || options
            .domain
            .chars()
            .any(|character| character.is_whitespace() || character == '/' || character == '@')
    {
        bail!("--domain must be a valid domain name");
    }
    if !options.emails.is_empty() && !options.total_explicit {
        options.total = options.emails.len();
    }
    if !options.emails.is_empty() && options.total > options.emails.len() {
        bail!("--total cannot exceed the number of --emails entries");
    }
    options.workers = options.workers.min(options.total);
    Ok(options)
}

pub fn load_proxies(options: &BatchOptions) -> Result<Vec<String>> {
    if let Some(proxy) = &options.proxy {
        return Ok(vec![proxy.clone()]);
    }
    let Some(file) = &options.proxy_file else {
        return Ok(Vec::new());
    };
    let text = std::fs::read_to_string(file)
        .map_err(|error| format!("{}: {error}", file.display()))
        .map_err(anyhow::Error::msg)?;
    let proxies = text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(str::to_string)
        .collect::<Vec<_>>();
    if proxies.is_empty() {
        bail!("proxy file is empty: {}", file.display());
    }
    Ok(proxies)
}

pub fn absolute_path(path: &Path) -> Result<PathBuf> {
    if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        Ok(std::env::current_dir()?.join(path))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_string()).collect()
    }

    #[test]
    fn preserves_legacy_defaults_and_aliases() {
        let options = parse_args(&args(&["-c", "4", "-n", "2"])).unwrap();
        assert_eq!(options.total, 2);
        assert_eq!(options.workers, 2);
        assert_eq!(options.domain, "luckfeed.com");
        assert_eq!(options.timeout_ms, 180_000);
    }

    #[test]
    fn derives_total_from_emails() {
        let options = parse_args(&args(&["--emails", "a@example.test, b@example.test"])).unwrap();
        assert_eq!(options.total, 2);
        assert_eq!(options.emails.len(), 2);
    }

    #[test]
    fn rejects_legacy_conflicts() {
        assert!(
            parse_args(&args(&[
                "--proxy",
                "http://localhost",
                "--proxy-file",
                "p.txt"
            ]))
            .is_err()
        );
        assert!(parse_args(&args(&["--guerrilla-mail", "--emails", "a@example.test"])).is_err());
        assert!(parse_args(&args(&["--emails", "a@example.test", "--total", "2"])).is_err());
        assert!(parse_args(&args(&["--domain", "@bad/domain"])).is_err());
    }

    #[test]
    fn accepts_javascript_style_integer_numbers() {
        let options = parse_args(&args(&["--total", "1.0", "--workers", "1e0"])).unwrap();
        assert_eq!(options.total, 1);
        assert_eq!(options.workers, 1);
    }

    #[test]
    fn default_config_is_valid() {
        let config = AppConfig::default();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn config_rejects_invalid_log_level() {
        let mut config = AppConfig::default();
        config.logging.level = "verbose".into();
        assert!(config.validate().is_err());
    }
}
