//! The library's error type. Every fallible public function returns
//! [`Result`]; nothing here ever formats a token or cookie into a message.
use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("no credentials: set ${token_var} and ${cookie_var}, or pass a credentials file")]
    MissingCredentials {
        token_var: String,
        cookie_var: String,
    },

    #[error("{method}: HTTP {status}")]
    Http { method: String, status: u16 },

    #[error("{method}: response is not JSON")]
    InvalidResponse {
        method: String,
        #[source]
        source: serde_json::Error,
    },

    #[error("{method}: response has no {pointer}")]
    UnexpectedShape { method: String, pointer: String },

    #[error("{method}: {error}")]
    Api { method: String, error: String },

    #[error("{method}: {error} -- the session token/cookie is likely stale; get a fresh one")]
    StaleCredentials { method: String, error: String },

    #[error("{method}: rate-limited after {attempts} attempts")]
    RateLimited { method: String, attempts: u32 },

    #[error("'{raw}' is not a Slack conversation id (C0123) or thread (C0123:1700000000.000100)")]
    InvalidThreadRef { raw: String },

    #[error("{0}")]
    Other(String),

    #[error(transparent)]
    Io(#[from] std::io::Error),

    /// Boxed: `reqwest::Error` is large enough that clippy flags an
    /// unboxed `Result<_, Error>` as a wide error type.
    #[error(transparent)]
    Reqwest(#[from] Box<reqwest::Error>),

    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

impl From<reqwest::Error> for Error {
    fn from(e: reqwest::Error) -> Self {
        Error::Reqwest(Box::new(e))
    }
}

pub type Result<T> = std::result::Result<T, Error>;
