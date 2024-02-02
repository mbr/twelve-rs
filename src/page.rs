use std::{env, fmt::Debug, iter, sync::OnceLock};

use axum::{
    body::Body,
    http::{header, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
};
use maud::{html, Markup, Render};

/// Whether or not to allow users that are being returned an error detailed insight.
static DETAILED_ERRORS: OnceLock<bool> = OnceLock::new();

/// An error message page.
///
/// Contains context from a given request informing how to build a response (e.g. whether or not a
/// user has permissions to view debug output).
#[derive(Debug)]
pub struct ErrorPage<E>(E);

impl<E> From<E> for ErrorPage<E> {
    #[inline(always)]
    fn from(value: E) -> Self {
        Self(value)
    }
}

impl<E> Render for ErrorPage<E>
where
    E: AppError,
{
    fn render(&self) -> Markup {
        let detailed_errors = *DETAILED_ERRORS.get_or_init(|| {
            let debug_var = env::var("DEBUG").unwrap_or_default();
            let debug_trimmed = debug_var.trim();

            debug_trimmed == "true" || debug_trimmed == "1"
        });

        let details = || {
            let mut cur: Option<&dyn std::error::Error> = Some(&self.0);
            let errors = iter::from_fn(move || {
                let next_err = cur?;
                cur = next_err.source();
                Some(next_err)
            });

            html! {
                @for err in errors {
                    hr;
                    pre { (err) }
                }
            }
        };

        html! {
            DOCTYPE;

            html {
                body {
                    h1 {
                        (self.0.status_code())
                    }
                    @if detailed_errors || self.0.user_visible() {
                        (details())
                    }
                }
            }
        }
    }
}

impl<E> IntoResponse for ErrorPage<E>
where
    E: AppError,
{
    #[inline(always)]
    fn into_response(self) -> Response {
        Response::builder()
            .status(self.0.status_code())
            .header(
                header::CONTENT_TYPE,
                HeaderValue::from_static("text/html; charset=utf-8"),
            )
            .body(Body::from(self.render().into_string()))
            .expect("should never fail to build response from string")
    }
}

pub trait AppError: Debug + std::error::Error {
    /// The error code associated with a given error.
    fn status_code(&self) -> StatusCode {
        StatusCode::INTERNAL_SERVER_ERROR
    }

    /// Whether or not details about the error should be displayed to regular users.
    fn user_visible(&self) -> bool {
        false
    }
}
