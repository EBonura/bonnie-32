//! Asset Definition
//!
//! A complete game asset - pure composition of components.
//! Everything is a component (including mesh), enabling uniform handling
//! and mesh-less assets (pure triggers, lights, spawn points).

use std::sync::atomic::{AtomicU64, Ordering};
#[cfg(not(target_arch = "wasm32"))]
use std::path::Path;
use serde::{Deserialize, Serialize};
use crate::modeler::{MeshPart, MeshProject, RigBone};
use crate::rasterizer::Vec3;
use super::component::AssetComponent;
use super::library::AssetSource;

/// Counter for generating unique asset IDs
static ASSET_ID_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Generate a stable unique ID for an asset
///
/// Uses a combination of atomic counter, random value, and timestamp to ensure
/// uniqueness both within a session and across separate launches.
pub fn generate_asset_id() -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let counter = ASSET_ID_COUNTER.fetch_add(1, Ordering::SeqCst);

    // Use macroquad's rand which works in WASM (avoids SystemTime::now() which panics in WASM)
    let random_bits = macroquad::rand::rand() as u64;

    let mut hasher = DefaultHasher::new();
    counter.hash(&mut hasher);
    random_bits.hash(&mut hasher);

    // Include timestamp for cross-session uniqueness (counter and rand may
    // repeat across launches since the counter resets and rand seed may match)
    #[cfg(not(target_arch = "wasm32"))]
    {
        if let Ok(time) = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
            time.as_nanos().hash(&mut hasher);
        }
    }

    hasher.finish()
}

/// Error type for asset operations
#[derive(Debug)]
pub enum AssetError {
    /// File I/O error
    Io(String),
    /// Serialization/deserialization error
    Serialization(String),
    /// Validation error
    ValidationError(String),
}

impl std::fmt::Display for AssetError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AssetError::Io(msg) => write!(f, "I/O error: {}", msg),
            AssetError::Serialization(msg) => write!(f, "Serialization error: {}", msg),
            AssetError::ValidationError(msg) => write!(f, "Validation error: {}", msg),
        }
    }
}

impl std::error::Error for AssetError {}

impl From<std::io::Error> for AssetError {
    fn from(e: std::io::Error) -> Self {
        AssetError::Io(e.to_string())
    }
}

/// A complete game asset - pure composition of components
///
/// Assets are self-contained bundles of:
/// - Mesh geometry (embedded, not file references)
/// - Texture references (ID-based, pointing to shared texture library)
/// - Component definitions (collision, enemy, light, etc.)
/// - Metadata (category, tags, description)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Asset {
    /// Stable unique identifier
    ///
    /// This ID survives edits and renames. Used for references from
    /// levels and other assets.
    #[serde(default = "generate_asset_id")]
    pub id: u64,

    /// Human-readable name (also used as filename)
    pub name: String,

    /// All components attached to this asset
    ///
    /// Mesh is just another component - assets can have zero or more meshes,
    /// or no mesh at all (for pure trigger zones, lights, etc.)
    pub components: Vec<AssetComponent>,

    /// Category for organization (e.g., "enemies", "pickups", "props")
    #[serde(default)]
    pub category: String,

    /// Optional description
    #[serde(default)]
    pub description: String,

    /// Tags for filtering in browser
    #[serde(default)]
    pub tags: Vec<String>,

    /// Whether this is a built-in asset (player_spawn, point_light, checkpoint)
    ///
    /// Built-in assets cannot be deleted and are shown with a special icon in the browser.
    #[serde(default)]
    pub is_builtin: bool,

    /// Source/origin of this asset (set at load time, not persisted)
    ///
    /// Determines where the asset came from and whether it's editable:
    /// - Sample: read-only bundled asset from assets/samples/assets/
    /// - User: editable user-created asset from assets/userdata/assets/
    #[serde(skip)]
    pub source: AssetSource,
}

