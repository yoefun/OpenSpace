use crate::db;
use crate::state::{AppState, ProjectEvent};
use axum::{
    extract::{Multipart, Path, State},
    http::StatusCode,
    response::{
        sse::{Event, KeepAlive, Sse},
        IntoResponse, Response,
    },
    routing::{get, post},
    Json, Router,
};
use chrono::Utc;
use futures::stream::StreamExt;
use openspace_core::{simplify_ir, FloorplanIR, PhotoRef, Project, ProjectStatus};
use serde::Serialize;
use std::convert::Infallible;
use tokio_stream::wrappers::BroadcastStream;
use uuid::Uuid;

pub fn api_router() -> Router<AppState> {
    Router::new()
        .route("/api/health", get(health))
        .route("/api/projects", post(create_project).get(list_projects))
        .route("/api/projects/{id}", get(get_project))
        .route("/api/projects/{id}/photos", post(upload_photos))
        .route("/api/projects/{id}/detect", post(detect))
        .route("/api/projects/{id}/ir", get(get_ir).put(put_ir))
        .route("/api/projects/{id}/ir/simplify", post(simplify_ir_route))
        .route("/api/projects/{id}/build", post(build))
        .route("/api/projects/{id}/events", get(events))
        .route("/api/projects/{id}/model.glb", get(model_glb))
        .route("/api/projects/{id}/floorplan", get(floorplan_image))
        .route("/api/projects/{id}/design", post(design_placeholder))
}

async fn health() -> Json<serde_json::Value> {
    Json(serde_json::json!({ "ok": true, "service": "openspace" }))
}

#[derive(Serialize)]
struct ErrorBody {
    error: String,
}

fn err(status: StatusCode, msg: impl Into<String>) -> Response {
    (status, Json(ErrorBody { error: msg.into() })).into_response()
}

async fn create_project(
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> Response {
    let mut name = "Untitled".to_string();
    let mut floorplan_bytes: Option<(String, Vec<u8>)> = None;

    while let Ok(Some(field)) = multipart.next_field().await {
        let field_name = field.name().unwrap_or("").to_string();
        if field_name == "name" {
            if let Ok(v) = field.text().await {
                if !v.trim().is_empty() {
                    name = v;
                }
            }
        } else if field_name == "floorplan" {
            let filename = field
                .file_name()
                .unwrap_or("floorplan.png")
                .to_string();
            match field.bytes().await {
                Ok(b) => floorplan_bytes = Some((filename, b.to_vec())),
                Err(e) => return err(StatusCode::BAD_REQUEST, e.to_string()),
            }
        }
    }

    let Some((filename, bytes)) = floorplan_bytes else {
        return err(StatusCode::BAD_REQUEST, "floorplan file required");
    };

    let ext = std::path::Path::new(&filename)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("png");
    let project = Project::new(name, format!("floorplan.{ext}"));
    let dir = state.project_dir(project.id);
    if let Err(e) = std::fs::create_dir_all(&dir) {
        return err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string());
    }
    let path = dir.join(&project.floorplan_image);
    if let Err(e) = std::fs::write(&path, &bytes) {
        return err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string());
    }

    if let Err(e) = db::insert_project(&state.pool, &project).await {
        return err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string());
    }

    Json(project).into_response()
}

