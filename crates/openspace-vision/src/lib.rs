//! Raster floorplan detection → draft FloorplanIR (classical CV, pure Rust).

use glam::Vec2;
use image::{DynamicImage, GrayImage, Luma};
use openspace_core::{simplify_ir, EntityId, FloorplanIR, OpenSpaceError, Result, Room, WallSegment};
use uuid::Uuid;

/// Detect walls and rooms from a floorplan raster image.
pub fn detect_floorplan(img: &DynamicImage) -> Result<FloorplanIR> {
    let gray = img.to_luma8();
    let (w, h) = gray.dimensions();
    if w < 8 || h < 8 {
        return Err(OpenSpaceError::InvalidFloorplan(
            "image too small".into(),
        ));
    }

    let binary = adaptive_threshold(&gray);
    let edges = simple_edges(&binary);
    let mut segments = extract_line_segments(&edges);
    orthogonalize(&mut segments);
    let snap_tol = (w.min(h) as f32 * 0.012).clamp(4.0, 12.0);
    snap_endpoints(&mut segments, snap_tol);
    merge_collinear(&mut segments, snap_tol, snap_tol * 1.2);
    segments = dedupe_segments(&segments, snap_tol * 0.8);

    let scale = estimate_scale(w, h);
    let height = 2.7_f32;
    let thickness = 0.15_f32;
    let min_px = (w.min(h) as f32 * 0.035).max(20.0);

    let walls: Vec<WallSegment> = segments
        .iter()
        .filter(|s| s.length() >= min_px)
        .map(|s| WallSegment {
            id: Uuid::new_v4(),
            a: Vec2::new(s.x0 * scale, s.y0 * scale),
            b: Vec2::new(s.x1 * scale, s.y1 * scale),
            thickness_m: thickness,
            height_m: height,
        })
        .collect();

    let mut ir = FloorplanIR {
        walls,
        openings: Vec::new(),
        rooms: Vec::new(),
        scale_m_per_px: scale,
        default_wall_height_m: height,
        default_wall_thickness_m: thickness,
    };
    simplify_ir(&mut ir, min_px * scale * 0.85, snap_tol * scale);
    ir.rooms = infer_rooms_from_walls(&ir.walls, scale);
    if ir.rooms.is_empty() && !ir.walls.is_empty() {
        simplify_ir(&mut ir, min_px * scale * 0.5, snap_tol * scale * 1.5);
        ir.rooms = infer_rooms_from_walls(&ir.walls, scale);
    }
    Ok(ir)
}

fn estimate_scale(w: u32, h: u32) -> f32 {
    // Assume longer side ~ 12 meters for a typical apartment plan.
    let longer = w.max(h) as f32;
    (12.0 / longer).clamp(0.002, 0.05)
}

fn adaptive_threshold(gray: &GrayImage) -> GrayImage {
    let (w, h) = gray.dimensions();
    let mut out = GrayImage::new(w, h);
    let mut sum: u64 = 0;
    for p in gray.pixels() {
        sum += p[0] as u64;
    }
    let mean = (sum / (w as u64 * h as u64).max(1)) as u8;
    // Floorplans: walls are usually dark lines on light background.
    let thresh = mean.saturating_sub(20);
    for (x, y, p) in gray.enumerate_pixels() {
        let v = if p[0] < thresh { 255 } else { 0 };
        out.put_pixel(x, y, Luma([v]));
    }
    // Light morphological close: dilate then erode to connect gaps.
    dilate(&mut out, 1);
    erode(&mut out, 1);
    out
}

fn dilate(img: &mut GrayImage, r: i32) {
    let (w, h) = img.dimensions();
    let copy = img.clone();
    for y in 0..h as i32 {
        for x in 0..w as i32 {
            let mut maxv = 0u8;
            for dy in -r..=r {
                for dx in -r..=r {
                    let nx = x + dx;
                    let ny = y + dy;
                    if nx >= 0 && ny >= 0 && nx < w as i32 && ny < h as i32 {
                        maxv = maxv.max(copy.get_pixel(nx as u32, ny as u32)[0]);
                    }
                }
            }
            img.put_pixel(x as u32, y as u32, Luma([maxv]));
        }
    }
}

