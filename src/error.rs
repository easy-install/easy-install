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
    ///
    /// `Display` only prints the context; use [`format_chain`] to render the
    /// complete `caused by:` chain. Previously the `Display` impl inlined the
    /// inner error, but `reqwest`'s own `Display` is generic ("error sending
    /// request for url (...)") and drops its sources, which made network
    /// failures on Android/Termux impossible to diagnose.
    #[error("{context}")]
    WithContext {
        /// Human-readable description of the failed operation.
        context: String,
        /// The underlying error.
        #[source]
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

/// Raw OS codes that mean "no space left on the device".
#[cfg(unix)]
const ENOSPC: i32 = 28;
#[cfg(windows)]
const ERROR_DISK_FULL: i32 = 112;
#[cfg(windows)]
const ERROR_HANDLE_DISK_FULL: i32 = 39;

/// Whether `err` (or any error in its source chain) reports a full disk.
///
/// The up-front disk-space guard cannot see every case — the filesystem can
/// fill between the check and the write, and remote hosts only report the
/// failure through the shell's stderr. Recognising the error turns a bare
/// "failed to write" into an actionable message on both paths.
pub fn is_out_of_space(err: &(dyn std::error::Error + 'static)) -> bool {
    let mut current = Some(err);
    while let Some(e) = current {
        if let Some(io) = e.downcast_ref::<std::io::Error>()
            && let Some(code) = io.raw_os_error()
        {
            #[cfg(unix)]
            if code == ENOSPC {
                return true;
            }
            #[cfg(windows)]
            if code == ERROR_DISK_FULL || code == ERROR_HANDLE_DISK_FULL {
                return true;
            }
            let _ = code;
        }

        // Remote hosts surface ENOSPC as text from `cat`/`dd`. Match the
        // common translations too, so a localized host still gets the hint.
        let msg = e.to_string().to_ascii_lowercase();
        if msg.contains("no space left")
            || msg.contains("disk full")
            || msg.contains("not enough space")
            || msg.contains("insufficient disk space")
        {
            return true;
        }

        current = e.source();
    }
    false
}

/// Message for a write that failed because the filesystem is full.
pub fn disk_full_message(path: &str, source: &(dyn std::error::Error + 'static)) -> String {
    format!(
        "not enough disk space to write '{path}'\n  {source}\n  Free some space, or install to another directory (--dir)."
    )
}

/// Render an error together with every underlying `source()`.
///
/// `reqwest` (and most async stacks) wrap the useful detail in nested
/// sources, e.g.:
///
/// ```text
/// send failed
///   caused by: error sending request for url (https://...)
///   caused by: client error (Connect)
///   caused by: tls handshake eof
/// ```
///
/// The nested lines are what actually identify Android/Termux networking
/// problems, so the CLI prints them instead of only the top-level message.
pub fn format_chain(err: &(dyn std::error::Error + 'static)) -> String {
    const MAX_DEPTH: usize = 8;

    let mut out = err.to_string();
    let mut source = err.source();
    let mut depth = 0;
    while let Some(e) = source {
        if depth >= MAX_DEPTH {
            out.push_str("\n  caused by: ...");
            break;
        }
        let msg = e.to_string();
        // Some chains repeat the parent message verbatim; skip duplicates.
        if !out.ends_with(&msg) {
            out.push_str("\n  caused by: ");
            out.push_str(&msg);
        }
        source = e.source();
        depth += 1;
    }
    out
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_out_of_space_by_message() {
        // What a remote `cat`/`dd` failure looks like after `sssh` wraps the
        // shell's stderr into an io::Error.
        let err = std::io::Error::other("cat: write error: No space left on device");
        assert!(is_out_of_space(&err));
        assert!(is_out_of_space(
            &std::io::Error::other("write failed: disk full")
        ));
        assert!(!is_out_of_space(&std::io::Error::other(
            "No such file or directory"
        )));
    }

    #[test]
    fn detects_out_of_space_by_os_code() {
        #[cfg(unix)]
        assert!(is_out_of_space(&std::io::Error::from_raw_os_error(28)));
        #[cfg(windows)]
        {
            assert!(is_out_of_space(&std::io::Error::from_raw_os_error(112)));
            assert!(is_out_of_space(&std::io::Error::from_raw_os_error(39)));
        }
    }

    #[test]
    fn walks_the_source_chain() {
        let inner = std::io::Error::other("No space left on device");
        let err = Error::WithContext {
            context: "failed to write".into(),
            inner: Box::new(inner),
        };
        assert!(is_out_of_space(&err));
    }

    #[test]
    fn disk_full_message_names_the_path_and_hint() {
        let inner = std::io::Error::other("No space left on device");
        let msg = disk_full_message("/jffs/mho", &inner);
        assert!(msg.contains("not enough disk space"), "{msg}");
        assert!(msg.contains("/jffs/mho"), "{msg}");
        assert!(msg.contains("--dir"), "{msg}");
    }
}
