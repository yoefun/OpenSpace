//! Core types for OpenSpace floorplan intermediate representation.

use chrono::{DateTime, Utc};
use glam::Vec2;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub type ProjectId = Uuid;
pub type EntityId = Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectStatus {
    Uploaded,
    Detecting,
    NeedsCorrection,
    Confirmed,
    Meshing,
    Texturing,
    Ready,
    Failed,
}

impl Default for ProjectStatus {
    fn default() -> Self {
        Self::Uploaded
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OpeningKind {
    Door,
    Window,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WallSegment {
    pub id: EntityId,
    pub a: Vec2,
    pub b: Vec2,
    pub thickness_m: f32,
    pub height_m: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Opening {
    pub id: EntityId,
    pub wall_id: EntityId,
    pub kind: OpeningKind,
    /// Parametric start along wall [0, 1]
    pub t0: f32,
    /// Parametric end along wall [0, 1]
    pub t1: f32,
    pub sill_m: Option<f32>,
    pub height_m: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Room {
    pub id: EntityId,
    pub name: String,
    pub polygon: Vec<Vec2>,
    pub wall_ids: Vec<EntityId>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PhotoRef {
    pub id: EntityId,
    pub path: String,
    pub room_id: Option<EntityId>,
    pub room_name: Option<String>,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FloorplanIR {
    pub walls: Vec<WallSegment>,
    pub openings: Vec<Opening>,
    pub rooms: Vec<Room>,
    pub scale_m_per_px: f32,
    pub default_wall_height_m: f32,
    pub default_wall_thickness_m: f32,
}

impl Default for FloorplanIR {
    fn default() -> Self {
        Self {
            walls: Vec::new(),
            openings: Vec::new(),
            rooms: Vec::new(),
            scale_m_per_px: 0.01,
            default_wall_height_m: 2.7,
            default_wall_thickness_m: 0.15,
        }
    }
}

impl FloorplanIR {
    pub fn is_empty(&self) -> bool {
        self.walls.is_empty() && self.rooms.is_empty()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    pub id: ProjectId,
    pub name: String,
    pub floorplan_image: String,
    pub photos: Vec<PhotoRef>,
    pub ir: FloorplanIR,
    pub status: ProjectStatus,
    pub error: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub model_path: Option<String>,
    pub progress: f32,
    pub progress_message: String,
}

impl Project {
    pub fn new(name: impl Into<String>, floorplan_image: impl Into<String>) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            floorplan_image: floorplan_image.into(),
            photos: Vec::new(),
            ir: FloorplanIR::default(),
            status: ProjectStatus::Uploaded,
            error: None,
            created_at: now,
            updated_at: now,
            model_path: None,
            progress: 0.0,
            progress_message: "uploaded".into(),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum OpenSpaceError {
    #[error("unsupported backend: {0}")]
    Unsupported(String),
    #[error("invalid floorplan: {0}")]
    InvalidFloorplan(String),
    #[error("io error: {0}")]
    Io(String),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("bad request: {0}")]
    BadRequest(String),
    #[error("internal: {0}")]
    Internal(String),
}

pub type Result<T> = std::result::Result<T, OpenSpaceError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ir_json_roundtrip() {
        let mut ir = FloorplanIR::default();
        let wall_id = Uuid::new_v4();
        ir.walls.push(WallSegment {
            id: wall_id,
            a: Vec2::new(0.0, 0.0),
            b: Vec2::new(5.0, 0.0),
            thickness_m: 0.15,
            height_m: 2.7,
        });
        ir.rooms.push(Room {
            id: Uuid::new_v4(),
            name: "Living".into(),
            polygon: vec![
                Vec2::ZERO,
                Vec2::new(5.0, 0.0),
                Vec2::new(5.0, 4.0),
                Vec2::new(0.0, 4.0),
            ],
            wall_ids: vec![wall_id],
        });
        let json = serde_json::to_string(&ir).unwrap();
        let back: FloorplanIR = serde_json::from_str(&json).unwrap();
        assert_eq!(back.walls.len(), 1);
        assert_eq!(back.rooms[0].name, "Living");
        assert!((back.walls[0].b.x - 5.0).abs() < 1e-5);
    }

    #[test]
    fn project_status_default() {
        assert_eq!(ProjectStatus::default(), ProjectStatus::Uploaded);
    }
}
