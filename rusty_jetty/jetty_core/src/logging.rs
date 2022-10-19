//! Logging utilities for Jetty-wide output to stdout.
//!

// Re-exports for convenience
pub use tracing::metadata::LevelFilter;
pub use tracing::{debug, error, info, warn};
use tracing_subscriber::{filter, prelude::*};
use tracing_subscriber::{util::SubscriberInitExt, Layer};

/// Set up basic logging
pub fn setup(level: Option<LevelFilter>) {
    // The caller can  specify a log level via `level`. If they don't, we
    // default to "info." This value is overridden by any env var levels.
    let level_filter = if let Some(level_filter) = level {
        level_filter
    } else {
        LevelFilter::INFO
    };

    // The user can specify a log level via an env var (such as for testing).
    // If they don't, then we default to the level_filter defined above.
    let env = std::env::var("RUST_LOG")
        .unwrap_or_else(|_| format!("{},tower_http=debug,hyper=info,reqwest=info", level_filter));

    let logging_layers = vec![tracing_subscriber::fmt::layer()
        // .with_filter(level_filter)
        .with_filter(tracing_subscriber::EnvFilter::new(env))
        .boxed()];

    // Actually initialize all logging layers
    tracing_subscriber::registry().with(logging_layers).init();

    debug!("logging set up");
}