fn erode(img: &mut GrayImage, r: i32) {
    let (w, h) = img.dimensions();
    let copy = img.clone();
    for y in 0..h as i32 {
        for x in 0..w as i32 {
            let mut minv = 255u8;
            for dy in -r..=r {
                for dx in -r..=r {
                    let nx = x + dx;
                    let ny = y + dy;
                    if nx >= 0 && ny >= 0 && nx < w as i32 && ny < h as i32 {
                        minv = minv.min(copy.get_pixel(nx as u32, ny as u32)[0]);
                    }
                }
            }
            img.put_pixel(x as u32, y as u32, Luma([minv]));
        }
    }
}

fn simple_edges(binary: &GrayImage) -> GrayImage {
    let (w, h) = binary.dimensions();
    let mut edges = GrayImage::new(w, h);
    for y in 1..h - 1 {
        for x in 1..w - 1 {
            let c = binary.get_pixel(x, y)[0];
            if c == 0 {
                continue;
            }
            let neighbors = [
                binary.get_pixel(x - 1, y)[0],
                binary.get_pixel(x + 1, y)[0],
                binary.get_pixel(x, y - 1)[0],
                binary.get_pixel(x, y + 1)[0],
            ];
            if neighbors.iter().any(|&n| n == 0) {
                edges.put_pixel(x, y, Luma([255]));
            }
        }
    }
    edges
}

#[derive(Clone, Debug)]
struct Seg {
    x0: f32,
    y0: f32,
    x1: f32,
    y1: f32,
}

impl Seg {
    fn length(&self) -> f32 {
        ((self.x1 - self.x0).hypot(self.y1 - self.y0)).abs()
    }

    fn angle_deg(&self) -> f32 {
        (self.y1 - self.y0).atan2(self.x1 - self.x0).to_degrees()
    }
}

fn extract_line_segments(edges: &GrayImage) -> Vec<Seg> {
    let (w, h) = edges.dimensions();
    // Horizontal runs
    let mut segs = Vec::new();
    for y in 0..h {
        let mut x = 0u32;
        while x < w {
            while x < w && edges.get_pixel(x, y)[0] == 0 {
                x += 1;
            }
            if x >= w {
                break;
            }
            let start = x;
            while x < w && edges.get_pixel(x, y)[0] > 0 {
                x += 1;
            }
            let end = x;
            if end - start >= 15 {
                segs.push(Seg {
                    x0: start as f32,
                    y0: y as f32,
                    x1: (end - 1) as f32,
                    y1: y as f32,
                });
            }
        }
    }
    // Vertical runs
    for x in 0..w {
        let mut y = 0u32;
        while y < h {
            while y < h && edges.get_pixel(x, y)[0] == 0 {
                y += 1;
            }
            if y >= h {
                break;
            }
            let start = y;
            while y < h && edges.get_pixel(x, y)[0] > 0 {
                y += 1;
            }
            let end = y;
            if end - start >= 15 {
                segs.push(Seg {
                    x0: x as f32,
                    y0: start as f32,
                    x1: x as f32,
                    y1: (end - 1) as f32,
                });
            }
        }
    }
    segs
}

fn orthogonalize(segs: &mut [Seg]) {
    for s in segs.iter_mut() {
        let ang = s.angle_deg().abs() % 180.0;
        let horiz = ang < 45.0 || ang > 135.0;
        if horiz {
            let y = ((s.y0 + s.y1) * 0.5).round();
            s.y0 = y;
            s.y1 = y;
        } else {
            let x = ((s.x0 + s.x1) * 0.5).round();
            s.x0 = x;
            s.x1 = x;
        }
    }
}

