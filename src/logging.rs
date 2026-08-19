//! Initializes human-readable tracing output.

use tracing_subscriber::util::{SubscriberInitExt, TryInitError};

use crate::config::LogFilter;

/// Installs a global tracing subscriber using the validated filter.
///
/// Returns an error if a global tracing subscriber or compatible log adapter
/// has already been installed.
pub fn init(filter: LogFilter) -> Result<(), TryInitError> {
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .finish()
        .try_init()
}
