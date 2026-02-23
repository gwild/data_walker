//! Rotating log system
//!
//! Logs to both console and rotating files in ./logs/

use std::path::Path;
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

/// Initialize the logging system with rotating file logs
pub fn init_logging(log_dir: &str) {
    // Create log directory if it doesn't exist
    let log_path = Path::new(log_dir);
    if !log_path.exists() {
        std::fs::create_dir_all(log_path).expect("Failed to create log directory");
    }

    // Rotating file appender - rotates daily, keeps files named data_walker.YYYY-MM-DD.log
    let file_appender = RollingFileAppender::new(Rotation::DAILY, log_dir, "data_walker.log");
    let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);

    // Keep the guard alive for the lifetime of the program
    // We leak it intentionally since logging should last the whole program
    std::mem::forget(_guard);

    // Environment filter - default to INFO, can be overridden with RUST_LOG
    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,data_walker=debug,tower_http=debug"));

    // Console layer - pretty printed for terminal
    let console_layer = fmt::layer()
        .with_target(true)
        .with_thread_ids(false)
        .with_file(true)
        .with_line_number(true);

    // File layer - JSON format for easier parsing
    let file_layer = fmt::layer()
        .with_writer(non_blocking)
        .with_target(true)
        .with_thread_ids(true)
        .with_file(true)
        .with_line_number(true)
        .with_ansi(false);

    tracing_subscriber::registry()
        .with(env_filter)
        .with(console_layer)
        .with(file_layer)
        .init();

    tracing::info!("Logging initialized. Log directory: {}", log_dir);
}

/// Log a request (for manual logging in handlers)
#[macro_export]
macro_rules! log_request {
    ($method:expr, $path:expr) => {
        tracing::info!(method = %$method, path = %$path, "Request received");
    };
    ($method:expr, $path:expr, $($field:tt)*) => {
        tracing::info!(method = %$method, path = %$path, $($field)*, "Request received");
    };
}

/// Log an error with context
#[macro_export]
macro_rules! log_error {
    ($msg:expr) => {
        tracing::error!(error = %$msg, "Error occurred");
    };
    ($msg:expr, $($field:tt)*) => {
        tracing::error!(error = %$msg, $($field)*, "Error occurred");
    };
}
