# Twelve factor webapp crate

`twelve` is a support crate for creating twelve-factor webapps with [`axum`](https://docs.rs/axum/latest/axum/).

> **Warning:** Parts of this crate are still exploratory. Expect substantial API breakage between major versions.

## Features

* `config::from_args()` - Typed TOML configuration from a file or standard input
* `config::Core` - Reusable listener and tracing configuration
* `listener::Listener` - Transport-independent TCP and Unix listeners for Axum
* `systemd::ready()` - systemd readiness notification (`systemd` feature)
* `postgres::DatabaseUrl` - Validated, redacted PostgreSQL connection options (`postgres` feature)
* `shutdown_signal()` - Graceful shutdown handler for SIGTERM/SIGINT
* `RequestContext` - Axum extractor for reverse proxy `X-Script-Name` header
* `page::ErrorPage` - HTML error page rendering with error chain display
* `page::AppError` - Trait for mapping errors to HTTP status codes
* `page::RedirectOnSuccess` - POST-Redirect-GET pattern helper

## Configuration

The twelve-factor methodology conventionally places deployment configuration in environment variables. Structured TOML configuration technically departs from that prescription, but preserves its central separation between configuration, source code, and application builds. A deployment can generate or mount a TOML file and pass its path as the application's sole argument; `-` reads the document from standard input.

TOML provides a direct representation for nested structures, collections, and quoted values. Applications whose configuration maps cleanly to flat environment variables may prefer [`envy`](https://docs.rs/envy) instead.

Applications can flatten `config::Core` into their own Serde configuration to reuse validated listener and tracing filter fields without changing the TOML structure. The optional `postgres` feature provides a validated PostgreSQL URL that keeps credentials out of diagnostic output.

## Listeners and readiness

`listener::Listener::bind()` binds configured TCP and Unix addresses for Axum. Once all startup work is complete, `systemd::ready()` or `systemd::ready_with_status()` can report readiness when the default `systemd` feature is enabled. Disable default features to omit systemd integration and its dependency.

## Removed features

* (0.2) `util::graceful_shutdown`: Moved to `twelve::shutdown_signal()`.
* (0.2) `util::as_opt_str`: Replace with `Option::as_deref()` (stable since Rust 1.40).
* (0.2) `from_env()`: Call `envy::from_env()` directly.
* (0.2) `page::Page`: Use `maud::Markup` directly (enable maud's `axum` feature).
* (0.2) `AppBuilder`: Too opinionated. Copy the pattern if needed.
* (0.2) `widgets`: Empty module removed.
