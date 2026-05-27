# Twelve factor webapp crate

`twelve` is a support crate for creating twelve-factor webapps with [`axum`](https://docs.rs/axum/latest/axum/).

## Features

* `shutdown_signal()` - Graceful shutdown handler for SIGTERM/SIGINT/SIGQUIT
* `RequestContext` - Axum extractor for reverse proxy `X-Script-Name` header
* `page::ErrorPage` - HTML error page rendering with error chain display
* `page::AppError` - Trait for mapping errors to HTTP status codes
* `page::RedirectOnSuccess` - POST-Redirect-GET pattern helper

## Removed features

* (0.2) `util::graceful_shutdown`: Moved to `twelve::shutdown_signal()`.
* (0.2) `util::as_opt_str`: Replace with `Option::as_deref()` (stable since Rust 1.40).
* (0.2) `from_env()`: Call `envy::from_env()` directly.
* (0.2) `page::Page`: Use `maud::Markup` directly (enable maud's `axum` feature).
* (0.2) `AppBuilder`: Too opinionated. Copy the pattern if needed.
* (0.2) `widgets`: Empty module removed.
