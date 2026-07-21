//! Simplify noisy wall graphs (post-detection / pre-build).

use crate::{FloorplanIR, Room, WallSegment};
use glam::Vec2;
use uuid::Uuid;

/// Merge collinear walls, drop short segments, rebuild a single room bbox.
/// Never removes all walls — keeps at least the longest segments.
pub fn simplify_ir(ir: &mut FloorplanIR, min_length_m: f32, merge_tol_m: f32) {
    if ir.walls.is_empty() {
        return;
    }
    let backup = ir.walls.clone();
    ir.walls.retain(|w| wall_length(w) >= min_length_m);
    merge_collinear_walls(&mut ir.walls, merge_tol_m);
    if ir.walls.len() < 3 {
        ir.walls = backup;
        let mut sorted = ir.walls.clone();
        sorted.sort_by(|a, b| {
            wall_length(b)
                .partial_cmp(&wall_length(a))
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        sorted.truncate(24);
        ir.walls = sorted;
        merge_collinear_walls(&mut ir.walls, merge_tol_m);
    }
    ir.openings
        .retain(|o| ir.walls.iter().any(|w| w.id == o.wall_id));
    rebuild_room_bbox(ir);
}

fn wall_length(w: &WallSegment) -> f32 {
    (w.b - w.a).length()
}

fn merge_collinear_walls(walls: &mut Vec<WallSegment>, tol: f32) {
    let mut changed = true;
    while changed {
        changed = false;
        'outer: for i in 0..walls.len() {
            for j in (i + 1)..walls.len() {
                if let Some(merged) = try_merge_walls(&walls[i], &walls[j], tol) {
                    walls[i] = merged;
                    walls.remove(j);
                    changed = true;
                    break 'outer;
                }
            }
        }
    }
}

fn try_merge_walls(a: &WallSegment, b: &WallSegment, tol: f32) -> Option<WallSegment> {
    let ha = is_horiz_wall(a);
    let hb = is_horiz_wall(b);
    if ha != hb {
        return None;
    }
    if ha {
        if (a.a.y - b.a.y).abs() > tol && (a.b.y - b.b.y).abs() > tol {
            return None;
        }
        let y = (a.a.y + a.b.y + b.a.y + b.b.y) * 0.25;
        let a_min = a.a.x.min(a.b.x);
        let a_max = a.a.x.max(a.b.x);
        let b_min = b.a.x.min(b.b.x);
        let b_max = b.a.x.max(b.b.x);
        if a_max < b_min - tol || b_max < a_min - tol {
            return None;
        }
        Some(WallSegment {
            id: a.id,
            a: Vec2::new(a_min.min(b_min), y),
            b: Vec2::new(a_max.max(b_max), y),
            thickness_m: a.thickness_m,
            height_m: a.height_m,
        })
    } else {
        if (a.a.x - b.a.x).abs() > tol && (a.b.x - b.b.x).abs() > tol {
            return None;
        }
        let x = (a.a.x + a.b.x + b.a.x + b.b.x) * 0.25;
        let a_min = a.a.y.min(a.b.y);
        let a_max = a.a.y.max(a.b.y);
        let b_min = b.a.y.min(b.b.y);
        let b_max = b.a.y.max(b.b.y);
        if a_max < b_min - tol || b_max < a_min - tol {
            return None;
        }
        Some(WallSegment {
            id: a.id,
            a: Vec2::new(x, a_min.min(b_min)),
            b: Vec2::new(x, a_max.max(b_max)),
            thickness_m: a.thickness_m,
            height_m: a.height_m,
        })
    }
}

fn is_horiz_wall(w: &WallSegment) -> bool {
    (w.a.y - w.b.y).abs() < (w.a.x - w.b.x).abs()
}

fn rebuild_room_bbox(ir: &mut FloorplanIR) {
    if ir.walls.is_empty() {
        ir.rooms.clear();
        return;
    }
    let mut min = Vec2::splat(f32::INFINITY);
    let mut max = Vec2::splat(f32::NEG_INFINITY);
    for w in &ir.walls {
        min = min.min(w.a).min(w.b);
        max = max.max(w.a).max(w.b);
    }
    let existing = ir.rooms.first();
    ir.rooms = vec![Room {
        id: existing.map(|r| r.id).unwrap_or_else(Uuid::new_v4),
        name: existing
            .map(|r| r.name.clone())
            .unwrap_or_else(|| "Room 1".into()),
        polygon: vec![
            Vec2::new(min.x, min.y),
            Vec2::new(max.x, min.y),
            Vec2::new(max.x, max.y),
            Vec2::new(min.x, max.y),
        ],
        wall_ids: ir.walls.iter().map(|w| w.id).collect(),
    }];
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merges_collinear_and_drops_short() {
        let mut ir = FloorplanIR::default();
        let id = Uuid::new_v4();
        ir.walls = vec![
            WallSegment {
                id,
                a: Vec2::new(0.0, 0.0),
                b: Vec2::new(2.0, 0.0),
                thickness_m: 0.15,
                height_m: 2.7,
            },
            WallSegment {
                id: Uuid::new_v4(),
                a: Vec2::new(2.05, 0.0),
                b: Vec2::new(5.0, 0.0),
                thickness_m: 0.15,
                height_m: 2.7,
            },
            WallSegment {
                id: Uuid::new_v4(),
                a: Vec2::new(0.0, 0.0),
                b: Vec2::new(0.05, 0.0),
                thickness_m: 0.15,
                height_m: 2.7,
            },
        ];
        simplify_ir(&mut ir, 0.2, 0.15);
        assert_eq!(ir.walls.len(), 1);
        assert!((wall_length(&ir.walls[0]) - 5.0).abs() < 0.2);
    }
}
