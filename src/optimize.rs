use crate::backend::{Backend, CommandResult, Host};
use crate::error::{Result, err};
use tracing::trace;

#[derive(Debug, thiserror::Error)]
enum OptimizeError {
    #[error("{0} command not found")]
    CommandNotFound(String),
    #[error("{0}")]
    AlreadyProcessed(String),
    #[error("{0}")]
    ProcessingFailed(String),
}

impl OptimizeError {
    /// Message shown to the user when the optimization of `file` fails.
    ///
    /// The install itself already succeeded, so the caller only warns.
    fn message(&self, tool: &str, file: &str) -> String {
        let action = if tool == "upx" { "compress" } else { "strip" };
        match self {
            Self::CommandNotFound(_) => format!(
                "failed to {action} '{file}': '{tool}' was not found\n  Make sure '{tool}' is installed and available in PATH, or drop --{tool}"
            ),
            Self::AlreadyProcessed(_) | Self::ProcessingFailed(_) => {
                format!("failed to {action} '{file}': {self}")
            }
        }
    }
}

/// Optimize a single executable file by running strip and/or upx.
///
/// The commands are executed through `backend`, so a remote install runs
/// them on the remote host. A failure is returned to the caller rather than
/// swallowed: the install itself already succeeded.
pub(crate) async fn optimize_executable(
    backend: &Backend,
    file_path: &str,
    strip: bool,
    upx: bool,
    quiet: bool,
) -> Result<()> {
    if !strip && !upx {
        return Ok(());
    }

    trace!("Optimizing executable: {}", file_path);

    // Strip first: UPX compresses a stripped binary better.
    if strip {
        run_strip(backend, file_path)
            .await
            .map_err(|e| err!("{}", e.message("strip", file_path)))?;
        if !quiet {
            println!("✓ Stripped debug symbols from: {}", file_path);
        }
    }

    if upx {
        run_upx(backend, file_path)
            .await
            .map_err(|e| err!("{}", e.message("upx", file_path)))?;
        if !quiet {
            println!("✓ Compressed with UPX: {}", file_path);
        }
    }

    Ok(())
}

async fn run_strip(backend: &Backend, file_path: &str) -> Result<(), OptimizeError> {
    trace!("Running strip on: {}", file_path);

    let output = backend
        .run("strip", &[file_path])
        .await
        .map_err(|e| OptimizeError::ProcessingFailed(e.to_string()))?;

    check_status("strip", output)
}

async fn run_upx(backend: &Backend, file_path: &str) -> Result<(), OptimizeError> {
    trace!("Running upx on: {}", file_path);

    // First check if the file is already compressed with UPX
    let test = backend
        .run("upx", &["-t", file_path])
        .await
        .map_err(|e| OptimizeError::ProcessingFailed(e.to_string()))?;

    // If the test succeeds, the file is already compressed
    if test.success() {
        return Err(OptimizeError::AlreadyProcessed(
            "File is already compressed with UPX".to_string(),
        ));
    }

    // Check if the error indicates the file is already packed
    if is_already_packed(&test.stderr) {
        return Err(OptimizeError::AlreadyProcessed(
            "File is already compressed with UPX".to_string(),
        ));
    }

    // Proceed with compression
    let output = backend
        .run("upx", &["--best", "--lzma", file_path])
        .await
        .map_err(|e| OptimizeError::ProcessingFailed(e.to_string()))?;

    if !output.success() && is_already_packed(&output.stderr) {
        return Err(OptimizeError::AlreadyProcessed(
            "File is already compressed with UPX".to_string(),
        ));
    }

    check_status("upx", output)
}

fn is_already_packed(stderr: &str) -> bool {
    stderr.contains("already packed")
        || stderr.contains("already compressed")
        || stderr.contains("NotPackable")
}

fn check_status(tool: &str, output: CommandResult) -> Result<(), OptimizeError> {
    if output.success() {
        return Ok(());
    }

    let stderr = output.stderr.trim();

    // Local runs surface a missing binary as an I/O error (mapped to this
    // message by the backend); remote runs produce "command not found".
    if stderr.contains("command not found") {
        return Err(OptimizeError::CommandNotFound(tool.to_string()));
    }

    let lower = stderr.to_lowercase();
    if lower.contains("no symbols") || lower.contains("already stripped") {
        return Err(OptimizeError::AlreadyProcessed(
            "File has no debug symbols or is already stripped".to_string(),
        ));
    }

    Err(OptimizeError::ProcessingFailed(format!(
        "{tool} exited with code {}: {stderr}",
        output.exit_code
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Failures reach the caller, which turns them into a warning instead of
    /// reporting a successful install.
    #[tokio::test]
    async fn optimization_failures_are_returned() {
        let backend = Backend::local();
        let missing = std::env::temp_dir()
            .join("ei-optimize-missing-binary")
            .to_string_lossy()
            .into_owned();

        let error = optimize_executable(&backend, &missing, true, false, true)
            .await
            .expect_err("optimizing a missing file must fail");
        let message = error.to_string();
        assert!(message.contains(&missing), "{message}");
        assert!(message.contains("strip"), "{message}");
    }
}