impl Asset {
    /// Create a new asset with a default cube mesh
    pub fn new(name: impl Into<String>) -> Self {
        let name = name.into();
        Self {
            id: generate_asset_id(),
            name: name.clone(),
            components: vec![AssetComponent::Mesh {
                parts: vec![MeshPart::cube(format!("{}_cube", name), 1024.0)],
            }],
            category: String::new(),
            description: String::new(),
            tags: Vec::new(),
            is_builtin: false,
            source: AssetSource::User,
        }
    }

    /// Create an empty asset with no components
    pub fn empty(name: impl Into<String>) -> Self {
        Self {
            id: generate_asset_id(),
            name: name.into(),
            components: Vec::new(),
            category: String::new(),
            description: String::new(),
            tags: Vec::new(),
            is_builtin: false,
            source: AssetSource::User,
        }
    }

    /// Create an asset from an existing MeshProject (for migration)
    pub fn from_mesh_project(name: &str, project: &MeshProject) -> Self {
        Self {
            id: generate_asset_id(),
            name: name.to_string(),
            components: vec![AssetComponent::Mesh {
                parts: project.objects.clone(),
            }],
            category: String::new(),
            description: String::new(),
            tags: Vec::new(),
            is_builtin: false,
            source: AssetSource::User,
        }
    }

    /// Convert this asset to a MeshProject for editing in the modeler
    ///
    /// This extracts the Mesh component's parts and creates a MeshProject.
    /// Note: Component metadata (collision, enemy, etc.) is not preserved
    /// in the MeshProject - use Asset directly when that data is needed.
    pub fn to_mesh_project(&self) -> MeshProject {
        let mut project = MeshProject::new(&self.name);
        project.objects = self.mesh().cloned().unwrap_or_default();
        project
    }

    /// Get the Mesh component if present
    ///
    /// Returns the first Mesh component found. Assets can theoretically
    /// have multiple Mesh components, but this returns only the first.
    pub fn mesh(&self) -> Option<&Vec<MeshPart>> {
        self.components.iter().find_map(|c| match c {
            AssetComponent::Mesh { parts } => Some(parts),
            _ => None,
        })
    }

    /// Get mutable reference to the Mesh component
    pub fn mesh_mut(&mut self) -> Option<&mut Vec<MeshPart>> {
        self.components.iter_mut().find_map(|c| match c {
            AssetComponent::Mesh { parts } => Some(parts),
            _ => None,
        })
    }

    /// Get reference to the first Skeleton component's bones
    pub fn skeleton(&self) -> Option<&Vec<RigBone>> {
        self.components.iter().find_map(|c| match c {
            AssetComponent::Skeleton { bones } => Some(bones),
            _ => None,
        })
    }

    /// Get mutable reference to the first Skeleton component's bones
    pub fn skeleton_mut(&mut self) -> Option<&mut Vec<RigBone>> {
        self.components.iter_mut().find_map(|c| match c {
            AssetComponent::Skeleton { bones } => Some(bones),
            _ => None,
        })
    }

    /// Add a component to this asset
    pub fn add_component(&mut self, component: AssetComponent) {
        self.components.push(component);
    }

    /// Remove a component by index
    pub fn remove_component(&mut self, index: usize) -> Option<AssetComponent> {
        if index < self.components.len() {
            Some(self.components.remove(index))
        } else {
            None
        }
    }

    /// Check if this asset has a Mesh component
    pub fn has_mesh(&self) -> bool {
        self.components.iter().any(|c| c.is_mesh())
    }

    /// Check if this asset has a Collision component
    pub fn has_collision(&self) -> bool {
        self.components.iter().any(|c| c.is_collision())
    }

    /// Check if this asset has a Light component
    pub fn has_light(&self) -> bool {
        self.components.iter().any(|c| c.is_light())
    }

    /// Check if this asset has an Enemy component
    pub fn has_enemy(&self) -> bool {
        self.components.iter().any(|c| c.is_enemy())
    }

    /// Check if this asset has a Trigger component
    pub fn has_trigger(&self) -> bool {
        self.components
            .iter()
            .any(|c| matches!(c, AssetComponent::Trigger { .. }))
    }