async fn list_projects(State(state): State<AppState>) -> Response {
    match db::list_projects(&state.pool).await {
        Ok(list) => Json(list).into_response(),
        Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

async fn get_project(State(state): State<AppState>, Path(id): Path<Uuid>) -> Response {
    match db::get_project(&state.pool, id).await {
        Ok(Some(p)) => Json(p).into_response(),
        Ok(None) => err(StatusCode::NOT_FOUND, "project not found"),
        Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

async fn upload_photos(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    mut multipart: Multipart,
) -> Response {
    let mut project = match db::get_project(&state.pool, id).await {
        Ok(Some(p)) => p,
        Ok(None) => return err(StatusCode::NOT_FOUND, "project not found"),
        Err(e) => return err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    };

    let dir = state.project_dir(id).join("photos");
    if let Err(e) = std::fs::create_dir_all(&dir) {
        return err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string());
    }

    let mut room_name: Option<String> = None;
    let mut files: Vec<(String, Vec<u8>)> = Vec::new();

    while let Ok(Some(field)) = multipart.next_field().await {
        let fname = field.name().unwrap_or("").to_string();
        if fname == "room_name" {
            room_name = field.text().await.ok().map(|s| s.trim().to_string());
        } else if fname == "photo" || fname == "photos" {
            let filename = field
                .file_name()
                .unwrap_or("photo.jpg")
                .to_string();
            if let Ok(b) = field.bytes().await {
                files.push((filename, b.to_vec()));
            }
        }
    }

    for (filename, bytes) in files {
        let photo_id = Uuid::new_v4();
        let ext = std::path::Path::new(&filename)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("jpg");
        let rel = format!("photos/{photo_id}.{ext}");
        let path = state.project_dir(id).join(&rel);
        if let Err(e) = std::fs::write(&path, &bytes) {
            return err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string());
        }

        let mut room_id = None;
        if let Some(ref rn) = room_name {
            if let Some(room) = project.ir.rooms.iter().find(|r| r.name == *rn) {
                room_id = Some(room.id);
            }
        }

        project.photos.push(PhotoRef {
            id: photo_id,
            path: rel,
            room_id,
            room_name: room_name.clone(),
            notes: None,
        });
    }

    project.updated_at = Utc::now();
    if let Err(e) = db::update_project(&state.pool, &project).await {
        return err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string());
    }
    Json(project).into_response()
}

async fn detect(State(state): State<AppState>, Path(id): Path<Uuid>) -> Response {
    let mut project = match db::get_project(&state.pool, id).await {
        Ok(Some(p)) => p,
        Ok(None) => return err(StatusCode::NOT_FOUND, "project not found"),
        Err(e) => return err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    };

    project.status = ProjectStatus::Detecting;
    project.progress = 0.1;
    project.progress_message = "detecting walls".into();
    project.updated_at = Utc::now();
    let _ = db::update_project(&state.pool, &project).await;
    let _ = state.events.send(ProjectEvent {
        project_id: id,
        status: "detecting".into(),
        progress: 0.1,
        message: "detecting walls".into(),
    });

    let img_path = state.project_dir(id).join(&project.floorplan_image);
    let img = match image::open(&img_path) {
        Ok(i) => i,
        Err(e) => {
            project.status = ProjectStatus::Failed;
            project.error = Some(e.to_string());
            let _ = db::update_project(&state.pool, &project).await;
            return err(StatusCode::BAD_REQUEST, format!("cannot open floorplan: {e}"));
        }
    };

    match state.backend.detect(&img).await {
        Ok(ir) => {
            // Preserve scale/height if user already set via empty defaults
            project.ir = ir;
            project.status = ProjectStatus::NeedsCorrection;
            project.progress = 0.3;
            project.progress_message = "needs correction".into();
            project.error = None;
            project.updated_at = Utc::now();
            if let Err(e) = db::update_project(&state.pool, &project).await {
                return err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string());
            }
            let _ = state.events.send(ProjectEvent {
                project_id: id,
                status: "needs_correction".into(),
                progress: 0.3,
                message: "needs correction".into(),
            });
            Json(project).into_response()
        }
        Err(e) => {
            // On failure, still allow manual editing with empty IR
            project.ir = FloorplanIR::default();
            project.status = ProjectStatus::NeedsCorrection;
            project.progress = 0.3;
            project.progress_message = format!("detect failed, edit manually: {e}");
            project.error = Some(e.to_string());
            project.updated_at = Utc::now();
            let _ = db::update_project(&state.pool, &project).await;
            Json(project).into_response()
        }
    }
}

async fn get_ir(State(state): State<AppState>, Path(id): Path<Uuid>) -> Response {
    match db::get_project(&state.pool, id).await {
        Ok(Some(p)) => Json(p.ir).into_response(),
        Ok(None) => err(StatusCode::NOT_FOUND, "project not found"),
        Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

async fn put_ir(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(ir): Json<FloorplanIR>,
) -> Response {
    let mut project = match db::get_project(&state.pool, id).await {
        Ok(Some(p)) => p,
        Ok(None) => return err(StatusCode::NOT_FOUND, "project not found"),
        Err(e) => return err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    };

    // Re-bind photo room_ids by room_name
    for photo in &mut project.photos {
        if let Some(ref rn) = photo.room_name {
            photo.room_id = ir.rooms.iter().find(|r| &r.name == rn).map(|r| r.id);
        }
    }

    project.ir = ir;
    project.status = ProjectStatus::NeedsCorrection;
    project.updated_at = Utc::now();
    if let Err(e) = db::update_project(&state.pool, &project).await {
        return err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string());
    }
    Json(project).into_response()
}

async fn simplify_ir_route(State(state): State<AppState>, Path(id): Path<Uuid>) -> Response {
    let mut project = match db::get_project(&state.pool, id).await {
        Ok(Some(p)) => p,
        Ok(None) => return err(StatusCode::NOT_FOUND, "project not found"),
        Err(e) => return err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    };

    let min_len = project.ir.scale_m_per_px * 12.0;
    let merge_tol = project.ir.scale_m_per_px * 8.0;
    simplify_ir(&mut project.ir, min_len.max(0.15), merge_tol.max(0.1));
    project.status = ProjectStatus::NeedsCorrection;
    project.updated_at = Utc::now();
    if let Err(e) = db::update_project(&state.pool, &project).await {
        return err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string());
    }
    Json(project).into_response()
}

