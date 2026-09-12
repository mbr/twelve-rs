# Changelog

## Unreleased

- Upgrade `sqlx` to `0.9`, `toml` to `1.1`, and `tower-http` to `0.7`, and
  refresh locked dependencies. Applications using PostgreSQL must also use
  `sqlx 0.9` to share database types with `twelve`.
- Raise the minimum supported Rust version to `1.94`.

- Disable unused dependency features in `axum`, `tokio`, `toml`, and `tracing`.
  Applications needing Axum's `form`, `json`, `query`, `matched-path`,
  `original-uri`, or `tower-log` features should enable them on their own
  `axum` dependency.
