use std::{env, fmt::Debug, sync::OnceLock};

use axum::{
    extract::Request,
    http::{Method, StatusCode, Uri},
    middleware::Next,
    response::{Html, IntoResponse, Redirect, Response},
};
use html_escape::encode_text;
use thiserror::Error;

/// Whether or not to allow users that are being returned an error detailed insight.
static DETAILED_ERRORS: OnceLock<bool> = OnceLock::new();

/// An error message page.
///
/// Renders a simple HTML error page with the status code and error chain.
/// Error details are shown if `DEBUG=1` or `DEBUG=true`, or if the error is user-visible.
#[derive(Debug)]
pub struct ErrorPage<E>(E);

impl<E> From<E> for ErrorPage<E> {
    #[inline(always)]
    fn from(value: E) -> Self {
        Self(value)
    }
}

impl<E> ErrorPage<E>
where
    E: AppError,
{
    fn render(&self) -> String {
        let detailed_errors = *DETAILED_ERRORS.get_or_init(|| {
            let debug_var = env::var("DEBUG").unwrap_or_default();
            let debug_trimmed = debug_var.trim();
            debug_trimmed == "true" || debug_trimmed == "1"
        });

        let mut html = format!(
            "<!DOCTYPE html><html><body><h1>{}</h1>",
            encode_text(&self.0.status_code().to_string())
        );

        if detailed_errors || self.0.user_visible() {
            let mut cur: Option<&dyn std::error::Error> = Some(&self.0);
            while let Some(err) = cur {
                html.push_str(&format!("<hr><pre>{}</pre>", encode_text(&err.to_string())));
                cur = err.source();
            }
        }

        html.push_str("</body></html>");
        html
    }
}

impl<E> IntoResponse for ErrorPage<E>
where
    E: AppError,
{
    fn into_response(self) -> Response {
        (self.0.status_code(), Html(self.render())).into_response()
    }
}

pub trait AppError: Debug + std::error::Error {
    /// The HTTP status code for this error.
    #[inline(always)]
    fn status_code(&self) -> StatusCode {
        StatusCode::INTERNAL_SERVER_ERROR
    }

    /// Whether error details should be shown to users (even without DEBUG).
    #[inline(always)]
    fn user_visible(&self) -> bool {
        self.status_code() != StatusCode::INTERNAL_SERVER_ERROR
    }
}

#[derive(Debug, Error)]
#[error("uri not found: {0}")]
pub struct NotFound(pub Uri);

impl NotFound {
    #[inline(always)]
    pub fn new(uri: Uri) -> Self {
        Self(uri)
    }

    #[inline(always)]
    pub async fn handler(uri: Uri) -> ErrorPage<Self> {
        ErrorPage(Self::new(uri))
    }
}

impl AppError for NotFound {
    #[inline(always)]
    fn status_code(&self) -> StatusCode {
        StatusCode::NOT_FOUND
    }
}

#[derive(Debug, Error)]
#[error("method not allowed: {0}")]
pub struct MethodNotAllowed(Method);

impl MethodNotAllowed {
    #[inline(always)]
    pub fn new(method: Method) -> Self {
        Self(method)
    }

    #[inline]
    pub async fn middleware(req: Request, next: Next) -> Response {
        let method = req.method().clone();
        let response = next.run(req).await;

        if response.status() == StatusCode::METHOD_NOT_ALLOWED {
            return ErrorPage(MethodNotAllowed::new(method)).into_response();
        }

        response
    }
}

impl AppError for MethodNotAllowed {
    #[inline(always)]
    fn status_code(&self) -> StatusCode {
        StatusCode::METHOD_NOT_ALLOWED
    }
}

/// Response type for POST handlers that redirect on success or render a page on error.
///
/// Implements the POST-Redirect-GET pattern.
#[derive(Debug)]
pub enum RedirectOnSuccess<T = Response> {
    Page(T),
    Redirect(Redirect),
}

impl<T> RedirectOnSuccess<T> {
    /// Return a page response (e.g., form with validation errors).
    #[inline(always)]
    pub fn page<E>(content: T) -> Result<Self, E> {
        Ok(RedirectOnSuccess::Page(content))
    }

    /// Return a redirect response (e.g., after successful form submission).
    #[inline(always)]
    pub fn redirect<E>(uri: &str) -> Result<Self, E> {
        Ok(RedirectOnSuccess::Redirect(Redirect::to(uri)))
    }
}

impl<T: IntoResponse> IntoResponse for RedirectOnSuccess<T> {
    #[inline(always)]
    fn into_response(self) -> Response {
        match self {
            RedirectOnSuccess::Page(page) => page.into_response(),
            RedirectOnSuccess::Redirect(redirect) => redirect.into_response(),
        }
    }
}
