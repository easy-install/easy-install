//! Crate-wide error type built on [`thiserror`].
//!
//! This replaces `anyhow` with an explicit error enum. The [`Result`] alias
//! defaults to [`Error`], and the [`Context`] trait provides `anyhow`-style
//! `.context(..)` on both `Result` and `Option`. The [`err!`] and [`bail!`]
//! macros replace `anyhow!` / `bail!`.

use thiserror::Error;

/// Crate-wide result alias.
pub type Result<T, E = Error> = std::result::Result<T, E>;

/// Crate-wide error type.
#[derive(Debug, Error)]
pub enum Error {
    /// A plain error message (replaces `anyhow!("...")`).
    #[error("{0}")]
    Msg(String),

    /// A context message attached to an underlying error
    /// (replaces `anyhow::Context::context`).
    #[error("{context}: {inner}")]
    WithContext {
        /// Human-readable description of the failed operation.
        context: String,
        /// The underlying error.
        inner: Box<dyn std::error::Error + Send + Sync + 'static>,
    },

    /// I/O error.
    #[error(transparent)]
    Io(#[from] std::io::Error),

    /// HTTP client error.
    #[error(transparent)]
    Reqwest(#[from] reqwest::Error),

    /// JSON (de)serialization error.
    #[error(transparent)]
    Json(#[from] serde_json::Error),

    /// Archive decode/encode error.
    #[error(transparent)]
    Archive(#[from] easy_archive::ArchiveError),

    /// Integer parsing error.
    #[error(transparent)]
    ParseInt(#[from] std::num::ParseIntError),

    /// Failed to join a spawned task.
    #[error(transparent)]
    Join(#[from] tokio::task::JoinError),
}

impl Error {
    /// Create a plain message error.
    pub fn msg(message: impl Into<String>) -> Self {
        Error::Msg(message.into())
    }
}

/// `anyhow`-style context extension for `Result` and `Option`.
pub trait Context<T> {
    /// Wrap the error with additional context.
    fn context(self, context: impl std::fmt::Display) -> Result<T>;

    /// Lazily wrap the error with additional context.
    fn with_context<F, D>(self, f: F) -> Result<T>
    where
        F: FnOnce() -> D,
        D: std::fmt::Display;
}

impl<T, E> Context<T> for std::result::Result<T, E>
where
    E: std::error::Error + Send + Sync + 'static,
{
    fn context(self, context: impl std::fmt::Display) -> Result<T> {
        self.map_err(|e| Error::WithContext {
            context: context.to_string(),
            inner: Box::new(e),
        })
    }

    fn with_context<F, D>(self, f: F) -> Result<T>
    where
        F: FnOnce() -> D,
        D: std::fmt::Display,
    {
        self.map_err(|e| Error::WithContext {
            context: f().to_string(),
            inner: Box::new(e),
        })
    }
}

impl<T> Context<T> for Option<T> {
    fn context(self, context: impl std::fmt::Display) -> Result<T> {
        self.ok_or_else(|| Error::msg(context.to_string()))
    }

    fn with_context<F, D>(self, f: F) -> Result<T>
    where
        F: FnOnce() -> D,
        D: std::fmt::Display,
    {
        self.ok_or_else(|| Error::msg(f().to_string()))
    }
}

/// Construct an [`Error::Msg`] (like `anyhow!`).
macro_rules! err {
    ($($arg:tt)*) => {
        $crate::error::Error::msg(format!($($arg)*))
    };
}

/// Return early with an [`Error::Msg`] (like `bail!`).
macro_rules! bail {
    ($($arg:tt)*) => {
        return Err($crate::error::Error::msg(format!($($arg)*)))
    };
}

pub(crate) use bail;
pub(crate) use err;