fn snap_endpoints(segs: &mut [Seg], tol: f32) {
    let mut pts: Vec<(f32, f32)> = Vec::new();
    for s in segs.iter() {
        pts.push((s.x0, s.y0));
        pts.push((s.x1, s.y1));
    }
    // Cluster points
    let mut anchors: Vec<(f32, f32)> = Vec::new();
    for (x, y) in pts {
        if let Some(a) = anchors
            .iter_mut()
            .find(|(ax, ay)| (ax - x).abs() < tol && (ay - y).abs() < tol)
        {
            a.0 = (a.0 + x) * 0.5;
            a.1 = (a.1 + y) * 0.5;
        } else {
            anchors.push((x, y));
        }
    }
    let snap = |x: f32, y: f32| -> (f32, f32) {
        anchors
            .iter()
            .min_by(|a, b| {
                let da = (a.0 - x).hypot(a.1 - y);
                let db = (b.0 - x).hypot(b.1 - y);
                da.partial_cmp(&db).unwrap()
            })
            .copied()
            .unwrap_or((x, y))
    };
    for s in segs.iter_mut() {
        let (x0, y0) = snap(s.x0, s.y0);
        let (x1, y1) = snap(s.x1, s.y1);
        s.x0 = x0;
        s.y0 = y0;
        s.x1 = x1;
        s.y1 = y1;
    }
}

/// Keep the longest segment per orientation/position bucket (reduces double-line noise).
fn dedupe_segments(segs: &[Seg], bucket: f32) -> Vec<Seg> {
    let mut kept: Vec<Seg> = Vec::new();
    for s in segs {
        let horiz = is_horiz(s);
        let key = if horiz {
            (true, (s.y0 / bucket).round() as i32)
        } else {
            (false, (s.x0 / bucket).round() as i32)
        };
        if let Some(existing) = kept.iter_mut().find(|e| {
            let eh = is_horiz(e);
            let ek = if eh {
                (true, (e.y0 / bucket).round() as i32)
            } else {
                (false, (e.x0 / bucket).round() as i32)
            };
            ek == key
        }) {
            if s.length() > existing.length() {
                *existing = s.clone();
            }
        } else {
            kept.push(s.clone());
        }
    }
    kept
}

fn merge_collinear(segs: &mut Vec<Seg>, dist_tol: f32, gap_tol: f32) {
    let mut changed = true;
    while changed {
        changed = false;
        'outer: for i in 0..segs.len() {
            for j in (i + 1)..segs.len() {
                if try_merge(&segs[i], &segs[j], dist_tol, gap_tol) {
                    let merged = merge_seg(&segs[i], &segs[j]);
                    segs[i] = merged;
                    segs.remove(j);
                    changed = true;
                    break 'outer;
                }
            }
        }
    }
}

fn is_horiz(s: &Seg) -> bool {
    (s.y0 - s.y1).abs() < 1.0
}

fn try_merge(a: &Seg, b: &Seg, dist_tol: f32, gap_tol: f32) -> bool {
    if is_horiz(a) != is_horiz(b) {
        return false;
    }
    if is_horiz(a) {
        if (a.y0 - b.y0).abs() > dist_tol {
            return false;
        }
        let a_min = a.x0.min(a.x1);
        let a_max = a.x0.max(a.x1);
        let b_min = b.x0.min(b.x1);
        let b_max = b.x0.max(b.x1);
        gap_overlap(a_min, a_max, b_min, b_max, gap_tol)
    } else {
        if (a.x0 - b.x0).abs() > dist_tol {
            return false;
        }
        let a_min = a.y0.min(a.y1);
        let a_max = a.y0.max(a.y1);
        let b_min = b.y0.min(b.y1);
        let b_max = b.y0.max(b.y1);
        gap_overlap(a_min, a_max, b_min, b_max, gap_tol)
    }
}

fn gap_overlap(a0: f32, a1: f32, b0: f32, b1: f32, gap: f32) -> bool {
    let left = a0.max(b0);
    let right = a1.min(b1);
    if left <= right {
        return true;
    }
    (left - right) <= gap
}

fn merge_seg(a: &Seg, b: &Seg) -> Seg {
    if is_horiz(a) {
        let y = (a.y0 + b.y0) * 0.5;
        let xmin = a.x0.min(a.x1).min(b.x0).min(b.x1);
        let xmax = a.x0.max(a.x1).max(b.x0).max(b.x1);
        Seg {
            x0: xmin,
            y0: y,
            x1: xmax,
            y1: y,
        }
    } else {
        let x = (a.x0 + b.x0) * 0.5;
        let ymin = a.y0.min(a.y1).min(b.y0).min(b.y1);
        let ymax = a.y0.max(a.y1).max(b.y0).max(b.y1);
        Seg {
            x0: x,
            y0: ymin,
            x1: x,
            y1: ymax,
        }
    }
}

