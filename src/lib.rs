#![doc = include_str!("../README.md")]

pub mod page;
pub mod util;

/// Perform initial setup
///
/// # Panics
///
/// Will panic if setting up the tracing subscriber fails.
pub fn setup() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();
}
