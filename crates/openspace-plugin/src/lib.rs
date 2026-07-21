//! Reconstruction backend plugin interface.

use async_trait::async_trait;
use image::DynamicImage;
use openspace_core::{FloorplanIR, OpenSpaceError, PhotoRef, Result};
use openspace_mesh::{export_glb, extrude, MeshScene};
use openspace_texture::apply_photo_textures;
use openspace_vision::detect_floorplan;
use std::path::Path;
use std::sync::Arc;

#[async_trait]
pub trait ReconstructionBackend: Send + Sync {
    fn name(&self) -> &str;

    async fn detect(&self, img: &DynamicImage) -> Result<FloorplanIR>;

    async fn mesh(&self, ir: &FloorplanIR) -> Result<MeshScene>;

    async fn texture(
        &self,
        scene: MeshScene,
        photos: &[PhotoRef],
        data_root: &Path,
    ) -> Result<MeshScene>;

    async fn build_glb(
        &self,
        ir: &FloorplanIR,
        photos: &[PhotoRef],
        data_root: &Path,
    ) -> Result<Vec<u8>> {
        let scene = self.mesh(ir).await?;
        let scene = self.texture(scene, photos, data_root).await?;
        export_glb(&scene)
    }
}

/// Classical geometric reconstruction (MVP default).
pub struct GeometricBackend;

#[async_trait]
impl ReconstructionBackend for GeometricBackend {
    fn name(&self) -> &str {
        "geometric"
    }

    async fn detect(&self, img: &DynamicImage) -> Result<FloorplanIR> {
        detect_floorplan(img)
    }

    async fn mesh(&self, ir: &FloorplanIR) -> Result<MeshScene> {
        extrude(ir)
    }

    async fn texture(
        &self,
        mut scene: MeshScene,
        photos: &[PhotoRef],
        data_root: &Path,
    ) -> Result<MeshScene> {
        apply_photo_textures(&mut scene, photos, data_root)?;
        Ok(scene)
    }
}

/// Placeholder for future ONNX-backed reconstruction. No Python; not implemented yet.
pub struct OnnxBackend;

#[async_trait]
impl ReconstructionBackend for OnnxBackend {
    fn name(&self) -> &str {
        "onnx"
    }

    async fn detect(&self, _img: &DynamicImage) -> Result<FloorplanIR> {
        Err(OpenSpaceError::Unsupported(
            "OnnxBackend::detect is not implemented yet; use GeometricBackend".into(),
        ))
    }

    async fn mesh(&self, _ir: &FloorplanIR) -> Result<MeshScene> {
        Err(OpenSpaceError::Unsupported(
            "OnnxBackend::mesh is not implemented yet".into(),
        ))
    }

    async fn texture(
        &self,
        _scene: MeshScene,
        _photos: &[PhotoRef],
        _data_root: &Path,
    ) -> Result<MeshScene> {
        Err(OpenSpaceError::Unsupported(
            "OnnxBackend::texture is not implemented yet".into(),
        ))
    }
}

pub fn default_backend() -> Arc<dyn ReconstructionBackend> {
    Arc::new(GeometricBackend)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn onnx_returns_unsupported() {
        let b = OnnxBackend;
        let img = DynamicImage::new_luma8(32, 32);
        let err = b.detect(&img).await.unwrap_err();
        assert!(matches!(err, OpenSpaceError::Unsupported(_)));
    }

    #[tokio::test]
    async fn geometric_detects_synthetic() {
        let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/floorplans");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("plugin_synth.png");
        openspace_vision::write_synthetic_floorplan(&path, 400, 300).unwrap();
        let img = image::open(&path).unwrap();
        let b = GeometricBackend;
        let ir = b.detect(&img).await.unwrap();
        assert!(!ir.walls.is_empty());
    }
}
