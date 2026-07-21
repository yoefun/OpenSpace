//! Apply photo-derived colors/textures onto a MeshScene.

use image::{DynamicImage, EncodableLayout, ImageBuffer, Rgba};
use openspace_core::{PhotoRef, Result};
use openspace_mesh::{MeshKind, MeshScene};
use std::collections::HashMap;
use std::path::Path;
use uuid::Uuid;

/// Sample dominant colors from photos and assign to room floors/walls.
pub fn apply_photo_textures(
    scene: &mut MeshScene,
    photos: &[PhotoRef],
    data_root: &Path,
) -> Result<()> {
    let mut room_colors: HashMap<Uuid, [f32; 4]> = HashMap::new();
    let mut room_png: HashMap<Uuid, Vec<u8>> = HashMap::new();

    for photo in photos {
        let Some(room_id) = photo.room_id else {
            continue;
        };
        let path = if Path::new(&photo.path).is_absolute() {
            Path::new(&photo.path).to_path_buf()
        } else {
            data_root.join(&photo.path)
        };
        if !path.exists() {
            tracing::warn!("photo missing: {}", path.display());
            continue;
        }
        match image::open(&path) {
            Ok(img) => {
                let color = dominant_color(&img);
                room_colors.insert(room_id, color);
                if let Ok(png) = make_swatch_png(&img, 64) {
                    room_png.insert(room_id, png);
                }
            }
            Err(e) => tracing::warn!("failed to open photo {}: {e}", path.display()),
        }
    }

    // Also match by room_name if room_id not set — handled at API layer preferably.

    for mesh in &mut scene.meshes {
        let Some(rid) = mesh.room_id else {
            // Walls without room: if any photo color exists, tint walls slightly from first
            if matches!(mesh.kind, MeshKind::Wall) {
                if let Some((_, c)) = room_colors.iter().next() {
                    mesh.color = [
                        (mesh.color[0] * 0.4 + c[0] * 0.6),
                        (mesh.color[1] * 0.4 + c[1] * 0.6),
                        (mesh.color[2] * 0.4 + c[2] * 0.6),
                        1.0,
                    ];
                }
            }
            continue;
        };

        if let Some(c) = room_colors.get(&rid) {
            match mesh.kind {
                MeshKind::Floor => {
                    mesh.color = *c;
                    if let Some(png) = room_png.get(&rid) {
                        mesh.texture_png = Some(png.clone());
                    }
                }
                MeshKind::Ceiling => {
                    mesh.color = [
                        (c[0] * 0.3 + 0.7).min(1.0),
                        (c[1] * 0.3 + 0.7).min(1.0),
                        (c[2] * 0.3 + 0.7).min(1.0),
                        1.0,
                    ];
                }
                MeshKind::Wall | MeshKind::OpeningFrame => {
                    mesh.color = [
                        (c[0] * 0.5 + 0.4).min(1.0),
                        (c[1] * 0.5 + 0.4).min(1.0),
                        (c[2] * 0.5 + 0.4).min(1.0),
                        1.0,
                    ];
                }
            }
        }
    }

    Ok(())
}

fn dominant_color(img: &DynamicImage) -> [f32; 4] {
    let rgb = img.to_rgb8();
    let (w, h) = rgb.dimensions();
    // Sample center region to avoid extreme edges
    let x0 = w / 4;
    let y0 = h / 4;
    let x1 = w * 3 / 4;
    let y1 = h * 3 / 4;
    let mut r = 0u64;
    let mut g = 0u64;
    let mut b = 0u64;
    let mut n = 0u64;
    for y in y0..y1 {
        for x in x0..x1 {
            let p = rgb.get_pixel(x, y);
            r += p[0] as u64;
            g += p[1] as u64;
            b += p[2] as u64;
            n += 1;
        }
    }
    if n == 0 {
        return [0.7, 0.7, 0.7, 1.0];
    }
    [
        (r as f32 / n as f32) / 255.0,
        (g as f32 / n as f32) / 255.0,
        (b as f32 / n as f32) / 255.0,
        1.0,
    ]
}

fn make_swatch_png(img: &DynamicImage, size: u32) -> Result<Vec<u8>> {
    let color = dominant_color(img);
    let r = (color[0] * 255.0) as u8;
    let g = (color[1] * 255.0) as u8;
    let b = (color[2] * 255.0) as u8;
    let buf: ImageBuffer<Rgba<u8>, Vec<u8>> =
        ImageBuffer::from_fn(size, size, |x, y| {
            // subtle noise variation from source image sample
            let src = img.resize_exact(size, size, image::imageops::FilterType::Triangle);
            let rgba = src.to_rgba8();
            let p = rgba.get_pixel(x, y);
            Rgba([
                ((p[0] as u16 + r as u16) / 2) as u8,
                ((p[1] as u16 + g as u16) / 2) as u8,
                ((p[2] as u16 + b as u16) / 2) as u8,
                255,
            ])
        });
    let mut out = Vec::new();
    let enc = image::codecs::png::PngEncoder::new(&mut out);
    use image::ImageEncoder;
    enc.write_image(buf.as_bytes(), size, size, image::ExtendedColorType::Rgba8)
        .map_err(|e| openspace_core::OpenSpaceError::Io(e.to_string()))?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Vec2;
    use openspace_core::{FloorplanIR, Room, WallSegment};
    use openspace_mesh::extrude;
    use uuid::Uuid;

    #[test]
    fn applies_color_from_photo() {
        let dir = std::env::temp_dir().join("openspace-texture-test");
        std::fs::create_dir_all(&dir).unwrap();
        let photo_path = dir.join("wall.jpg");
        // Solid reddish image
        let img = ImageBuffer::from_fn(32, 32, |_x, _y| Rgba([200u8, 60, 60, 255]));
        DynamicImage::ImageRgba8(img)
            .save(&photo_path)
            .unwrap();

        let room_id = Uuid::new_v4();
        let w1 = Uuid::new_v4();
        let ir = FloorplanIR {
            walls: vec![WallSegment {
                id: w1,
                a: Vec2::ZERO,
                b: Vec2::new(3.0, 0.0),
                thickness_m: 0.15,
                height_m: 2.7,
            }],
            openings: vec![],
            rooms: vec![Room {
                id: room_id,
                name: "R".into(),
                polygon: vec![
                    Vec2::ZERO,
                    Vec2::new(3.0, 0.0),
                    Vec2::new(3.0, 3.0),
                    Vec2::new(0.0, 3.0),
                ],
                wall_ids: vec![w1],
            }],
            ..FloorplanIR::default()
        };
        let mut scene = extrude(&ir).unwrap();
        let photos = vec![PhotoRef {
            id: Uuid::new_v4(),
            path: photo_path.to_string_lossy().into(),
            room_id: Some(room_id),
            room_name: Some("R".into()),
            notes: None,
        }];
        apply_photo_textures(&mut scene, &photos, &dir).unwrap();
        let floor = scene
            .meshes
            .iter()
            .find(|m| matches!(m.kind, MeshKind::Floor))
            .unwrap();
        assert!(floor.color[0] > 0.5, "expected reddish floor, got {:?}", floor.color);
        assert!(floor.texture_png.is_some());
    }
}
