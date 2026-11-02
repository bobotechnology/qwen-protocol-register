use crate::config::LoggingConfig;
use anyhow::Result;
use tracing_subscriber::{EnvFilter, fmt, layer::SubscriberExt, util::SubscriberInitExt};

/// Initialize tracing subscriber with optional file output and JSON formatting.
pub fn init_logging(config: &LoggingConfig) -> Result<()> {
    let env_filter = EnvFilter::try_from_default_env()
        .or_else(|_| EnvFilter::try_new(&config.level))?;

    if let Some(file_path) = &config.file {
        if let Some(parent) = file_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(file_path)?;
        let (non_blocking, _guard) = tracing_appender::non_blocking(file);
        // Note: _guard is leaked intentionally to keep the file writer alive
        // for the program lifetime. In a long-running service this would be
        // stored in a static or returned to the caller.
        std::mem::forget(_guard);

        if config.json {
            tracing_subscriber::registry()
                .with(env_filter)
                .with(fmt::layer().json().with_writer(non_blocking))
                .init();
        } else {
            tracing_subscriber::registry()
                .with(env_filter)
                .with(fmt::layer().with_writer(non_blocking))
                .init();
        }
    } else if config.json {
        tracing_subscriber::registry()
            .with(env_filter)
            .with(fmt::layer().json())
            .init();
    } else {
        tracing_subscriber::registry()
            .with(env_filter)
            .with(fmt::layer())
            .init();
    }

    Ok(())
}
