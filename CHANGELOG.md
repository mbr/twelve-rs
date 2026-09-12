# Changelog

## Unreleased

- Disable unused dependency features in `axum`, `tokio`, `toml`, and `tracing`.
  Applications needing Axum's `form`, `json`, `query`, `matched-path`,
  `original-uri`, or `tower-log` features should enable them on their own
  `axum` dependency.
