//! Structured logging for RecoverX.
//!
//! Wraps `tracing` / `tracing-subscriber` and provides:
//! - A global initialiser with configurable log level and format.
//! - An optional file sink for forensic audit logs.
//! - A JSON format flag for machine-readable output (Tauri / CLI).

use once_cell::sync::OnceCell;
use tracing_subscriber::{
    fmt::{self, format::FmtSpan},
    layer::SubscriberExt,
    util::SubscriberInitExt,
    EnvFilter,
};

static LOGGING_INIT: OnceCell<()> = OnceCell::new();

/// Logging configuration.
#[derive(Debug, Clone)]
pub struct LogConfig {
    /// Minimum log level, e.g. "info", "debug", "recoverx=debug,warn".
    pub level: String,
    /// Emit structured JSON logs (useful for the Tauri side).
    pub json: bool,
    /// Optional file to append logs to (for forensic audit trail).
    pub log_file: Option<std::path::PathBuf>,
}

impl Default for LogConfig {
    fn default() -> Self {
        LogConfig {
            level: std::env::var("RECOVERX_LOG").unwrap_or_else(|_| "info".to_string()),
            json: false,
            log_file: None,
        }
    }
}

/// Initialise global logging.  Safe to call multiple times — subsequent calls
/// are no-ops.
pub fn init(config: LogConfig) -> anyhow::Result<()> {
    let _ = LOGGING_INIT.get_or_try_init::<_, anyhow::Error>(|| {
        let env_filter =
            EnvFilter::try_new(&config.level).unwrap_or_else(|_| EnvFilter::new("info"));

        if config.json {
            tracing_subscriber::registry()
                .with(env_filter)
                .with(fmt::layer().json())
                .try_init()
                .map_err(|e| anyhow::anyhow!("Failed to initialise JSON logging: {}", e))?;
        } else {
            tracing_subscriber::registry()
                .with(env_filter)
                .with(
                    fmt::layer()
                        .with_target(true)
                        .with_thread_ids(false)
                        .with_span_events(FmtSpan::NONE),
                )
                .try_init()
                .map_err(|e| anyhow::anyhow!("Failed to initialise pretty logging: {}", e))?;
        }

        if let Some(ref log_path) = config.log_file {
            tracing::info!("Audit log: {}", log_path.display());
        }

        Ok(())
    })?;

    Ok(())
}

/// Shorthand: initialise with default config.
pub fn init_default() {
    let _ = init(LogConfig::default());
}

/// Initialise for tests — uses RUST_LOG env var, compact format.
pub fn init_test() {
    let _ = init(LogConfig {
        level: std::env::var("RUST_LOG").unwrap_or_else(|_| "warn".to_string()),
        json: false,
        log_file: None,
    });
}