/// Infer axis-aligned rooms by finding closed rectangles from wall endpoints.
fn infer_rooms_from_walls(walls: &[WallSegment], _scale: f32) -> Vec<Room> {
    if walls.is_empty() {
        return Vec::new();
    }

    let mut xs: Vec<f32> = Vec::new();
    let mut ys: Vec<f32> = Vec::new();
    for w in walls {
        xs.push(w.a.x);
        xs.push(w.b.x);
        ys.push(w.a.y);
        ys.push(w.b.y);
    }
    xs.sort_by(|a, b| a.partial_cmp(b).unwrap());
    ys.sort_by(|a, b| a.partial_cmp(b).unwrap());
    xs.dedup_by(|a, b| (*a - *b).abs() < 0.05);
    ys.dedup_by(|a, b| (*a - *b).abs() < 0.05);

    if xs.len() < 2 || ys.len() < 2 {
        // Fallback: bounding box as one room
        let min_x = xs[0];
        let max_x = *xs.last().unwrap_or(&min_x);
        let min_y = ys[0];
        let max_y = *ys.last().unwrap_or(&min_y);
        return vec![Room {
            id: Uuid::new_v4(),
            name: "Room 1".into(),
            polygon: vec![
                Vec2::new(min_x, min_y),
                Vec2::new(max_x, min_y),
                Vec2::new(max_x, max_y),
                Vec2::new(min_x, max_y),
            ],
            wall_ids: walls.iter().map(|w| w.id).collect(),
        }];
    }

    let mut rooms = Vec::new();
    let mut idx = 1usize;
    for i in 0..xs.len() - 1 {
        for j in 0..ys.len() - 1 {
            let x0 = xs[i];
            let x1 = xs[i + 1];
            let y0 = ys[j];
            let y1 = ys[j + 1];
            if (x1 - x0).abs() < 0.3 || (y1 - y0).abs() < 0.3 {
                continue;
            }
            // Cell is a room if surrounded by walls approximately
            let has_bottom = wall_covers(walls, x0, y0, x1, y0);
            let has_top = wall_covers(walls, x0, y1, x1, y1);
            let has_left = wall_covers(walls, x0, y0, x0, y1);
            let has_right = wall_covers(walls, x1, y0, x1, y1);
            let score = [has_bottom, has_top, has_left, has_right]
                .iter()
                .filter(|&&b| b)
                .count();
            if score >= 3 {
                let poly = vec![
                    Vec2::new(x0, y0),
                    Vec2::new(x1, y0),
                    Vec2::new(x1, y1),
                    Vec2::new(x0, y1),
                ];
                let wall_ids = walls
                    .iter()
                    .filter(|w| wall_near_poly(w, &poly))
                    .map(|w| w.id)
                    .collect::<Vec<EntityId>>();
                rooms.push(Room {
                    id: Uuid::new_v4(),
                    name: format!("Room {idx}"),
                    polygon: poly,
                    wall_ids,
                });
                idx += 1;
            }
        }
    }

    if rooms.is_empty() {
        let min_x = xs[0];
        let max_x = *xs.last().unwrap();
        let min_y = ys[0];
        let max_y = *ys.last().unwrap();
        rooms.push(Room {
            id: Uuid::new_v4(),
            name: "Room 1".into(),
            polygon: vec![
                Vec2::new(min_x, min_y),
                Vec2::new(max_x, min_y),
                Vec2::new(max_x, max_y),
                Vec2::new(min_x, max_y),
            ],
            wall_ids: walls.iter().map(|w| w.id).collect(),
        });
    }
    rooms
}

