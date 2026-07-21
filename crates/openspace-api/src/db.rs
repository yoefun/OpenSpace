use openspace_core::{FloorplanIR, PhotoRef, Project, ProjectStatus};
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

pub async fn migrate(pool: &SqlitePool) -> anyhow::Result<()> {
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS projects (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            floorplan_image TEXT NOT NULL,
            photos_json TEXT NOT NULL DEFAULT '[]',
            ir_json TEXT NOT NULL,
            status TEXT NOT NULL,
            error TEXT,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            model_path TEXT,
            progress REAL NOT NULL DEFAULT 0,
            progress_message TEXT NOT NULL DEFAULT ''
        )
        "#,
    )
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn insert_project(pool: &SqlitePool, p: &Project) -> anyhow::Result<()> {
    sqlx::query(
        r#"
        INSERT INTO projects
        (id, name, floorplan_image, photos_json, ir_json, status, error, created_at, updated_at, model_path, progress, progress_message)
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind(p.id.to_string())
    .bind(&p.name)
    .bind(&p.floorplan_image)
    .bind(serde_json::to_string(&p.photos)?)
    .bind(serde_json::to_string(&p.ir)?)
    .bind(status_str(p.status))
    .bind(&p.error)
    .bind(p.created_at.to_rfc3339())
    .bind(p.updated_at.to_rfc3339())
    .bind(&p.model_path)
    .bind(p.progress)
    .bind(&p.progress_message)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn get_project(pool: &SqlitePool, id: Uuid) -> anyhow::Result<Option<Project>> {
    let row = sqlx::query("SELECT * FROM projects WHERE id = ?")
        .bind(id.to_string())
        .fetch_optional(pool)
        .await?;
    Ok(row.map(|r| row_to_project(&r)).transpose()?)
}

pub async fn update_project(pool: &SqlitePool, p: &Project) -> anyhow::Result<()> {
    sqlx::query(
        r#"
        UPDATE projects SET
            name = ?, floorplan_image = ?, photos_json = ?, ir_json = ?, status = ?,
            error = ?, updated_at = ?, model_path = ?, progress = ?, progress_message = ?
        WHERE id = ?
        "#,
    )
    .bind(&p.name)
    .bind(&p.floorplan_image)
    .bind(serde_json::to_string(&p.photos)?)
    .bind(serde_json::to_string(&p.ir)?)
    .bind(status_str(p.status))
    .bind(&p.error)
    .bind(p.updated_at.to_rfc3339())
    .bind(&p.model_path)
    .bind(p.progress)
    .bind(&p.progress_message)
    .bind(p.id.to_string())
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn list_projects(pool: &SqlitePool) -> anyhow::Result<Vec<Project>> {
    let rows = sqlx::query("SELECT * FROM projects ORDER BY created_at DESC")
        .fetch_all(pool)
        .await?;
    rows.iter().map(row_to_project).collect()
}

fn row_to_project(r: &sqlx::sqlite::SqliteRow) -> anyhow::Result<Project> {
    let id: String = r.get("id");
    let status: String = r.get("status");
    let ir_json: String = r.get("ir_json");
    let photos_json: String = r.get("photos_json");
    let created_at: String = r.get("created_at");
    let updated_at: String = r.get("updated_at");
    Ok(Project {
        id: Uuid::parse_str(&id)?,
        name: r.get("name"),
        floorplan_image: r.get("floorplan_image"),
        photos: serde_json::from_str::<Vec<PhotoRef>>(&photos_json).unwrap_or_default(),
        ir: serde_json::from_str::<FloorplanIR>(&ir_json).unwrap_or_default(),
        status: parse_status(&status),
        error: r.get("error"),
        created_at: chrono::DateTime::parse_from_rfc3339(&created_at)?
            .with_timezone(&chrono::Utc),
        updated_at: chrono::DateTime::parse_from_rfc3339(&updated_at)?
            .with_timezone(&chrono::Utc),
        model_path: r.get("model_path"),
        progress: r.get("progress"),
        progress_message: r.get("progress_message"),
    })
}

fn status_str(s: ProjectStatus) -> &'static str {
    match s {
        ProjectStatus::Uploaded => "uploaded",
        ProjectStatus::Detecting => "detecting",
        ProjectStatus::NeedsCorrection => "needs_correction",
        ProjectStatus::Confirmed => "confirmed",
        ProjectStatus::Meshing => "meshing",
        ProjectStatus::Texturing => "texturing",
        ProjectStatus::Ready => "ready",
        ProjectStatus::Failed => "failed",
    }
}

fn parse_status(s: &str) -> ProjectStatus {
    match s {
        "detecting" => ProjectStatus::Detecting,
        "needs_correction" => ProjectStatus::NeedsCorrection,
        "confirmed" => ProjectStatus::Confirmed,
        "meshing" => ProjectStatus::Meshing,
        "texturing" => ProjectStatus::Texturing,
        "ready" => ProjectStatus::Ready,
        "failed" => ProjectStatus::Failed,
        _ => ProjectStatus::Uploaded,
    }
}
