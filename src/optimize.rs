use anyhow::Result;
use std::process::Command;
use tracing::trace;

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
                    eprintln!("Warning: Failed to strip {}: {}", file_path, e);
                    eprintln!("  Make sure 'strip' is installed and available in PATH");
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
                    eprintln!("Warning: Failed to compress {} with UPX: {}", file_path, e);
                    eprintln!("  Make sure 'upx' is installed and available in PATH");
                }
            }
        }
    }

    Ok(())
}

fn run_strip(file_path: &str) -> Result<()> {
    trace!("Running strip on: {}", file_path);

    let output = Command::new("strip")
        .arg(file_path)
        .output()
        .map_err(|e| anyhow::anyhow!("Failed to execute strip command: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow::anyhow!("strip command failed: {}", stderr));
    }

    Ok(())
}

fn run_upx(file_path: &str) -> Result<()> {
    trace!("Running upx on: {}", file_path);

    let output = Command::new("upx")
        .arg("--best")
        .arg("--lzma")
        .arg(file_path)
        .output()
        .map_err(|e| anyhow::anyhow!("Failed to execute upx command: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow::anyhow!("upx command failed: {}", stderr));
    }

    Ok(())
}
