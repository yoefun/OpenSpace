use crate::db;
use crate::state::{AppState, ProjectEvent};
use chrono::Utc;
use openspace_core::ProjectStatus;

pub fn spawn_worker(state: AppState) {
    tokio::spawn(async move {
        let mut rx = {
            let mut guard = state.build_rx.lock().await;
            guard.take().expect("build receiver already taken")
        };

        while let Some(project_id) = rx.recv().await {
            if let Err(e) = process_build(&state, project_id).await {
                tracing::error!("build failed for {project_id}: {e}");
                if let Ok(Some(mut p)) = db::get_project(&state.pool, project_id).await {
                    p.status = ProjectStatus::Failed;
                    p.error = Some(e.to_string());
                    p.progress_message = format!("failed: {e}");
                    p.updated_at = Utc::now();
                    let _ = db::update_project(&state.pool, &p).await;
                    let _ = state.events.send(ProjectEvent {
                        project_id,
                        status: "failed".into(),
                        progress: p.progress,
                        message: p.progress_message.clone(),
                    });
                }
            }
        }
    });
}

async fn process_build(state: &AppState, project_id: uuid::Uuid) -> anyhow::Result<()> {
    let mut project = db::get_project(&state.pool, project_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("project not found"))?;

    let emit = |state: &AppState, project_id: uuid::Uuid, status: &str, progress: f32, message: &str| {
        let _ = state.events.send(ProjectEvent {
            project_id,
            status: status.into(),
            progress,
            message: message.into(),
        });
    };

    project.status = ProjectStatus::Meshing;
    project.progress = 0.55;
    project.progress_message = "meshing".into();
    project.updated_at = Utc::now();
    db::update_project(&state.pool, &project).await?;
    emit(state, project_id, "meshing", 0.55, "meshing");

    let data_root = state.project_dir(project_id);

    // Resolve photo room_ids by name again
    for photo in &mut project.photos {
        if photo.room_id.is_none() {
            if let Some(ref rn) = photo.room_name {
                photo.room_id = project
                    .ir
                    .rooms
                    .iter()
                    .find(|r| &r.name == rn)
                    .map(|r| r.id);
            }
        }
    }

    project.status = ProjectStatus::Texturing;
    project.progress = 0.75;
    project.progress_message = "texturing".into();
    project.updated_at = Utc::now();
    db::update_project(&state.pool, &project).await?;
    emit(state, project_id, "texturing", 0.75, "texturing");

    let glb = state
        .backend
        .build_glb(&project.ir, &project.photos, &data_root)
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;

    let model_rel = "model.glb";
    std::fs::write(data_root.join(model_rel), &glb)?;

    project.status = ProjectStatus::Ready;
    project.progress = 1.0;
    project.progress_message = "ready".into();
    project.model_path = Some(model_rel.into());
    project.error = None;
    project.updated_at = Utc::now();
    db::update_project(&state.pool, &project).await?;
    emit(state, project_id, "ready", 1.0, "ready");

    tracing::info!(
        "build complete for {project_id}: {} bytes",
        glb.len()
    );
    Ok(())
}
