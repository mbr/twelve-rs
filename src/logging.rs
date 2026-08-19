//! Initializes process-wide application logging.
//!
//! [`init()`] installs a human-readable [`tracing`] subscriber configured by a
//! validated [`LogFilter`]. Events emitted through the
//! [`log`](https://docs.rs/log) facade are forwarded to the same subscriber.
//!
//! This is the opinionated default for applications that do not need custom
//! subscriber layers or output formats. Applications requiring structured
//! output or additional layers can instead convert [`LogFilter`] into a
//! [`tracing_subscriber::EnvFilter`] and construct their own subscriber.
//!
//! ```no_run
//! use twelve::{config::LogFilter, logging};
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let filter: LogFilter = "myapp=info,tower_http=warn".parse()?;
//! logging::init(filter)?;
//! # Ok(())
//! # }
//! ```

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