fn wall_covers(walls: &[WallSegment], x0: f32, y0: f32, x1: f32, y1: f32) -> bool {
    let target_len = (x1 - x0).hypot(y1 - y0);
    if target_len < 1e-3 {
        return false;
    }
    let horiz = (y0 - y1).abs() < 1e-3;
    for w in walls {
        let wh = (w.a.y - w.b.y).abs() < 0.05;
        if horiz != wh {
            continue;
        }
        if horiz {
            if (w.a.y - y0).abs() > 0.15 && (w.b.y - y0).abs() > 0.15 {
                continue;
            }
            let a0 = w.a.x.min(w.b.x);
            let a1 = w.a.x.max(w.b.x);
            let b0 = x0.min(x1);
            let b1 = x0.max(x1);
            let overlap = a1.min(b1) - a0.max(b0);
            if overlap > target_len * 0.4 {
                return true;
            }
        } else {
            if (w.a.x - x0).abs() > 0.15 && (w.b.x - x0).abs() > 0.15 {
                continue;
            }
            let a0 = w.a.y.min(w.b.y);
            let a1 = w.a.y.max(w.b.y);
            let b0 = y0.min(y1);
            let b1 = y0.max(y1);
            let overlap = a1.min(b1) - a0.max(b0);
            if overlap > target_len * 0.4 {
                return true;
            }
        }
    }
    false
}

fn wall_near_poly(w: &WallSegment, poly: &[Vec2]) -> bool {
    let min_x = poly.iter().map(|p| p.x).fold(f32::INFINITY, f32::min);
    let max_x = poly.iter().map(|p| p.x).fold(f32::NEG_INFINITY, f32::max);
    let min_y = poly.iter().map(|p| p.y).fold(f32::INFINITY, f32::min);
    let max_y = poly.iter().map(|p| p.y).fold(f32::NEG_INFINITY, f32::max);
    let mx = (w.a.x + w.b.x) * 0.5;
    let my = (w.a.y + w.b.y) * 0.5;
    mx >= min_x - 0.2 && mx <= max_x + 0.2 && my >= min_y - 0.2 && my <= max_y + 0.2
}

/// Create a synthetic orthogonal floorplan PNG for tests/fixtures.
pub fn write_synthetic_floorplan(path: &std::path::Path, width: u32, height: u32) -> Result<()> {
    let mut img = GrayImage::from_pixel(width, height, Luma([255]));
    // Outer rectangle walls (dark)
    draw_rect_border(&mut img, 40, 40, width - 40, height - 40, 4);
    // Interior wall
    draw_hline(&mut img, 40, width / 2, height / 2, 4);
    draw_vline(&mut img, width / 2, 40, height / 2, 4);
    DynamicImage::ImageLuma8(img)
        .save(path)
        .map_err(|e| OpenSpaceError::Io(e.to_string()))?;
    Ok(())
}

fn draw_rect_border(img: &mut GrayImage, x0: u32, y0: u32, x1: u32, y1: u32, t: u32) {
    draw_hline(img, x0, x1, y0, t);
    draw_hline(img, x0, x1, y1, t);
    draw_vline(img, x0, y0, y1, t);
    draw_vline(img, x1, y0, y1, t);
}

fn draw_hline(img: &mut GrayImage, x0: u32, x1: u32, y: u32, t: u32) {
    let (w, h) = img.dimensions();
    let lo = x0.min(x1);
    let hi = x0.max(x1);
    for yy in y.saturating_sub(t / 2)..=(y + t / 2).min(h - 1) {
        for x in lo..=hi.min(w - 1) {
            img.put_pixel(x, yy, Luma([0]));
        }
    }
}

fn draw_vline(img: &mut GrayImage, x: u32, y0: u32, y1: u32, t: u32) {
    let (w, h) = img.dimensions();
    let lo = y0.min(y1);
    let hi = y0.max(y1);
    for xx in x.saturating_sub(t / 2)..=(x + t / 2).min(w - 1) {
        for y in lo..=hi.min(h - 1) {
            img.put_pixel(xx, y, Luma([0]));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn detects_walls_on_synthetic_floorplan() {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/floorplans");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("synthetic_ortho.png");
        write_synthetic_floorplan(&path, 400, 300).unwrap();
        let img = image::open(&path).unwrap();
        let ir = detect_floorplan(&img).unwrap();
        assert!(
            ir.walls.len() >= 4,
            "expected >=4 walls, got {}",
            ir.walls.len()
        );
        assert!(
            !ir.rooms.is_empty(),
            "expected at least one room"
        );
    }

    #[test]
    fn tiny_image_errors() {
        let img = DynamicImage::ImageLuma8(GrayImage::new(4, 4));
        assert!(detect_floorplan(&img).is_err());
    }
}
