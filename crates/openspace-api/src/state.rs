use openspace_plugin::{default_backend, ReconstructionBackend};
use sqlx::sqlite::SqlitePoolOptions;
use sqlx::SqlitePool;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{broadcast, Mutex};

use crate::db;

#[derive(Clone)]
pub struct AppState {
    pub pool: SqlitePool,
    pub data_dir: PathBuf,
    pub backend: Arc<dyn ReconstructionBackend>,
    pub build_tx: tokio::sync::mpsc::Sender<uuid::Uuid>,
    pub build_rx: Arc<Mutex<Option<tokio::sync::mpsc::Receiver<uuid::Uuid>>>>,
    pub events: broadcast::Sender<ProjectEvent>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ProjectEvent {
    pub project_id: uuid::Uuid,
    pub status: String,
    pub progress: f32,
    pub message: String,
}

impl AppState {
    pub async fn new(data_dir: PathBuf) -> anyhow::Result<Self> {
        std::fs::create_dir_all(&data_dir)?;
        let db_path = data_dir.join("openspace.db");
        let url = format!("sqlite://{}?mode=rwc", db_path.display());
        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect(&url)
            .await?;
        db::migrate(&pool).await?;

        let (build_tx, build_rx) = tokio::sync::mpsc::channel(32);
        let (events, _) = broadcast::channel(64);

        Ok(Self {
            pool,
            data_dir,
            backend: default_backend(),
            build_tx,
            build_rx: Arc::new(Mutex::new(Some(build_rx))),
            events,
        })
    }

    pub fn project_dir(&self, id: uuid::Uuid) -> PathBuf {
        self.data_dir.join("projects").join(id.to_string())
    }
}
