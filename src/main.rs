mod caldav;
mod config;
mod tam;

use anyhow::{Context, Result};
use caldav::CalDav;
use clap::Parser;
use config::Config;
use tam::Tam;

#[derive(Parser, Debug)]
struct Args {
    /// Path to the TOML configuration file.
    #[arg(short, long)]
    config: std::path::PathBuf,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    let config_text = tokio::fs::read_to_string(&args.config)
        .await
        .with_context(|| format!("cannot read configuration {}", args.config.display()))?;
    let config: Config = serde_json::from_str(&config_text)
        .with_context(|| format!("cannot parse configuration {}", args.config.display()))?;
    config.validate()?;

    let tam = Tam::new(&config)?;
    let lessons = tam.fetch().await.context("fetching timetable")?;
    if lessons.is_empty() {
        anyhow::bail!("the timetable response contained no entries; refusing to modify calendar");
    }

    let calendar = CalDav::new(&config)?;
    calendar
        .sync(&lessons)
        .await
        .context("synchronizing calendar")
}
