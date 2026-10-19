use anyhow::{Context, Result, bail};
use std::{env, path::PathBuf};

#[derive(Clone, Debug)]
pub struct Config {
    pub bind: String,
    pub database_path: PathBuf,
    pub uploads_dir: PathBuf,
    pub web_dir: PathBuf,
    pub max_upload_bytes: usize,
    pub monitor_interval_secs: u64,
    pub allow_private_favicon: bool,
    pub worker_threads: usize,
    pub database_connections: u32,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        let data =
            PathBuf::from(env::var("SUN_PANEL_DATA_DIR").unwrap_or_else(|_| "./data".into()));
        Ok(Self {
            bind: env::var("SUN_PANEL_BIND").unwrap_or_else(|_| "127.0.0.1:3002".into()),
            database_path: env::var_os("SUN_PANEL_DATABASE")
                .map(PathBuf::from)
                .unwrap_or_else(|| data.join("database.db")),
            uploads_dir: env::var_os("SUN_PANEL_UPLOADS")
                .map(PathBuf::from)
                .unwrap_or_else(|| data.join("uploads")),
            web_dir: env::var_os("SUN_PANEL_WEB_DIR")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("./dist")),
            max_upload_bytes: number(
                "SUN_PANEL_MAX_UPLOAD_BYTES",
                8 * 1024 * 1024,
                1024,
                64 * 1024 * 1024,
            )? as usize,
            monitor_interval_secs: number("SUN_PANEL_MONITOR_INTERVAL", 5, 1, 300)? as u64,
            allow_private_favicon: env::var("SUN_PANEL_ALLOW_PRIVATE_FAVICON")
                .map(|v| v == "true" || v == "1")
                .unwrap_or(false),
            worker_threads: number("SUN_PANEL_WORKER_THREADS", 2, 1, 64)? as usize,
            database_connections: number("SUN_PANEL_DB_CONNECTIONS", 2, 1, 16)? as u32,
        })
    }
}

fn number(key: &str, default: i64, min: i64, max: i64) -> Result<i64> {
    let value = env::var(key)
        .ok()
        .map(|v| v.parse::<i64>().with_context(|| format!("Invalid {key}")))
        .transpose()?
        .unwrap_or(default);
    if !(min..=max).contains(&value) {
        bail!("{key} must be between {min} and {max}");
    }
    Ok(value)
}
