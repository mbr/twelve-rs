# Twelve factor webapp crate

`twelve` is a strongly opinionated support crate for creating twelve-factor webapps. Specifically, it is aimed at aiding the creation of "classic" HTML-based web applications that are modelled after the principles dictated by the [Twelve-Factor](https://12factor.net/) methodology.

It also is aimed at working with a fixed set of crate, most importantly [`axum`](https://docs.rs/axum/latest/axum/) and [`maud`](https://docs.rs/maud/latest/maud/). A scheme for organizing the code into an [MVP](https://en.wikipedia.org/wiki/Model%E2%80%93view%E2%80%93presenter)-like fasion is also included and strongly recommended.
