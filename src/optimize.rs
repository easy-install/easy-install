use anyhow::Result;
use std::process::Command;
use tracing::trace;

#[derive(Debug)]
enum OptimizeError {
    CommandNotFound(String),
    AlreadyProcessed(String),
    ProcessingFailed(String),
}

impl std::fmt::Display for OptimizeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OptimizeError::CommandNotFound(cmd) => write!(f, "{} command not found", cmd),
            OptimizeError::AlreadyProcessed(msg) => write!(f, "{}", msg),
            OptimizeError::ProcessingFailed(msg) => write!(f, "{}", msg),
        }
    }
}

impl std::error::Error for OptimizeError {}

/// Optimize a single executable file by running strip and/or upx
pub fn optimize_executable(file_path: &str, strip: bool, upx: bool, quiet: bool) -> Result<()> {
    if !strip && !upx {
        return Ok(());
    }

    trace!("Optimizing executable: {}", file_path);

    // Run strip first if enabled
    if strip {
        match run_strip(file_path) {
            Ok(_) => {
                if !quiet {
                    println!("✓ Stripped debug symbols from: {}", file_path);
                }
            }
            Err(e) => {
                if !quiet {
                    match e {
                        OptimizeError::CommandNotFound(_) => {
                            eprintln!("Warning: Failed to strip {}: {}", file_path, e);
                            eprintln!("  Make sure 'strip' is installed and available in PATH");
                        }
                        OptimizeError::AlreadyProcessed(_) => {
                            eprintln!("Warning: {}", e);
                        }
                        OptimizeError::ProcessingFailed(_) => {
                            eprintln!("Warning: Failed to strip {}: {}", file_path, e);
                        }
                    }
                }
            }
        }
    }

    // Run upx after strip if enabled
    if upx {
        match run_upx(file_path) {
            Ok(_) => {
                if !quiet {
                    println!("✓ Compressed with UPX: {}", file_path);
                }
            }
            Err(e) => {
                if !quiet {
                    match e {
                        OptimizeError::CommandNotFound(_) => {
                            eprintln!("Warning: Failed to compress {} with UPX: {}", file_path, e);
                            eprintln!("  Make sure 'upx' is installed and available in PATH");
                        }
                        OptimizeError::AlreadyProcessed(_) => {
                            eprintln!("Warning: {}", e);
                        }
                        OptimizeError::ProcessingFailed(_) => {
                            eprintln!("Warning: Failed to compress {} with UPX: {}", file_path, e);
                        }
                    }
                }
            }
        }
    }

    Ok(())
}

fn run_strip(file_path: &str) -> Result<(), OptimizeError> {
    trace!("Running strip on: {}", file_path);

    let output = Command::new("strip").arg(file_path).output().map_err(|e| {
        // Check if the error is because the command was not found
        if e.kind() == std::io::ErrorKind::NotFound {
            OptimizeError::CommandNotFound("strip".to_string())
        } else {
            OptimizeError::ProcessingFailed(format!("Failed to execute strip command: {}", e))
        }
    })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);

        // Check for specific error patterns
        let stderr_lower = stderr.to_lowercase();
        if stderr_lower.contains("no symbols") || stderr_lower.contains("already stripped") {
            return Err(OptimizeError::AlreadyProcessed(
                "File has no debug symbols or is already stripped".to_string(),
            ));
        }

        return Err(OptimizeError::ProcessingFailed(stderr.to_string()));
    }

    Ok(())
}

fn run_upx(file_path: &str) -> Result<(), OptimizeError> {
    trace!("Running upx on: {}", file_path);

    // First check if the file is already compressed with UPX
    let test_output = Command::new("upx")
        .arg("-t")
        .arg(file_path)
        .output()
        .map_err(|e| {
            // Check if the error is because the command was not found
            if e.kind() == std::io::ErrorKind::NotFound {
                OptimizeError::CommandNotFound("upx".to_string())
            } else {
                OptimizeError::ProcessingFailed(format!(
                    "Failed to execute upx test command: {}",
                    e
                ))
            }
        })?;

    // If test succeeds, the file is already compressed
    if test_output.status.success() {
        return Err(OptimizeError::AlreadyProcessed(
            "File is already compressed with UPX".to_string(),
        ));
    }

    // Check if the error indicates the file is already packed
    let test_stderr = String::from_utf8_lossy(&test_output.stderr);
    if test_stderr.contains("already packed")
        || test_stderr.contains("already compressed")
        || test_stderr.contains("NotPackable")
    {
        return Err(OptimizeError::AlreadyProcessed(
            "File is already compressed with UPX".to_string(),
        ));
    }

    // Proceed with compression
    let output = Command::new("upx")
        .arg("--best")
        .arg("--lzma")
        .arg(file_path)
        .output()
        .map_err(|e| {
            // Check if the error is because the command was not found
            if e.kind() == std::io::ErrorKind::NotFound {
                OptimizeError::CommandNotFound("upx".to_string())
            } else {
                OptimizeError::ProcessingFailed(format!("Failed to execute upx command: {}", e))
            }
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);

        // Check for specific error messages
        if stderr.contains("already packed")
            || stderr.contains("already compressed")
            || stderr.contains("NotPackable")
        {
            return Err(OptimizeError::AlreadyProcessed(
                "File is already compressed with UPX".to_string(),
            ));
        }

        return Err(OptimizeError::ProcessingFailed(stderr.to_string()));
    }

    Ok(())
}
