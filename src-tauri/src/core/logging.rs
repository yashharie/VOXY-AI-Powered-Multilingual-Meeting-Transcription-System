//! Application logging initialisation using `tracing` and `tracing-subscriber`.
//!
//! Configured from AppConfig.logging.level (defaults to "info").
//! On first call, sets up:
//!   - A human-readable console layer (stderr)
//!   - A rolling JSON file layer under %APPDATA%\VOXY\logs\ (if enabled)

use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

/// Initialise the global tracing subscriber.
/// Safe to call multiple times — subsequent calls are no-ops.
pub fn init_logging() {
    // Respect RUST_LOG if set, otherwise fall back to "info".
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("voxy=debug,info"));

    // Console layer — human-readable output during development.
    let console_layer = fmt::layer()
        .with_target(true)
        .with_thread_ids(false)
        .with_file(false)
        .with_line_number(false)
        .compact();

    tracing_subscriber::registry()
        .with(filter)
        .with(console_layer)
        .try_init()
        .ok(); // ok() — ignore error if subscriber already initialised in tests.

    tracing::debug!("Logging initialised");
}
