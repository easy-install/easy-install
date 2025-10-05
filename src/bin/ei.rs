use anyhow::Result;
use clap::Parser;
use easy_install::{Args, run_main};
use std::str::FromStr;

#[tokio::main]
async fn main() -> Result<()> {
    if let Ok(lv) = std::env::var("LOG_LEVEL")
        && let Ok(lv) = tracing::Level::from_str(&lv)
    {
        tracing_subscriber::fmt().with_max_level(lv).init();
    }
    let args = Args::parse();
    if let Err(e) = run_main(args).await {
        eprintln!("Error: {e:?}");
        std::process::exit(1);
    }
    Ok(())
}