async fn build(State(state): State<AppState>, Path(id): Path<Uuid>) -> Response {
    let mut project = match db::get_project(&state.pool, id).await {
        Ok(Some(p)) => p,
        Ok(None) => return err(StatusCode::NOT_FOUND, "project not found"),
        Err(e) => return err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    };

    if project.ir.is_empty() {
        return err(
            StatusCode::BAD_REQUEST,
            "IR is empty; correct walls/rooms before build",
        );
    }

    if matches!(
        project.status,
        ProjectStatus::Meshing | ProjectStatus::Texturing | ProjectStatus::Confirmed
    ) {
        return err(
            StatusCode::CONFLICT,
            "build already in progress; wait or refresh status",
        );
    }

    // Clean noisy detections before meshing (common with raster floorplans).
    let min_len = project.ir.scale_m_per_px * 10.0;
    let merge_tol = project.ir.scale_m_per_px * 6.0;
    simplify_ir(&mut project.ir, min_len.max(0.12), merge_tol.max(0.08));
    project.status = ProjectStatus::Confirmed;
    project.progress = 0.4;
    project.progress_message = "queued for build".into();
    project.error = None;
    project.updated_at = Utc::now();
    if let Err(e) = db::update_project(&state.pool, &project).await {
        return err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string());
    }

    if let Err(e) = state.build_tx.send(id).await {
        return err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string());
    }

    let _ = state.events.send(ProjectEvent {
        project_id: id,
        status: "confirmed".into(),
        progress: 0.4,
        message: "queued for build".into(),
    });

    Json(project).into_response()
}

async fn events(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Sse<impl futures::Stream<Item = Result<Event, Infallible>>> {
    let rx = state.events.subscribe();
    let stream = BroadcastStream::new(rx).filter_map(move |msg| {
        let id = id;
        async move {
            match msg {
                Ok(ev) if ev.project_id == id => {
                    let data = serde_json::to_string(&ev).unwrap_or_default();
                    Some(Ok(Event::default().event("progress").data(data)))
                }
                _ => None,
            }
        }
    });
    Sse::new(stream).keep_alive(KeepAlive::default())
}

async fn model_glb(State(state): State<AppState>, Path(id): Path<Uuid>) -> Response {
    let project = match db::get_project(&state.pool, id).await {
        Ok(Some(p)) => p,
        Ok(None) => return err(StatusCode::NOT_FOUND, "project not found"),
        Err(e) => return err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    };
    let Some(rel) = project.model_path else {
        return err(StatusCode::NOT_FOUND, "model not ready");
    };
    let path = state.project_dir(id).join(rel);
    match std::fs::read(&path) {
        Ok(bytes) => (
            StatusCode::OK,
            [(axum::http::header::CONTENT_TYPE, "model/gltf-binary")],
            bytes,
        )
            .into_response(),
        Err(e) => err(StatusCode::NOT_FOUND, e.to_string()),
    }
}

async fn floorplan_image(State(state): State<AppState>, Path(id): Path<Uuid>) -> Response {
    let project = match db::get_project(&state.pool, id).await {
        Ok(Some(p)) => p,
        Ok(None) => return err(StatusCode::NOT_FOUND, "project not found"),
        Err(e) => return err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    };
    let path = state.project_dir(id).join(&project.floorplan_image);
    match std::fs::read(&path) {
        Ok(bytes) => {
            let mime = mime_guess::from_path(&path)
                .first_or_octet_stream()
                .essence_str()
                .to_string();
            (StatusCode::OK, [(axum::http::header::CONTENT_TYPE, mime)], bytes).into_response()
        }
        Err(e) => err(StatusCode::NOT_FOUND, e.to_string()),
    }
}

async fn design_placeholder() -> Response {
    (
        StatusCode::NOT_IMPLEMENTED,
        Json(ErrorBody {
            error: "AI design output is not implemented in MVP; reserved for future release"
                .into(),
        }),
    )
        .into_response()
}
