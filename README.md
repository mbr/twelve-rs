# Twelve factor webapp crate

`twelve` is a strongly opinionated support crate for creating twelve-factor webapps. Specifically, it is aimed at aiding the creation of "classic" HTML-based web applications that are modelled after the principles dictated by the [Twelve-Factor](https://12factor.net/) methodology.

It also is aimed at working with a fixed set of crates, most importantly [`axum`](https://docs.rs/axum/latest/axum/) and [`maud`](https://docs.rs/maud/latest/maud/). A scheme for organizing the code into an [MVP](https://en.wikipedia.org/wiki/Model%E2%80%93view%E2%80%93presenter)-like fashion is also included and strongly recommended.

## Removed features

* (0.2) `util::graceful_shutdown`: Replace with `tokio::signal::unix::signal()` for SIGTERM/SIGINT handling.
* (0.2) `util::as_opt_str`: Replace with `Option::as_deref()` (stable since Rust 1.40).
* (0.2) `from_env()`: Call `envy::from_env()` directly.
