use crate::{config::Config, system::MonitorState};
use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    pub db: sqlx::SqlitePool,
    pub config: Arc<Config>,
    pub http: reqwest::Client,
    pub monitor: Arc<MonitorState>,
    pub owner_id: i64,
}
