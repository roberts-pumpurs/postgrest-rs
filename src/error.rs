use std::fmt;

use reqwest::{header::HeaderMap, StatusCode, Url};
use rp_postgrest_error::{DecodeError, PostgrestError};

/// HTTP response metadata retained for checked execution failures.
#[derive(Clone, Debug)]
pub struct ResponseMetadata {
    status: StatusCode,
    headers: HeaderMap,
    url: Url,
}

impl ResponseMetadata {
    pub(crate) fn from_response(response: &reqwest::Response) -> Self {
        Self {
            status: response.status(),
            headers: response.headers().clone(),
            url: response.url().clone(),
        }
    }

    /// Returns the authoritative HTTP response status.
    #[must_use]
    pub const fn status(&self) -> StatusCode {
        self.status
    }

    /// Returns all HTTP response headers.
    #[must_use]
    pub const fn headers(&self) -> &HeaderMap {
        &self.headers
    }

    /// Returns the effective response URL after redirects.
    #[must_use]
    pub const fn url(&self) -> &Url {
        &self.url
    }
}

/// Error returned by [`crate::Builder::execute_checked`].
///
/// Unlike [`crate::Builder::execute`], checked execution distinguishes failures
/// to send or read a request from structured and malformed PostgREST errors.
#[derive(Debug)]
#[non_exhaustive]
pub enum ExecuteError {
    /// The request failed before an HTTP response was available.
    Request(reqwest::Error),

    /// Reading the body of a non-success HTTP response failed.
    ResponseBody {
        /// Metadata retained from the HTTP response.
        metadata: ResponseMetadata,
        /// The body read failure.
        source: reqwest::Error,
    },

    /// PostgREST returned a structured error response.
    Postgrest {
        /// Metadata retained from the HTTP response.
        metadata: ResponseMetadata,
        /// The structured PostgREST error.
        source: PostgrestError,
    },

    /// A non-success response was not a valid structured PostgREST error.
    Decode {
        /// Metadata retained from the HTTP response.
        metadata: ResponseMetadata,
        /// The lossless decode failure.
        source: DecodeError,
    },
}

impl ExecuteError {
    /// Returns metadata retained from an HTTP response, when one was received.
    #[must_use]
    pub const fn response_metadata(&self) -> Option<&ResponseMetadata> {
        match self {
            Self::Request(_) => None,
            Self::ResponseBody { metadata, .. }
            | Self::Postgrest { metadata, .. }
            | Self::Decode { metadata, .. } => Some(metadata),
        }
    }

    /// Returns the authoritative HTTP status when one was observed.
    #[must_use]
    pub fn status(&self) -> Option<StatusCode> {
        self.response_metadata()
            .map(ResponseMetadata::status)
            .or_else(|| match self {
                Self::Request(source) => source.status(),
                _ => None,
            })
    }

    /// Returns the effective response URL, or the request URL retained by a
    /// request failure.
    #[must_use]
    pub fn url(&self) -> Option<&Url> {
        self.response_metadata()
            .map(ResponseMetadata::url)
            .or_else(|| match self {
                Self::Request(source) => source.url(),
                _ => None,
            })
    }
}

impl fmt::Display for ExecuteError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Request(source) => {
                write!(formatter, "failed to execute PostgREST request: {source}")
            }
            Self::ResponseBody { metadata, source } => write!(
                formatter,
                "failed to read PostgREST response body with status {}: {source}",
                metadata.status()
            ),
            Self::Postgrest { source, .. } => source.fmt(formatter),
            Self::Decode { source, .. } => source.fmt(formatter),
        }
    }
}

impl std::error::Error for ExecuteError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Request(source) => Some(source),
            Self::ResponseBody { source, .. } => Some(source),
            Self::Postgrest { source, .. } => Some(source),
            Self::Decode { source, .. } => Some(source),
        }
    }
}
