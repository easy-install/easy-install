use clap::Parser;
use easy_install::{run_main, Args};
use std::str::FromStr;

#[tokio::main]
async fn main() {
    if let Ok(lv) = std::env::var("LOG_LEVEL") {
        if let Ok(lv) = tracing::Level::from_str(&lv) {
            tracing_subscriber::fmt().with_max_level(lv).init();
        }
    }
    let args = Args::parse();
    run_main(args).await;
}