    /// Check if this asset has a Pickup component
    pub fn has_pickup(&self) -> bool {
        self.components
            .iter()
            .any(|c| matches!(c, AssetComponent::Pickup { .. }))
    }

    /// Check if this asset has a Door component
    pub fn has_door(&self) -> bool {
        self.components
            .iter()
            .any(|c| matches!(c, AssetComponent::Door { .. }))
    }

    /// Check if this asset has a SpawnPoint component with the given player flag
    pub fn has_spawn_point(&self, is_player: bool) -> bool {
        self.components.iter().any(|c| {
            matches!(c, AssetComponent::SpawnPoint { is_player: p, .. } if *p == is_player)
        })
    }

    /// Compute axis-aligned bounding box from mesh (if present)
    ///
    /// Returns (min, max) corners of the bounding box, or None if no mesh.
    pub fn bounds(&self) -> Option<(Vec3, Vec3)> {
        let objects = self.mesh()?;

        let mut min = Vec3::new(f32::MAX, f32::MAX, f32::MAX);
        let mut max = Vec3::new(f32::MIN, f32::MIN, f32::MIN);
        let mut has_verts = false;

        for obj in objects {
            for v in &obj.mesh.vertices {
                has_verts = true;
                min.x = min.x.min(v.pos.x);
                min.y = min.y.min(v.pos.y);
                min.z = min.z.min(v.pos.z);
                max.x = max.x.max(v.pos.x);
                max.y = max.y.max(v.pos.y);
                max.z = max.z.max(v.pos.z);
            }
        }

        if has_verts {
            Some((min, max))
        } else {
            None
        }
    }

    /// Get total vertex count across all mesh objects
    pub fn total_vertices(&self) -> usize {
        self.mesh()
            .map(|objs| objs.iter().map(|o| o.mesh.vertex_count()).sum())
            .unwrap_or(0)
    }

    /// Get total face count across all mesh objects
    pub fn total_faces(&self) -> usize {
        self.mesh()
            .map(|objs| objs.iter().map(|o| o.mesh.face_count()).sum())
            .unwrap_or(0)
    }

    /// Save asset to file (compressed RON format with brotli)
    #[cfg(not(target_arch = "wasm32"))]
    pub fn save(&self, path: &Path) -> Result<(), AssetError> {
        use std::io::Cursor;

        let config = ron::ser::PrettyConfig::new()
            .depth_limit(4)
            .indentor("  ".to_string());
        let ron_data = ron::ser::to_string_pretty(self, config)
            .map_err(|e| AssetError::Serialization(e.to_string()))?;

        // Compress with brotli
        let mut compressed = Vec::new();
        brotli::BrotliCompress(
            &mut Cursor::new(ron_data.as_bytes()),
            &mut compressed,
            &brotli::enc::BrotliEncoderParams {
                quality: 6,
                lgwin: 22,
                ..Default::default()
            },
        )
        .map_err(|e| AssetError::Io(format!("compression failed: {}", e)))?;

        std::fs::write(path, compressed)?;
        Ok(())
    }

    /// Load asset from file (supports both compressed and uncompressed RON)
    #[cfg(not(target_arch = "wasm32"))]
    pub fn load(path: &Path) -> Result<Self, AssetError> {
        let data = std::fs::read(path)?;
        Self::load_from_bytes(&data)
    }

    /// Load asset from bytes (supports both compressed and uncompressed RON)
    pub fn load_from_bytes(data: &[u8]) -> Result<Self, AssetError> {
        // Try to detect if compressed or plain text
        // RON files start with '(' or whitespace before '('
        let is_ron = data.first().map(|&b| b == b'(' || b == b' ' || b == b'\n' || b == b'\r').unwrap_or(false);

        let ron_str = if is_ron {
            String::from_utf8_lossy(data).to_string()
        } else {
            // Decompress brotli
            let mut decompressed = Vec::new();
            brotli::BrotliDecompress(&mut std::io::Cursor::new(data), &mut decompressed)
                .map_err(|e| AssetError::Io(format!("decompression failed: {}", e)))?;
            String::from_utf8_lossy(&decompressed).to_string()
        };

        let mut asset: Self = ron::from_str(&ron_str).map_err(|e| AssetError::Serialization(e.to_string()))?;
        // Resolve texture refs to populate atlas fields
        asset.resolve_texture_refs();
        Ok(asset)
    }

