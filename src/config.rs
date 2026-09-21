use anyhow::{bail, Context, Result};
use chrono::{Datelike, Duration, TimeZone};
use chrono_tz::Europe::Zurich;
use serde::Deserialize;
use std::{
    fs,
    path::{Path, PathBuf},
};

#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    pub tam: TamConfig,
    pub caldav: CalDavConfig,
    #[serde(default)]
    pub sync: SyncConfig,
}

#[derive(Debug, Deserialize, Clone)]
pub struct TamConfig {
    pub username: String,
    pub password_file: PathBuf,
    pub id: String,
    #[serde(default)]
    pub id_is_class: bool,
    #[serde(default = "default_school")]
    pub school: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct CalDavConfig {
    pub calendar_url: String,
    pub username: String,
    pub password_file: PathBuf,
}

#[derive(Debug, Deserialize, Clone)]
pub struct SyncConfig {
    #[serde(default = "default_past")]
    pub weeks_past: i64,
    #[serde(default = "default_future")]
    pub weeks_future: i64,
}

impl Default for SyncConfig {
    fn default() -> Self {
        Self {
            weeks_past: default_past(),
            weeks_future: default_future(),
        }
    }
}
fn default_school() -> String {
    "krm".into()
}
fn default_past() -> i64 {
    2
}
fn default_future() -> i64 {
    12
}

impl Config {
    pub fn validate(&self) -> Result<()> {
        if self.sync.weeks_past < 0 || self.sync.weeks_future < 1 {
            bail!("sync.weeks_past must be >= 0 and sync.weeks_future must be >= 1");
        }
        if self.caldav.calendar_url.is_empty() {
            bail!("caldav.calendar_url must not be empty");
        }
        Ok(())
    }
}

pub fn secret(path: &Path) -> Result<String> {
    let value = fs::read_to_string(path)
        .with_context(|| format!("cannot read secret file {}", path.display()))?;
    let value = value.trim().to_owned();
    if value.is_empty() {
        bail!("secret file {} is empty", path.display());
    }
    Ok(value)
}

pub fn range(config: &SyncConfig) -> Result<(i64, i64)> {
    let now = Zurich.from_utc_datetime(&chrono::Utc::now().naive_utc());
    let monday = now.date_naive() - Duration::days(now.weekday().num_days_from_monday() as i64);
    let start = monday
        .and_hms_opt(0, 0, 0)
        .context("invalid start of week")?
        .and_utc()
        - Duration::weeks(config.weeks_past);
    let end = start + Duration::weeks(config.weeks_past + config.weeks_future);
    Ok((start.timestamp_millis(), end.timestamp_millis()))
}
