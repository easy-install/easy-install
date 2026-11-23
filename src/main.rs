use clap::Parser;
use easy_install::error::{Result, format_chain};
use easy_install::{Args, run_main};

#[cfg(feature = "logging")]
use std::str::FromStr;

/// Initialise tracing when `LOG_LEVEL` is set (`logging` feature only).
#[cfg(feature = "logging")]
fn init_logging() {
    if let Ok(level) = std::env::var("LOG_LEVEL")
        && let Ok(level) = tracing::Level::from_str(&level)
    {
        tracing_subscriber::fmt().with_max_level(level).init();
    }
}

#[cfg(not(feature = "logging"))]
fn init_logging() {}

#[tokio::main]
async fn main() -> Result<()> {
    init_logging();

    let args = Args::parse();
    if let Err(e) = run_main(args).await {
        eprintln!("Error: {}", format_chain(&e));
        std::process::exit(1);
    }
    Ok(())
}
