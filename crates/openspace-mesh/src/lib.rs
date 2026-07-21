//! Extrude FloorplanIR into a mesh scene and export glTF/GLB.

use glam::{Vec2, Vec3};
use openspace_core::{FloorplanIR, OpeningKind, OpenSpaceError, Result, Room};
use serde::Serialize;
use std::collections::HashMap;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct MeshScene {
    pub meshes: Vec<NamedMesh>,
}

#[derive(Debug, Clone)]
pub struct NamedMesh {
    pub name: String,
    pub room_id: Option<Uuid>,
    pub kind: MeshKind,
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub uvs: Vec<[f32; 2]>,
    pub indices: Vec<u32>,
    pub color: [f32; 4],
    pub texture_png: Option<Vec<u8>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MeshKind {
    Wall,
    Floor,
    Ceiling,
    OpeningFrame,
}

/// Options for mesh extrusion (MVP defaults to open-top dollhouse view).
#[derive(Debug, Clone, Copy)]
pub struct ExtrudeOptions {
    pub include_ceiling: bool,
    pub cap_wall_tops: bool,
}

impl Default for ExtrudeOptions {
    fn default() -> Self {
        Self {
            include_ceiling: false,
            cap_wall_tops: false,
        }
    }
}

/// Extrude walls, floors, and optional ceilings from IR (meters).
pub fn extrude(ir: &FloorplanIR) -> Result<MeshScene> {
    extrude_with_options(ir, &ExtrudeOptions::default())
}

pub fn extrude_with_options(ir: &FloorplanIR, opts: &ExtrudeOptions) -> Result<MeshScene> {
    if ir.walls.is_empty() && ir.rooms.is_empty() {
        return Err(OpenSpaceError::InvalidFloorplan(
            "empty IR: add walls or rooms before build".into(),
        ));
    }

    let mut meshes = Vec::new();

    for wall in &ir.walls {
        meshes.push(extrude_wall(
            wall.a,
            wall.b,
            wall.thickness_m,
            wall.height_m,
            wall.id,
            opts.cap_wall_tops,
        ));
    }

    for opening in &ir.openings {
        if let Some(wall) = ir.walls.iter().find(|w| w.id == opening.wall_id) {
            let t_mid = (opening.t0 + opening.t1) * 0.5;
            let dir = wall.b - wall.a;
            let pos = wall.a + dir * t_mid;
            let sill = opening.sill_m.unwrap_or(match opening.kind {
                OpeningKind::Door => 0.0,
                OpeningKind::Window => 0.9,
            });
            let width = (opening.t1 - opening.t0).abs() * dir.length();
            meshes.push(opening_frame(
                pos,
                dir,
                wall.thickness_m,
                sill,
                opening.height_m,
                width.max(0.6),
                opening.kind,
                opening.id,
            ));
        }
    }

    for room in &ir.rooms {
        if room.polygon.len() < 3 {
            continue;
        }
        meshes.push(polygon_slab(room, 0.0, true, MeshKind::Floor));
        if opts.include_ceiling {
            meshes.push(polygon_slab(
                room,
                ir.default_wall_height_m,
                false,
                MeshKind::Ceiling,
            ));
        }
    }

    if ir.rooms.is_empty() && !ir.walls.is_empty() {
        let mut min = Vec2::splat(f32::INFINITY);
        let mut max = Vec2::splat(f32::NEG_INFINITY);
        for w in &ir.walls {
            min = min.min(w.a).min(w.b);
            max = max.max(w.a).max(w.b);
        }
        let room = Room {
            id: Uuid::new_v4(),
            name: "Room 1".into(),
            polygon: vec![
                Vec2::new(min.x, min.y),
                Vec2::new(max.x, min.y),
                Vec2::new(max.x, max.y),
                Vec2::new(min.x, max.y),
            ],
            wall_ids: ir.walls.iter().map(|w| w.id).collect(),
        };
        meshes.push(polygon_slab(&room, 0.0, true, MeshKind::Floor));
        if opts.include_ceiling {
            meshes.push(polygon_slab(
                &room,
                ir.default_wall_height_m,
                false,
                MeshKind::Ceiling,
            ));
        }
    }

    Ok(MeshScene { meshes })
}

fn extrude_wall(a: Vec2, b: Vec2, thickness: f32, height: f32, id: Uuid, cap_top: bool) -> NamedMesh {
    let dir = (b - a).normalize_or_zero();
    let normal = Vec2::new(-dir.y, dir.x);
    let half = thickness * 0.5;
    let bottom = [
        a - normal * half,
        a + normal * half,
        b + normal * half,
        b - normal * half,
    ];

    let mut positions = Vec::new();
    let mut normals = Vec::new();
    let mut uvs = Vec::new();
    let mut indices = Vec::new();

    let corners_b: Vec<Vec3> = bottom
        .iter()
        .map(|p| Vec3::new(p.x, 0.0, p.y))
        .collect();
    let corners_t: Vec<Vec3> = bottom
        .iter()
        .map(|p| Vec3::new(p.x, height, p.y))
        .collect();

    let push_quad = |positions: &mut Vec<[f32; 3]>,
                     normals: &mut Vec<[f32; 3]>,
                     uvs: &mut Vec<[f32; 2]>,
                     indices: &mut Vec<u32>,
                     a: Vec3,
                     b: Vec3,
                     c: Vec3,
                     d: Vec3,
                     n: Vec3| {
        let base = positions.len() as u32;
        for (p, uv) in [
            (a, [0.0_f32, 0.0]),
            (b, [1.0, 0.0]),
            (c, [1.0, 1.0]),
            (d, [0.0, 1.0]),
        ] {
            positions.push(p.into());
            normals.push(n.into());
            uvs.push(uv);
        }
        indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    };

    for i in 0..4 {
        let j = (i + 1) % 4;
        let a = corners_b[i];
        let b = corners_b[j];
        let c = corners_t[j];
        let d = corners_t[i];
        let n = (b - a).cross(d - a).normalize_or_zero();
        push_quad(
            &mut positions,
            &mut normals,
            &mut uvs,
            &mut indices,
            a,
            b,
            c,
            d,
            n,
        );
    }
    if cap_top {
        push_quad(
            &mut positions,
            &mut normals,
            &mut uvs,
            &mut indices,
            corners_t[0],
            corners_t[1],
            corners_t[2],
            corners_t[3],
            Vec3::Y,
        );
    }
    push_quad(
        &mut positions,
        &mut normals,
        &mut uvs,
        &mut indices,
        corners_b[0],
        corners_b[3],
        corners_b[2],
        corners_b[1],
        -Vec3::Y,
    );

    NamedMesh {
        name: format!("wall-{id}"),
        room_id: None,
        kind: MeshKind::Wall,
        positions,
        normals,
        uvs,
        indices,
        color: [0.82, 0.82, 0.78, 1.0],
        texture_png: None,
    }
}

fn opening_frame(
    pos: Vec2,
    dir: Vec2,
    thickness: f32,
    sill: f32,
    height: f32,
    width: f32,
    kind: OpeningKind,
    id: Uuid,
) -> NamedMesh {
    let dir_n = dir.normalize_or_zero();
    let n = Vec2::new(-dir_n.y, dir_n.x);
    let half_w = width * 0.5;
    let half_t = thickness * 0.55;
    let corners = [
        pos - dir_n * half_w - n * half_t,
        pos + dir_n * half_w - n * half_t,
        pos + dir_n * half_w + n * half_t,
        pos - dir_n * half_w + n * half_t,
    ];

    let mut positions = Vec::new();
    let mut normals = Vec::new();
    let mut uvs = Vec::new();
    let mut indices = Vec::new();

    let y0 = sill;
    let y1 = sill + height;
    let corners_b: Vec<Vec3> = corners
        .iter()
        .map(|p| Vec3::new(p.x, y0, p.y))
        .collect();
    let corners_t: Vec<Vec3> = corners
        .iter()
        .map(|p| Vec3::new(p.x, y1, p.y))
        .collect();

    for i in 0..4 {
        let j = (i + 1) % 4;
        let base = positions.len() as u32;
        let aa = corners_b[i];
        let bb = corners_b[j];
        let cc = corners_t[j];
        let dd = corners_t[i];
        let nn = (bb - aa).cross(dd - aa).normalize_or_zero();
        for (p, uv) in [
            (aa, [0.0_f32, 0.0]),
            (bb, [1.0, 0.0]),
            (cc, [1.0, 1.0]),
            (dd, [0.0, 1.0]),
        ] {
            positions.push(p.into());
            normals.push(nn.into());
            uvs.push(uv);
        }
        indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }

    let color = match kind {
        OpeningKind::Door => [0.45, 0.28, 0.12, 1.0],
        OpeningKind::Window => [0.55, 0.75, 0.9, 0.85],
    };

    NamedMesh {
        name: format!("opening-{id}"),
        room_id: None,
        kind: MeshKind::OpeningFrame,
        positions,
        normals,
        uvs,
        indices,
        color,
        texture_png: None,
    }
}

fn polygon_slab(room: &Room, y: f32, upward: bool, kind: MeshKind) -> NamedMesh {
    let poly = &room.polygon;
    let mut positions = Vec::new();
    let mut normals = Vec::new();
    let mut uvs = Vec::new();
    let mut indices = Vec::new();

    let n = if upward { Vec3::Y } else { -Vec3::Y };
    let min_x = poly.iter().map(|p| p.x).fold(f32::INFINITY, f32::min);
    let min_y = poly.iter().map(|p| p.y).fold(f32::INFINITY, f32::min);
    let max_x = poly.iter().map(|p| p.x).fold(f32::NEG_INFINITY, f32::max);
    let max_y = poly.iter().map(|p| p.y).fold(f32::NEG_INFINITY, f32::max);
    let sx = (max_x - min_x).max(1e-3);
    let sy = (max_y - min_y).max(1e-3);

    for p in poly {
        positions.push([p.x, y, p.y]);
        normals.push(n.into());
        uvs.push([(p.x - min_x) / sx, (p.y - min_y) / sy]);
    }
    for i in 1..(poly.len() as u32 - 1) {
        if upward {
            indices.extend_from_slice(&[0, i, i + 1]);
        } else {
            indices.extend_from_slice(&[0, i + 1, i]);
        }
    }

    let color = match kind {
        MeshKind::Floor => [0.72, 0.68, 0.6, 1.0],
        MeshKind::Ceiling => [0.95, 0.95, 0.93, 1.0],
        _ => [0.8, 0.8, 0.8, 1.0],
    };

    let prefix = if matches!(kind, MeshKind::Floor) {
        "floor"
    } else {
        "ceiling"
    };

    NamedMesh {
        name: format!("{prefix}-{}", room.name),
        room_id: Some(room.id),
        kind,
        positions,
        normals,
        uvs,
        indices,
        color,
        texture_png: None,
    }
}

/// Export MeshScene as a binary GLB (glTF 2.0).
pub fn export_glb(scene: &MeshScene) -> Result<Vec<u8>> {
    let mut bin = Vec::new();
    let mut buffer_views = Vec::new();
    let mut accessors = Vec::new();
    let mut meshes_json = Vec::new();
    let mut materials = Vec::new();
    let mut textures = Vec::new();
    let mut images = Vec::new();
    let mut nodes = Vec::new();
    let mut node_indices = Vec::new();

    for (mi, mesh) in scene.meshes.iter().enumerate() {
        let mat_index = materials.len();
        let mut mat = GltfMaterial {
            name: Some(format!("mat-{}", mesh.name)),
            pbr_metallic_roughness: Pbr {
                base_color_factor: Some(mesh.color),
                metallic_factor: 0.0,
                roughness_factor: 0.85,
                base_color_texture: None,
            },
            double_sided: true,
        };

        if let Some(png) = &mesh.texture_png {
            let img_index = images.len();
            let offset = align4(bin.len());
            pad_to(&mut bin, offset);
            let start = bin.len();
            bin.extend_from_slice(png);
            buffer_views.push(BufferView {
                buffer: 0,
                byte_offset: start,
                byte_length: png.len(),
                target: None,
            });
            images.push(GltfImage {
                buffer_view: buffer_views.len() - 1,
                mime_type: "image/png".into(),
            });
            textures.push(GltfTexture { source: img_index });
            mat.pbr_metallic_roughness.base_color_texture = Some(TextureInfo {
                index: textures.len() - 1,
            });
            mat.pbr_metallic_roughness.base_color_factor = Some([1.0, 1.0, 1.0, 1.0]);
        }
        materials.push(mat);

        let pos_acc =
            push_f32_accessor3(&mut bin, &mut buffer_views, &mut accessors, &mesh.positions, true);
        let nrm_acc =
            push_f32_accessor3(&mut bin, &mut buffer_views, &mut accessors, &mesh.normals, false);
        let uv_acc = push_f32_accessor2(&mut bin, &mut buffer_views, &mut accessors, &mesh.uvs);
        let idx_acc = push_u32_accessor(&mut bin, &mut buffer_views, &mut accessors, &mesh.indices);

        meshes_json.push(GltfMesh {
            name: Some(mesh.name.clone()),
            primitives: vec![Primitive {
                attributes: HashMap::from([
                    ("POSITION".into(), pos_acc),
                    ("NORMAL".into(), nrm_acc),
                    ("TEXCOORD_0".into(), uv_acc),
                ]),
                indices: idx_acc,
                material: mat_index,
            }],
        });

        nodes.push(GltfNode {
            name: Some(mesh.name.clone()),
            mesh: Some(mi),
        });
        node_indices.push(mi);
    }

    let pad_len = align4(bin.len());
    pad_to(&mut bin, pad_len);

    let gltf = GltfRoot {
        asset: Asset {
            version: "2.0".into(),
            generator: Some("openspace-mesh".into()),
        },
        buffers: vec![GltfBuffer {
            byte_length: bin.len(),
        }],
        buffer_views,
        accessors,
        meshes: meshes_json,
        materials,
        images,
        textures,
        nodes,
        scenes: vec![GltfScene {
            nodes: node_indices,
        }],
        scene: 0,
    };

    let json = serde_json::to_vec(&gltf).map_err(|e| OpenSpaceError::Internal(e.to_string()))?;
    let json_padded_len = align4(json.len());
    let mut json_chunk = json;
    while json_chunk.len() < json_padded_len {
        json_chunk.push(b' ');
    }

    let total_len = 12 + 8 + json_chunk.len() + 8 + bin.len();
    let mut out = Vec::with_capacity(total_len);
    out.extend_from_slice(&0x4654_6C67u32.to_le_bytes());
    out.extend_from_slice(&2u32.to_le_bytes());
    out.extend_from_slice(&(total_len as u32).to_le_bytes());
    out.extend_from_slice(&(json_chunk.len() as u32).to_le_bytes());
    out.extend_from_slice(&0x4E4F_534Au32.to_le_bytes());
    out.extend_from_slice(&json_chunk);
    out.extend_from_slice(&(bin.len() as u32).to_le_bytes());
    out.extend_from_slice(&0x004E_4942u32.to_le_bytes());
    out.extend_from_slice(&bin);
    Ok(out)
}

fn align4(n: usize) -> usize {
    (n + 3) & !3
}

fn pad_to(buf: &mut Vec<u8>, len: usize) {
    while buf.len() < len {
        buf.push(0);
    }
}

fn push_f32_vec(
    bin: &mut Vec<u8>,
    views: &mut Vec<BufferView>,
    accessors: &mut Vec<Accessor>,
    flat: &[f32],
    comps: usize,
    mins: Option<Vec<f32>>,
    maxs: Option<Vec<f32>>,
) -> usize {
    let offset = align4(bin.len());
    pad_to(bin, offset);
    let start = bin.len();
    for v in flat {
        bin.extend_from_slice(&v.to_le_bytes());
    }
    let byte_length = bin.len() - start;
    let view_index = views.len();
    views.push(BufferView {
        buffer: 0,
        byte_offset: start,
        byte_length,
        target: Some(34962),
    });
    let count = flat.len() / comps;
    let type_str = match comps {
        2 => "VEC2",
        3 => "VEC3",
        4 => "VEC4",
        _ => "SCALAR",
    };
    let acc_index = accessors.len();
    accessors.push(Accessor {
        buffer_view: view_index,
        component_type: 5126,
        count,
        type_: type_str.into(),
        max: maxs,
        min: mins,
    });
    acc_index
}

fn push_f32_accessor3(
    bin: &mut Vec<u8>,
    views: &mut Vec<BufferView>,
    accessors: &mut Vec<Accessor>,
    data: &[[f32; 3]],
    with_bounds: bool,
) -> usize {
    let flat: Vec<f32> = data.iter().flat_map(|v| v.iter().copied()).collect();
    let (mins, maxs) = if with_bounds && !data.is_empty() {
        let mut mn = data[0];
        let mut mx = data[0];
        for p in data {
            for i in 0..3 {
                mn[i] = mn[i].min(p[i]);
                mx[i] = mx[i].max(p[i]);
            }
        }
        (Some(mn.to_vec()), Some(mx.to_vec()))
    } else {
        (None, None)
    };
    push_f32_vec(bin, views, accessors, &flat, 3, mins, maxs)
}

fn push_f32_accessor2(
    bin: &mut Vec<u8>,
    views: &mut Vec<BufferView>,
    accessors: &mut Vec<Accessor>,
    data: &[[f32; 2]],
) -> usize {
    let flat: Vec<f32> = data.iter().flat_map(|v| v.iter().copied()).collect();
    push_f32_vec(bin, views, accessors, &flat, 2, None, None)
}

fn push_u32_accessor(
    bin: &mut Vec<u8>,
    views: &mut Vec<BufferView>,
    accessors: &mut Vec<Accessor>,
    data: &[u32],
) -> usize {
    let offset = align4(bin.len());
    pad_to(bin, offset);
    let start = bin.len();
    for v in data {
        bin.extend_from_slice(&v.to_le_bytes());
    }
    let byte_length = bin.len() - start;
    let view_index = views.len();
    views.push(BufferView {
        buffer: 0,
        byte_offset: start,
        byte_length,
        target: Some(34963),
    });
    let acc_index = accessors.len();
    accessors.push(Accessor {
        buffer_view: view_index,
        component_type: 5125,
        count: data.len(),
        type_: "SCALAR".into(),
        max: None,
        min: None,
    });
    acc_index
}

#[derive(Serialize)]
struct GltfRoot {
    asset: Asset,
    buffers: Vec<GltfBuffer>,
    #[serde(rename = "bufferViews")]
    buffer_views: Vec<BufferView>,
    accessors: Vec<Accessor>,
    meshes: Vec<GltfMesh>,
    materials: Vec<GltfMaterial>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    images: Vec<GltfImage>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    textures: Vec<GltfTexture>,
    nodes: Vec<GltfNode>,
    scenes: Vec<GltfScene>,
    scene: usize,
}

#[derive(Serialize)]
struct Asset {
    version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    generator: Option<String>,
}

#[derive(Serialize)]
struct GltfBuffer {
    #[serde(rename = "byteLength")]
    byte_length: usize,
}

#[derive(Serialize)]
struct BufferView {
    buffer: usize,
    #[serde(rename = "byteOffset")]
    byte_offset: usize,
    #[serde(rename = "byteLength")]
    byte_length: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    target: Option<u32>,
}

#[derive(Serialize)]
struct Accessor {
    #[serde(rename = "bufferView")]
    buffer_view: usize,
    #[serde(rename = "componentType")]
    component_type: u32,
    count: usize,
    #[serde(rename = "type")]
    type_: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    max: Option<Vec<f32>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    min: Option<Vec<f32>>,
}

#[derive(Serialize)]
struct GltfMesh {
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    primitives: Vec<Primitive>,
}

#[derive(Serialize)]
struct Primitive {
    attributes: HashMap<String, usize>,
    indices: usize,
    material: usize,
}

#[derive(Serialize)]
struct GltfMaterial {
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    #[serde(rename = "pbrMetallicRoughness")]
    pbr_metallic_roughness: Pbr,
    #[serde(rename = "doubleSided")]
    double_sided: bool,
}

#[derive(Serialize)]
struct Pbr {
    #[serde(rename = "baseColorFactor", skip_serializing_if = "Option::is_none")]
    base_color_factor: Option<[f32; 4]>,
    #[serde(rename = "metallicFactor")]
    metallic_factor: f32,
    #[serde(rename = "roughnessFactor")]
    roughness_factor: f32,
    #[serde(rename = "baseColorTexture", skip_serializing_if = "Option::is_none")]
    base_color_texture: Option<TextureInfo>,
}

#[derive(Serialize)]
struct TextureInfo {
    index: usize,
}

#[derive(Serialize)]
struct GltfImage {
    #[serde(rename = "bufferView")]
    buffer_view: usize,
    #[serde(rename = "mimeType")]
    mime_type: String,
}

#[derive(Serialize)]
struct GltfTexture {
    source: usize,
}

#[derive(Serialize)]
struct GltfNode {
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    mesh: Option<usize>,
}

#[derive(Serialize)]
struct GltfScene {
    nodes: Vec<usize>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Vec2;
    use openspace_core::{FloorplanIR, WallSegment};

