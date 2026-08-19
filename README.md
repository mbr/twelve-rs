# Twelve factor webapp crate

`twelve` provides small, composable building blocks for twelve-factor web
applications built with [`axum`](https://docs.rs/axum/latest/axum/). It covers
common process boundaries without prescribing application routing, database
policy, or deployment topology.

> **Warning:** Parts of this crate are still exploratory. Expect substantial API breakage between major versions.

Start with the major modules:

* [`config`] loads typed TOML configuration and provides reusable validated settings.
* [`listener`] binds application-owned TCP or Unix listeners for Axum.
* [`mount`] builds links for applications mounted below a reverse-proxy path prefix.
* [`page`] provides conventional server-rendered HTML responses with the optional `html` feature.
* [`postgres`] validates and redacts PostgreSQL connection configuration with the optional `postgres` feature.
* [`shutdown`] waits for conventional process signals to coordinate graceful termination.