    /// Resolve texture references and populate atlas fields
    ///
    /// After deserialization, the `atlas` field on MeshParts is empty (skip_serializing).
    /// This method populates it based on the `texture_ref` field:
    /// - Checkerboard → uses procedural checkerboard atlas
    /// - Embedded → copies from the embedded atlas data
    /// - Id → requires external texture library (not handled here)
    /// - None → uses default checkerboard as placeholder
    pub fn resolve_texture_refs(&mut self) {
        use crate::modeler::{TextureRef, IndexedAtlas, checkerboard_atlas};
        use crate::rasterizer::ClutDepth;

        if let Some(parts) = self.mesh_mut() {
            for part in parts.iter_mut() {
                match &part.texture_ref {
                    TextureRef::Checkerboard | TextureRef::None => {
                        // Use procedural checkerboard
                        part.atlas = checkerboard_atlas().clone();
                    }
                    TextureRef::Embedded(embedded_atlas) => {
                        // Copy from embedded data
                        part.atlas = embedded_atlas.as_ref().clone();
                    }
                    TextureRef::Id(_) => {
                        // ID-based refs need the texture library - set to checkerboard as placeholder
                        // The caller (ModelerState) should call resolve_all_texture_refs to properly resolve these
                        if part.atlas.width == 0 || part.atlas.indices.is_empty() {
                            part.atlas = IndexedAtlas::new_checkerboard(128, 128, ClutDepth::Bpp4);
                        }
                    }
                }
            }
        }
    }

    /// Serialize to bytes (compressed RON)
    pub fn to_bytes(&self) -> Result<Vec<u8>, AssetError> {
        let config = ron::ser::PrettyConfig::new()
            .depth_limit(4)
            .indentor("  ".to_string());
        let ron_data = ron::ser::to_string_pretty(self, config)
            .map_err(|e| AssetError::Serialization(e.to_string()))?;

        let mut compressed = Vec::new();
        brotli::BrotliCompress(
            &mut std::io::Cursor::new(ron_data.as_bytes()),
            &mut compressed,
            &brotli::enc::BrotliEncoderParams {
                quality: 6,
                lgwin: 22,
                ..Default::default()
            },
        )
        .map_err(|e| AssetError::Io(format!("compression failed: {}", e)))?;

        Ok(compressed)
    }
}

impl Default for Asset {
    fn default() -> Self {
        Self::new("untitled")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_asset_creation() {
        let asset = Asset::new("test_asset");
        assert_eq!(asset.name, "test_asset");
        assert!(asset.has_mesh());
        assert!(!asset.has_collision());
        assert!(!asset.has_enemy());
    }

    #[test]
    fn test_empty_asset() {
        let asset = Asset::empty("empty_asset");
        assert!(!asset.has_mesh());
        assert_eq!(asset.components.len(), 0);
    }

    #[test]
    fn test_add_remove_component() {
        let mut asset = Asset::empty("test");

        asset.add_component(AssetComponent::Light {
            color: [255, 200, 100],
            intensity: 1.0,
            radius: 500.0,
            offset: [0.0, 100.0, 0.0],
        });

        assert!(asset.has_light());
        assert_eq!(asset.components.len(), 1);

        asset.remove_component(0);
        assert!(!asset.has_light());
        assert_eq!(asset.components.len(), 0);
    }

    #[test]
    fn test_unique_ids() {
        let asset1 = Asset::new("asset1");
        let asset2 = Asset::new("asset2");
        assert_ne!(asset1.id, asset2.id);
    }
}