    fn sample_ir() -> FloorplanIR {
        let w1 = Uuid::new_v4();
        let w2 = Uuid::new_v4();
        let w3 = Uuid::new_v4();
        let w4 = Uuid::new_v4();
        let room_id = Uuid::new_v4();
        FloorplanIR {
            walls: vec![
                WallSegment {
                    id: w1,
                    a: Vec2::new(0.0, 0.0),
                    b: Vec2::new(5.0, 0.0),
                    thickness_m: 0.15,
                    height_m: 2.7,
                },
                WallSegment {
                    id: w2,
                    a: Vec2::new(5.0, 0.0),
                    b: Vec2::new(5.0, 4.0),
                    thickness_m: 0.15,
                    height_m: 2.7,
                },
                WallSegment {
                    id: w3,
                    a: Vec2::new(5.0, 4.0),
                    b: Vec2::new(0.0, 4.0),
                    thickness_m: 0.15,
                    height_m: 2.7,
                },
                WallSegment {
                    id: w4,
                    a: Vec2::new(0.0, 4.0),
                    b: Vec2::new(0.0, 0.0),
                    thickness_m: 0.15,
                    height_m: 2.7,
                },
            ],
            openings: vec![],
            rooms: vec![Room {
                id: room_id,
                name: "Living".into(),
                polygon: vec![
                    Vec2::new(0.0, 0.0),
                    Vec2::new(5.0, 0.0),
                    Vec2::new(5.0, 4.0),
                    Vec2::new(0.0, 4.0),
                ],
                wall_ids: vec![w1, w2, w3, w4],
            }],
            scale_m_per_px: 0.01,
            default_wall_height_m: 2.7,
            default_wall_thickness_m: 0.15,
        }
    }

    #[test]
    fn extrude_and_export_glb() {
        let ir = sample_ir();
        let scene = extrude(&ir).unwrap();
        assert!(scene.meshes.len() >= 5); // 4 walls + floor (open-top MVP)
        assert!(!scene.meshes.iter().any(|m| m.kind == MeshKind::Ceiling));
        assert!(scene.meshes.iter().any(|m| m.name.contains("Living")));
        let glb = export_glb(&scene).unwrap();
        assert!(glb.len() > 100);
        assert_eq!(&glb[0..4], b"glTF");
    }

    #[test]
    fn empty_ir_fails() {
        assert!(extrude(&FloorplanIR::default()).is_err());
    }
}
