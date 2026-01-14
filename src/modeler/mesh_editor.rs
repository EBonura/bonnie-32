//! Mesh editor for importing OBJ files and assigning faces to bones
//! PS1-style skeletal animation with binary bone weights
//!
//! Also includes PicoCAD-style mesh organization with named objects and texture atlas.
//!
//! Supports both compressed (brotli) and uncompressed RON files.
//! - Reading: Auto-detects format by checking for valid RON start
//! - Writing: Always uses brotli compression

use crate::rasterizer::{Vec3, Vec2, Vertex, Color as RasterColor, Color15, Texture15, BlendMode, ClutDepth, ClutId, Clut, IndexedTexture};
use serde::{Deserialize, Serialize};
use std::path::Path;
use super::state::MirrorSettings;
#[cfg(not(target_arch = "wasm32"))]
use std::io::Cursor;

// ============================================================================
// N-Gon Face Support (Blender-style)
// ============================================================================

/// N-gon face for editing (supports 3+ vertices)
///
/// Unlike the rasterizer's `Face` which is triangle-only, EditFace supports
/// arbitrary polygon sizes. Triangulation happens in `to_render_data()`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EditFace {
    /// Vertex indices (3 for triangle, 4 for quad, n for n-gon)
    pub vertices: Vec<usize>,
    /// Optional texture ID
    pub texture_id: Option<usize>,
    /// If true, pure black pixels are treated as transparent
    #[serde(default = "default_black_transparent")]
    pub black_transparent: bool,
    /// PS1 blend mode for this face
    #[serde(default)]
    pub blend_mode: BlendMode,
}

fn default_black_transparent() -> bool {
    true
}

impl EditFace {
    /// Create a triangle face
    pub fn tri(v0: usize, v1: usize, v2: usize) -> Self {
        Self {
            vertices: vec![v0, v1, v2],
            texture_id: None,
            black_transparent: true,
            blend_mode: BlendMode::Opaque,
        }
    }

    /// Create a quad face
    pub fn quad(v0: usize, v1: usize, v2: usize, v3: usize) -> Self {
        Self {
            vertices: vec![v0, v1, v2, v3],
            texture_id: None,
            black_transparent: true,
            blend_mode: BlendMode::Opaque,
        }
    }

    /// Create an n-gon face from a slice of vertex indices
    pub fn ngon(vertices: &[usize]) -> Self {
        Self {
            vertices: vertices.to_vec(),
            texture_id: None,
            black_transparent: true,
            blend_mode: BlendMode::Opaque,
        }
    }

    /// Number of vertices in this face
    pub fn vertex_count(&self) -> usize {
        self.vertices.len()
    }

    /// Check if this is a triangle
    pub fn is_tri(&self) -> bool {
        self.vertices.len() == 3
    }

    /// Check if this is a quad
    pub fn is_quad(&self) -> bool {
        self.vertices.len() == 4
    }

    /// Get edges as pairs of vertex indices (in winding order)
    pub fn edges(&self) -> impl Iterator<Item = (usize, usize)> + '_ {
        let n = self.vertices.len();
        (0..n).map(move |i| (self.vertices[i], self.vertices[(i + 1) % n]))
    }

    /// Fan triangulation: split n-gon into triangles from first vertex
    /// Returns vertex index triplets for each triangle
    pub fn triangulate(&self) -> Vec<[usize; 3]> {
        let n = self.vertices.len();
        if n < 3 {
            return vec![];
        }
        if n == 3 {
            return vec![[self.vertices[0], self.vertices[1], self.vertices[2]]];
        }

        // Fan from vertex 0: creates (n-2) triangles
        (1..n - 1)
            .map(|i| [self.vertices[0], self.vertices[i], self.vertices[i + 1]])
            .collect()
    }

    /// Set texture ID (builder pattern)
    pub fn with_texture(mut self, texture_id: usize) -> Self {
        self.texture_id = Some(texture_id);
        self
    }

    /// Set black_transparent flag (builder pattern)
    pub fn with_black_transparent(mut self, black_transparent: bool) -> Self {
        self.black_transparent = black_transparent;
        self
    }

    /// Set blend mode (builder pattern)
    pub fn with_blend_mode(mut self, blend_mode: BlendMode) -> Self {
        self.blend_mode = blend_mode;
        self
    }
}

// ============================================================================
// PicoCAD-style Mesh Organization
// ============================================================================

/// A named mesh object (like picoCAD's Overview panel items)
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MeshObject {
    /// Display name (e.g., "hull", "wing 1", "cockpit")
    pub name: String,
    /// The geometry
    pub mesh: EditableMesh,
    /// Per-object texture atlas (stores palette indices)
    pub atlas: IndexedAtlas,
    /// Whether this object is visible in the viewport
    pub visible: bool,
    /// Whether this object is locked (can't be selected/edited)
    pub locked: bool,
    /// Color tint for identification in viewport (optional)
    pub color: Option<[u8; 3]>,
    /// If true, backface culling is disabled (both sides render)
    #[serde(default)]
    pub double_sided: bool,
    /// Per-object mirror settings (replaces global mirror)
    #[serde(default)]
    pub mirror: Option<MirrorSettings>,
}

impl MeshObject {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            mesh: EditableMesh::new(),
            atlas: IndexedAtlas::new_checkerboard(128, 128, ClutDepth::Bpp4),
            visible: true,
            locked: false,
            color: None,
            double_sided: false,
            mirror: None,
        }
    }

    pub fn with_mesh(name: impl Into<String>, mesh: EditableMesh) -> Self {
        Self {
            name: name.into(),
            mesh,
            atlas: IndexedAtlas::new_checkerboard(128, 128, ClutDepth::Bpp4),
            visible: true,
            locked: false,
            color: None,
            double_sided: false,
            mirror: None,
        }
    }

    pub fn with_mesh_and_atlas(name: impl Into<String>, mesh: EditableMesh, atlas: IndexedAtlas) -> Self {
        Self {
            name: name.into(),
            mesh,
            atlas,
            visible: true,
            locked: false,
            color: None,
            double_sided: false,
            mirror: None,
        }
    }

    pub fn cube(name: impl Into<String>, size: f32) -> Self {
        Self::with_mesh(name, EditableMesh::cube(size))
    }
}

/// A complete PicoCAD-style project with multiple objects and indexed texture atlas
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MeshProject {
    /// Project name
    pub name: String,
    /// All mesh objects in the project
    pub objects: Vec<MeshObject>,
    /// The indexed texture atlas (stores palette indices)
    pub atlas: IndexedAtlas,

    /// Global CLUT pool (shared across all textures)
    #[serde(default)]
    pub clut_pool: ClutPool,

    /// Preview CLUT override (for testing palette swaps without changing default)
    #[serde(skip)]
    pub preview_clut: Option<ClutId>,

    /// Currently selected object index
    #[serde(skip)]
    pub selected_object: Option<usize>,
}

impl MeshProject {
    pub fn new(name: impl Into<String>) -> Self {
        // Create pool first so we can link its first CLUT to the atlas
        let clut_pool = ClutPool::default();
        let first_clut_id = clut_pool.first_id().unwrap_or(ClutId::NONE);

        // Create atlas with default CLUT linked to pool's first CLUT
        let mut atlas = IndexedAtlas::new(128, 128, ClutDepth::Bpp4);
        atlas.default_clut = first_clut_id;

        Self {
            name: name.into(),
            // Default cube: 1024 units = 1 meter (SECTOR_SIZE)
            objects: vec![MeshObject::cube("Cube.00", 1024.0)],
            atlas,
            clut_pool,
            preview_clut: None,
            selected_object: Some(0),
        }
    }

    /// Add a new object and return its index
    pub fn add_object(&mut self, obj: MeshObject) -> usize {
        let idx = self.objects.len();
        self.objects.push(obj);
        idx
    }

    /// Get the currently selected object
    pub fn selected(&self) -> Option<&MeshObject> {
        self.selected_object.and_then(|i| self.objects.get(i))
    }

    /// Get the currently selected object mutably
    pub fn selected_mut(&mut self) -> Option<&mut MeshObject> {
        self.selected_object.and_then(|i| self.objects.get_mut(i))
    }

    /// Get total vertex count across all objects
    pub fn total_vertices(&self) -> usize {
        self.objects.iter().map(|o| o.mesh.vertex_count()).sum()
    }

    /// Get total face count across all objects
    pub fn total_faces(&self) -> usize {
        self.objects.iter().map(|o| o.mesh.face_count()).sum()
    }

    /// Get the effective CLUT for the atlas (preview_clut > default_clut > first in pool)
    pub fn effective_clut(&self) -> Option<&Clut> {
        // Try preview override first
        if let Some(preview_id) = self.preview_clut {
            if let Some(clut) = self.clut_pool.get(preview_id) {
                return Some(clut);
            }
        }
        // Try atlas default
        if self.atlas.default_clut.is_valid() {
            if let Some(clut) = self.clut_pool.get(self.atlas.default_clut) {
                return Some(clut);
            }
        }
        // Fall back to first CLUT in pool
        self.clut_pool.first_id().and_then(|id| self.clut_pool.get(id))
    }

    /// Save project to file (compressed RON format with brotli)
    #[cfg(not(target_arch = "wasm32"))]
    pub fn save_to_file(&self, path: &Path) -> Result<(), MeshEditorError> {
        let config = ron::ser::PrettyConfig::new()
            .depth_limit(4)
            .indentor("  ".to_string());
        let ron_data = ron::ser::to_string_pretty(self, config)
            .map_err(|e| MeshEditorError::Serialization(e.to_string()))?;

        // Compress with brotli
        let mut compressed = Vec::new();
        brotli::BrotliCompress(&mut Cursor::new(ron_data.as_bytes()), &mut compressed, &brotli::enc::BrotliEncoderParams {
            quality: 6,
            lgwin: 22,
            ..Default::default()
        }).map_err(|e| MeshEditorError::Io(format!("compression failed: {}", e)))?;

        std::fs::write(path, compressed)
            .map_err(|e| MeshEditorError::Io(e.to_string()))?;
        Ok(())
    }

    /// Load project from file (supports both compressed and uncompressed RON)
    #[cfg(not(target_arch = "wasm32"))]
    pub fn load_from_file(path: &Path) -> Result<Self, MeshEditorError> {
        let bytes = std::fs::read(path)
            .map_err(|e| MeshEditorError::Io(e.to_string()))?;

        // Detect format: RON files start with '(' or whitespace, brotli is binary
        let is_plain_ron = bytes.first().map(|&b| b == b'(' || b == b' ' || b == b'\n' || b == b'\r' || b == b'\t').unwrap_or(false);

        let ron_data = if is_plain_ron {
            String::from_utf8(bytes)
                .map_err(|e| MeshEditorError::Io(format!("invalid UTF-8: {}", e)))?
        } else {
            let mut decompressed = Vec::new();
            brotli::BrotliDecompress(&mut Cursor::new(&bytes), &mut decompressed)
                .map_err(|e| MeshEditorError::Io(format!("decompression failed: {}", e)))?;
            String::from_utf8(decompressed)
                .map_err(|e| MeshEditorError::Io(format!("invalid UTF-8: {}", e)))?
        };

        Self::load_from_str(&ron_data)
    }

    /// Load project from string (.ron format) - works on all platforms including WASM
    pub fn load_from_str(ron_data: &str) -> Result<Self, MeshEditorError> {
        let mut project: MeshProject = ron::from_str(ron_data)
            .map_err(|e| MeshEditorError::Serialization(e.to_string()))?;

        // Select first object by default after loading
        if !project.objects.is_empty() {
            project.selected_object = Some(0);
        }

        Ok(project)
    }
}

impl Default for MeshProject {
    fn default() -> Self {
        Self::new("Untitled")
    }
}

// ============================================================================
// CLUT Pool (Global palette storage like PS1 VRAM)
// ============================================================================

/// Global CLUT pool - shared across all textures in a project
///
/// Mimics PS1 VRAM where CLUTs are stored as 16x1 or 256x1 pixel strips.
/// Multiple textures can reference the same CLUT for palette swapping.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClutPool {
    /// All CLUTs in the pool
    cluts: Vec<Clut>,
    /// Next ID to assign (starts at 1, 0 is reserved for NONE)
    next_id: u32,
}

impl ClutPool {
    /// Create a new pool with a default grayscale CLUT
    pub fn new() -> Self {
        let mut pool = Self {
            cluts: Vec::new(),
            next_id: 1,
        };
        // Add a default 4-bit grayscale CLUT
        pool.add_clut(Clut::new_4bit("Default"));
        pool
    }

    /// Add a CLUT to the pool and return its assigned ID
    pub fn add_clut(&mut self, mut clut: Clut) -> ClutId {
        let id = ClutId(self.next_id);
        self.next_id += 1;
        clut.id = id;
        self.cluts.push(clut);
        id
    }

    /// Get CLUT by ID
    pub fn get(&self, id: ClutId) -> Option<&Clut> {
        self.cluts.iter().find(|c| c.id == id)
    }

    /// Get mutable CLUT by ID
    pub fn get_mut(&mut self, id: ClutId) -> Option<&mut Clut> {
        self.cluts.iter_mut().find(|c| c.id == id)
    }

    /// Remove CLUT by ID, returns the removed CLUT
    pub fn remove(&mut self, id: ClutId) -> Option<Clut> {
        if let Some(pos) = self.cluts.iter().position(|c| c.id == id) {
            Some(self.cluts.remove(pos))
        } else {
            None
        }
    }

    /// Get the first CLUT ID (useful as default)
    pub fn first_id(&self) -> Option<ClutId> {
        self.cluts.first().map(|c| c.id)
    }

    /// Iterate over all CLUTs
    pub fn iter(&self) -> impl Iterator<Item = &Clut> {
        self.cluts.iter()
    }

    /// Iterate over all CLUTs mutably
    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut Clut> {
        self.cluts.iter_mut()
    }

    /// Number of CLUTs in the pool
    pub fn len(&self) -> usize {
        self.cluts.len()
    }

    /// Check if pool is empty
    pub fn is_empty(&self) -> bool {
        self.cluts.is_empty()
    }

    /// Get all CLUTs as a slice
    pub fn as_slice(&self) -> &[Clut] {
        &self.cluts
    }

    /// Clear all CLUTs from the pool (for import operations)
    pub fn clear(&mut self) {
        self.cluts.clear();
        self.next_id = 1;
    }
}

impl Default for ClutPool {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Indexed Atlas (PS1-style palette-indexed texture)
// ============================================================================

/// Indexed texture atlas storing palette indices instead of colors
///
/// PS1-authentic texture format where each pixel is a palette index (4-bit or 8-bit).
/// Colors are resolved at render time using a CLUT (Color Look-Up Table).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexedAtlas {
    pub width: usize,
    pub height: usize,
    /// CLUT depth (4-bit or 8-bit)
    pub depth: ClutDepth,
    /// Palette indices for each pixel (one byte per pixel)
    pub indices: Vec<u8>,
    /// Default CLUT ID for rendering this atlas
    pub default_clut: ClutId,
}

impl IndexedAtlas {
    /// Create a new indexed atlas filled with index 0 (transparent)
    pub fn new(width: usize, height: usize, depth: ClutDepth) -> Self {
        Self {
            width,
            height,
            depth,
            indices: vec![0; width * height],
            default_clut: ClutId::NONE,
        }
    }

    /// Create a new indexed atlas with a checkerboard pattern (visible by default)
    pub fn new_checkerboard(width: usize, height: usize, depth: ClutDepth) -> Self {
        let mut indices = vec![0u8; width * height];
        let cell_size = 8; // 8x8 checkerboard cells
        for y in 0..height {
            for x in 0..width {
                let checker = ((x / cell_size) + (y / cell_size)) % 2 == 0;
                // Use indices 7 and 15 for visible checkerboard (light/dark gray)
                indices[y * width + x] = if checker { 7 } else { 15 };
            }
        }
        Self {
            width,
            height,
            depth,
            indices,
            default_clut: ClutId::NONE,
        }
    }

    /// Get palette index at pixel coordinates
    pub fn get_index(&self, x: usize, y: usize) -> u8 {
        if x < self.width && y < self.height {
            self.indices.get(y * self.width + x).copied().unwrap_or(0)
        } else {
            0
        }
    }

    /// Set palette index at pixel coordinates
    pub fn set_index(&mut self, x: usize, y: usize, index: u8) {
        if x < self.width && y < self.height {
            let clamped = index.min(self.depth.max_index());
            if let Some(pixel) = self.indices.get_mut(y * self.width + x) {
                *pixel = clamped;
            }
        }
    }

    /// Convert to IndexedTexture for rendering
    pub fn to_indexed_texture(&self, name: &str) -> IndexedTexture {
        IndexedTexture {
            width: self.width,
            height: self.height,
            depth: self.depth,
            indices: self.indices.clone(),
            default_clut: self.default_clut,
            name: name.to_string(),
        }
    }

    /// Convert to Texture15 using a CLUT (for preview/backwards compat)
    pub fn to_texture15(&self, clut: &Clut, name: &str) -> Texture15 {
        let pixels: Vec<Color15> = self.indices
            .iter()
            .map(|&idx| clut.lookup(idx))
            .collect();

        Texture15 {
            width: self.width,
            height: self.height,
            pixels,
            name: name.to_string(),
            blend_mode: crate::rasterizer::BlendMode::Opaque,
        }
    }

    /// Total number of pixels
    pub fn pixel_count(&self) -> usize {
        self.width * self.height
    }

    /// Get pixel color using a CLUT (for preview rendering)
    pub fn get_color(&self, x: usize, y: usize, clut: &Clut) -> Color15 {
        let index = self.get_index(x, y);
        clut.lookup(index)
    }

    /// Resize the atlas (resamples indices using nearest-neighbor)
    pub fn resize(&mut self, new_width: usize, new_height: usize) {
        if new_width == self.width && new_height == self.height {
            return;
        }
        let mut new_indices = vec![0u8; new_width * new_height];
        for y in 0..new_height {
            for x in 0..new_width {
                // Nearest-neighbor sampling from old atlas
                let src_x = (x * self.width) / new_width;
                let src_y = (y * self.height) / new_height;
                let src_idx = src_y * self.width + src_x;
                let dst_idx = y * new_width + x;
                new_indices[dst_idx] = self.indices.get(src_idx).copied().unwrap_or(0);
            }
        }
        self.width = new_width;
        self.height = new_height;
        self.indices = new_indices;
    }

    /// Convert to 24-bit raster Texture for rendering (non-indexed)
    /// Uses the provided CLUT to look up colors
    pub fn to_raster_texture(&self, clut: &Clut, name: &str) -> crate::rasterizer::Texture {
        let mut pixels = Vec::with_capacity(self.width * self.height);
        for &index in &self.indices {
            let c15 = clut.lookup(index);
            // Convert Color15 to RGB24 (5-bit to 8-bit)
            let r = (c15.r5() << 3) | (c15.r5() >> 2);
            let g = (c15.g5() << 3) | (c15.g5() >> 2);
            let b = (c15.b5() << 3) | (c15.b5() >> 2);
            let blend = if index == 0 {
                crate::rasterizer::BlendMode::Erase // Index 0 = transparent
            } else {
                crate::rasterizer::BlendMode::Opaque
            };
            pixels.push(crate::rasterizer::Color::with_blend(r, g, b, blend));
        }
        crate::rasterizer::Texture {
            width: self.width,
            height: self.height,
            pixels,
            name: name.to_string(),
            blend_mode: crate::rasterizer::BlendMode::Opaque,
        }
    }
}

/// Main data structure for the mesh editor workflow
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MeshEditorModel {
    pub version: u32,
    pub name: String,
    pub skeleton: Skeleton,
    pub mesh: EditableMesh,
    pub bone_assignments: BoneAssignments,
}

impl MeshEditorModel {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            version: 1,
            name: name.into(),
            skeleton: Skeleton::new(),
            mesh: EditableMesh::new(),
            bone_assignments: BoneAssignments::new(),
        }
    }

    pub fn from_mesh(name: impl Into<String>, mesh: EditableMesh) -> Self {
        let face_count = mesh.faces.len();
        Self {
            version: 1,
            name: name.into(),
            skeleton: Skeleton::new(),
            mesh,
            bone_assignments: BoneAssignments::with_face_count(face_count),
        }
    }

    /// Save mesh editor model to file (compressed RON format with brotli)
    #[cfg(not(target_arch = "wasm32"))]
    pub fn save_to_file(&self, path: &Path) -> Result<(), MeshEditorError> {
        let config = ron::ser::PrettyConfig::new()
            .depth_limit(4)
            .indentor("  ".to_string());
        let ron_data = ron::ser::to_string_pretty(self, config)
            .map_err(|e| MeshEditorError::Serialization(e.to_string()))?;

        // Compress with brotli (quality 6, window 22 - good balance of speed/ratio)
        let mut compressed = Vec::new();
        brotli::BrotliCompress(
            &mut Cursor::new(ron_data.as_bytes()),
            &mut compressed,
            &brotli::enc::BrotliEncoderParams {
                quality: 6,
                lgwin: 22,
                ..Default::default()
            },
        ).map_err(|e| MeshEditorError::Io(format!("compression failed: {}", e)))?;

        std::fs::write(path, compressed)
            .map_err(|e| MeshEditorError::Io(e.to_string()))?;
        Ok(())
    }

    /// Load mesh editor model from file (supports both compressed and uncompressed RON)
    #[cfg(not(target_arch = "wasm32"))]
    pub fn load_from_file(path: &Path) -> Result<Self, MeshEditorError> {
        let bytes = std::fs::read(path)
            .map_err(|e| MeshEditorError::Io(e.to_string()))?;

        // Detect format: RON files start with '(' or whitespace, brotli is binary
        let is_plain_ron = bytes.first().map(|&b| b == b'(' || b == b' ' || b == b'\n' || b == b'\r' || b == b'\t').unwrap_or(false);

        let ron_data = if is_plain_ron {
            String::from_utf8(bytes)
                .map_err(|e| MeshEditorError::Io(format!("invalid UTF-8: {}", e)))?
        } else {
            let mut decompressed = Vec::new();
            brotli::BrotliDecompress(&mut Cursor::new(&bytes), &mut decompressed)
                .map_err(|e| MeshEditorError::Io(format!("decompression failed: {}", e)))?;
            String::from_utf8(decompressed)
                .map_err(|e| MeshEditorError::Io(format!("invalid UTF-8: {}", e)))?
        };

        let model: MeshEditorModel = ron::from_str(&ron_data)
            .map_err(|e| MeshEditorError::Serialization(e.to_string()))?;
        Ok(model)
    }

    /// Validate the model before export
    pub fn validate(&self) -> Result<(), MeshEditorError> {
        // Check all faces are assigned
        let unassigned = self.bone_assignments.unassigned_faces();
        if !unassigned.is_empty() {
            return Err(MeshEditorError::Validation(format!(
                "{} faces are not assigned to any bone",
                unassigned.len()
            )));
        }

        // Check skeleton is not empty
        if self.skeleton.bones.is_empty() {
            return Err(MeshEditorError::Validation(
                "Skeleton has no bones".to_string()
            ));
        }

        // Check no cyclic dependencies in bone hierarchy
        for (i, bone) in self.skeleton.bones.iter().enumerate() {
            if let Some(parent) = bone.parent {
                if parent == i {
                    return Err(MeshEditorError::Validation(format!(
                        "Bone '{}' is its own parent",
                        bone.name
                    )));
                }
                // Simple cycle detection (could be more thorough)
                let mut current = parent;
                let mut depth = 0;
                while depth < self.skeleton.bones.len() {
                    if current == i {
                        return Err(MeshEditorError::Validation(format!(
                            "Cyclic dependency in bone hierarchy involving '{}'",
                            bone.name
                        )));
                    }
                    if let Some(next_parent) = self.skeleton.bones[current].parent {
                        current = next_parent;
                    } else {
                        break;
                    }
                    depth += 1;
                }
            }
        }

        Ok(())
    }
}

/// Skeleton with hierarchical bone structure
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Skeleton {
    pub bones: Vec<EditorBone>,
}

impl Skeleton {
    pub fn new() -> Self {
        Self { bones: Vec::new() }
    }

    pub fn add_bone(&mut self, bone: EditorBone) -> usize {
        let index = self.bones.len();
        self.bones.push(bone);
        index
    }

    pub fn remove_bone(&mut self, index: usize) -> Option<EditorBone> {
        if index >= self.bones.len() {
            return None;
        }

        // Update parent indices for bones that reference removed bone
        for bone in &mut self.bones {
            if bone.parent == Some(index) {
                bone.parent = None; // Orphan children
            } else if let Some(parent) = bone.parent {
                if parent > index {
                    bone.parent = Some(parent - 1); // Shift down indices
                }
            }
        }

        Some(self.bones.remove(index))
    }

    pub fn get_root_bones(&self) -> Vec<usize> {
        self.bones
            .iter()
            .enumerate()
            .filter_map(|(i, bone)| if bone.parent.is_none() { Some(i) } else { None })
            .collect()
    }
}

impl Default for Skeleton {
    fn default() -> Self {
        Self::new()
    }
}

/// Bone in the editor (before export to runtime format)
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EditorBone {
    pub name: String,
    pub parent: Option<usize>,
    pub position: Vec3,
    pub rotation: Vec3, // Euler angles for visualization
    pub length: f32,    // Visual representation length
}

impl EditorBone {
    pub fn new(name: impl Into<String>, position: Vec3) -> Self {
        Self {
            name: name.into(),
            parent: None,
            position,
            rotation: Vec3::ZERO,
            length: 10.0,
        }
    }

    pub fn with_parent(mut self, parent: usize) -> Self {
        self.parent = Some(parent);
        self
    }

    pub fn with_length(mut self, length: f32) -> Self {
        self.length = length;
        self
    }
}

/// Editable mesh with n-gon face support
///
/// Faces can be triangles, quads, or arbitrary n-gons.
/// Triangulation happens in `to_render_data()` for the rasterizer.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EditableMesh {
    pub vertices: Vec<Vertex>,
    pub faces: Vec<EditFace>,
}

impl EditableMesh {
    pub fn new() -> Self {
        Self {
            vertices: Vec::new(),
            faces: Vec::new(),
        }
    }

    pub fn from_parts(vertices: Vec<Vertex>, faces: Vec<EditFace>) -> Self {
        Self { vertices, faces }
    }

    /// Create a cube primitive centered at origin
    pub fn cube(size: f32) -> Self {
        use crate::rasterizer::Vec2;

        let half = size / 2.0;
        let vertices = vec![
            // Front face
            Vertex::new(Vec3::new(-half, -half,  half), Vec2::new(0.0, 1.0), Vec3::new(0.0, 0.0, 1.0)),
            Vertex::new(Vec3::new( half, -half,  half), Vec2::new(1.0, 1.0), Vec3::new(0.0, 0.0, 1.0)),
            Vertex::new(Vec3::new( half,  half,  half), Vec2::new(1.0, 0.0), Vec3::new(0.0, 0.0, 1.0)),
            Vertex::new(Vec3::new(-half,  half,  half), Vec2::new(0.0, 0.0), Vec3::new(0.0, 0.0, 1.0)),
            // Back face
            Vertex::new(Vec3::new( half, -half, -half), Vec2::new(0.0, 1.0), Vec3::new(0.0, 0.0, -1.0)),
            Vertex::new(Vec3::new(-half, -half, -half), Vec2::new(1.0, 1.0), Vec3::new(0.0, 0.0, -1.0)),
            Vertex::new(Vec3::new(-half,  half, -half), Vec2::new(1.0, 0.0), Vec3::new(0.0, 0.0, -1.0)),
            Vertex::new(Vec3::new( half,  half, -half), Vec2::new(0.0, 0.0), Vec3::new(0.0, 0.0, -1.0)),
            // Top face
            Vertex::new(Vec3::new(-half,  half,  half), Vec2::new(0.0, 1.0), Vec3::new(0.0, 1.0, 0.0)),
            Vertex::new(Vec3::new( half,  half,  half), Vec2::new(1.0, 1.0), Vec3::new(0.0, 1.0, 0.0)),
            Vertex::new(Vec3::new( half,  half, -half), Vec2::new(1.0, 0.0), Vec3::new(0.0, 1.0, 0.0)),
            Vertex::new(Vec3::new(-half,  half, -half), Vec2::new(0.0, 0.0), Vec3::new(0.0, 1.0, 0.0)),
            // Bottom face
            Vertex::new(Vec3::new(-half, -half, -half), Vec2::new(0.0, 1.0), Vec3::new(0.0, -1.0, 0.0)),
            Vertex::new(Vec3::new( half, -half, -half), Vec2::new(1.0, 1.0), Vec3::new(0.0, -1.0, 0.0)),
            Vertex::new(Vec3::new( half, -half,  half), Vec2::new(1.0, 0.0), Vec3::new(0.0, -1.0, 0.0)),
            Vertex::new(Vec3::new(-half, -half,  half), Vec2::new(0.0, 0.0), Vec3::new(0.0, -1.0, 0.0)),
            // Right face
            Vertex::new(Vec3::new( half, -half,  half), Vec2::new(0.0, 1.0), Vec3::new(1.0, 0.0, 0.0)),
            Vertex::new(Vec3::new( half, -half, -half), Vec2::new(1.0, 1.0), Vec3::new(1.0, 0.0, 0.0)),
            Vertex::new(Vec3::new( half,  half, -half), Vec2::new(1.0, 0.0), Vec3::new(1.0, 0.0, 0.0)),
            Vertex::new(Vec3::new( half,  half,  half), Vec2::new(0.0, 0.0), Vec3::new(1.0, 0.0, 0.0)),
            // Left face
            Vertex::new(Vec3::new(-half, -half, -half), Vec2::new(0.0, 1.0), Vec3::new(-1.0, 0.0, 0.0)),
            Vertex::new(Vec3::new(-half, -half,  half), Vec2::new(1.0, 1.0), Vec3::new(-1.0, 0.0, 0.0)),
            Vertex::new(Vec3::new(-half,  half,  half), Vec2::new(1.0, 0.0), Vec3::new(-1.0, 0.0, 0.0)),
            Vertex::new(Vec3::new(-half,  half, -half), Vec2::new(0.0, 0.0), Vec3::new(-1.0, 0.0, 0.0)),
        ];

        // Quad faces (CW winding for rasterizer - triangulated in to_render_data)
        let faces = vec![
            EditFace::quad(0, 3, 2, 1),    // Front
            EditFace::quad(4, 7, 6, 5),    // Back
            EditFace::quad(8, 11, 10, 9),  // Top
            EditFace::quad(12, 15, 14, 13), // Bottom
            EditFace::quad(16, 19, 18, 17), // Right
            EditFace::quad(20, 23, 22, 21), // Left
        ];

        Self { vertices, faces }
    }

    /// Create a plane primitive on the XZ plane, centered at origin
    pub fn plane(size: f32) -> Self {
        use crate::rasterizer::Vec2;

        let half = size / 2.0;
        let vertices = vec![
            Vertex::new(Vec3::new(-half, 0.0, -half), Vec2::new(0.0, 0.0), Vec3::new(0.0, 1.0, 0.0)),
            Vertex::new(Vec3::new( half, 0.0, -half), Vec2::new(1.0, 0.0), Vec3::new(0.0, 1.0, 0.0)),
            Vertex::new(Vec3::new( half, 0.0,  half), Vec2::new(1.0, 1.0), Vec3::new(0.0, 1.0, 0.0)),
            Vertex::new(Vec3::new(-half, 0.0,  half), Vec2::new(0.0, 1.0), Vec3::new(0.0, 1.0, 0.0)),
        ];

        // Single quad face (CW winding for rasterizer)
        let faces = vec![EditFace::quad(0, 1, 2, 3)];

        Self { vertices, faces }
    }

    /// Create a triangular prism (wedge) primitive
    pub fn prism(size: f32, height: f32) -> Self {
        use crate::rasterizer::Vec2;

        let half = size / 2.0;
        let h = height;

        // 6 vertices: 3 on bottom, 3 on top
        let vertices = vec![
            // Bottom triangle (Y=0)
            Vertex::new(Vec3::new(-half, 0.0, -half), Vec2::new(0.0, 1.0), Vec3::new(0.0, -1.0, 0.0)),
            Vertex::new(Vec3::new( half, 0.0, -half), Vec2::new(1.0, 1.0), Vec3::new(0.0, -1.0, 0.0)),
            Vertex::new(Vec3::new( 0.0,  0.0,  half), Vec2::new(0.5, 0.0), Vec3::new(0.0, -1.0, 0.0)),
            // Top triangle (Y=height)
            Vertex::new(Vec3::new(-half, h, -half), Vec2::new(0.0, 1.0), Vec3::new(0.0, 1.0, 0.0)),
            Vertex::new(Vec3::new( half, h, -half), Vec2::new(1.0, 1.0), Vec3::new(0.0, 1.0, 0.0)),
            Vertex::new(Vec3::new( 0.0,  h,  half), Vec2::new(0.5, 0.0), Vec3::new(0.0, 1.0, 0.0)),
        ];

        // Faces (CW winding for rasterizer, matches cube)
        let faces = vec![
            // Bottom and top triangles
            EditFace::tri(0, 1, 2),   // Bottom (CW from below)
            EditFace::tri(3, 4, 5),   // Top (CW from above)
            // Side faces are quads (CW from outside)
            EditFace::quad(0, 1, 4, 3), // Back face
            EditFace::quad(1, 2, 5, 4), // Right face
            EditFace::quad(2, 0, 3, 5), // Left face
        ];

        Self { vertices, faces }
    }

    /// Create a cylinder primitive with given segments
    pub fn cylinder(radius: f32, height: f32, segments: usize) -> Self {
        use crate::rasterizer::Vec2;
        use std::f32::consts::PI;

        let segments = segments.max(3);
        let mut vertices = Vec::new();
        let mut faces = Vec::new();

        // Ring vertices for caps
        let bottom_ring_start = vertices.len();
        for i in 0..segments {
            let angle = (i as f32 / segments as f32) * 2.0 * PI;
            let x = angle.cos() * radius;
            let z = angle.sin() * radius;
            let u = 0.5 + angle.cos() * 0.5;
            let v = 0.5 + angle.sin() * 0.5;

            // Bottom ring (for cap)
            vertices.push(Vertex::new(Vec3::new(x, 0.0, z), Vec2::new(u, v), Vec3::new(0.0, -1.0, 0.0)));
        }

        let top_ring_start = vertices.len();
        for i in 0..segments {
            let angle = (i as f32 / segments as f32) * 2.0 * PI;
            let x = angle.cos() * radius;
            let z = angle.sin() * radius;
            let u = 0.5 + angle.cos() * 0.5;
            let v = 0.5 + angle.sin() * 0.5;

            // Top ring (for cap)
            vertices.push(Vertex::new(Vec3::new(x, height, z), Vec2::new(u, v), Vec3::new(0.0, 1.0, 0.0)));
        }

        // Side vertices (need separate for proper normals)
        let side_bottom_start = vertices.len();
        for i in 0..segments {
            let angle = (i as f32 / segments as f32) * 2.0 * PI;
            let x = angle.cos() * radius;
            let z = angle.sin() * radius;
            let normal = Vec3::new(angle.cos(), 0.0, angle.sin());
            let u = i as f32 / segments as f32;

            vertices.push(Vertex::new(Vec3::new(x, 0.0, z), Vec2::new(u, 1.0), normal));
        }

        let side_top_start = vertices.len();
        for i in 0..segments {
            let angle = (i as f32 / segments as f32) * 2.0 * PI;
            let x = angle.cos() * radius;
            let z = angle.sin() * radius;
            let normal = Vec3::new(angle.cos(), 0.0, angle.sin());
            let u = i as f32 / segments as f32;

            vertices.push(Vertex::new(Vec3::new(x, height, z), Vec2::new(u, 0.0), normal));
        }

        // Bottom cap face (single n-gon, CW winding for rasterizer)
        // Reversed order so normal points down (-Y)
        let bottom_cap_verts: Vec<usize> = (0..segments).rev().map(|i| bottom_ring_start + i).collect();
        faces.push(EditFace::ngon(&bottom_cap_verts));

        // Top cap face (single n-gon, CW winding for rasterizer)
        // Normal order so normal points up (+Y)
        let top_cap_verts: Vec<usize> = (0..segments).map(|i| top_ring_start + i).collect();
        faces.push(EditFace::ngon(&top_cap_verts));

        // Side faces (quads, CW winding for rasterizer)
        for i in 0..segments {
            let next = (i + 1) % segments;
            faces.push(EditFace::quad(
                side_bottom_start + i,
                side_bottom_start + next,
                side_top_start + next,
                side_top_start + i,
            ));
        }

        Self { vertices, faces }
    }

    /// Create a pyramid primitive
    pub fn pyramid(base_size: f32, height: f32) -> Self {
        use crate::rasterizer::Vec2;

        let half = base_size / 2.0;

        // 5 vertices: 4 base corners + 1 apex
        let vertices = vec![
            // Base corners (Y=0)
            Vertex::new(Vec3::new(-half, 0.0, -half), Vec2::new(0.0, 0.0), Vec3::new(0.0, -1.0, 0.0)),
            Vertex::new(Vec3::new( half, 0.0, -half), Vec2::new(1.0, 0.0), Vec3::new(0.0, -1.0, 0.0)),
            Vertex::new(Vec3::new( half, 0.0,  half), Vec2::new(1.0, 1.0), Vec3::new(0.0, -1.0, 0.0)),
            Vertex::new(Vec3::new(-half, 0.0,  half), Vec2::new(0.0, 1.0), Vec3::new(0.0, -1.0, 0.0)),
            // Apex
            Vertex::new(Vec3::new(0.0, height, 0.0), Vec2::new(0.5, 0.5), Vec3::new(0.0, 1.0, 0.0)),
        ];

        // Faces (CW winding for rasterizer, matches cube)
        let faces = vec![
            // Base (quad, CW from below)
            EditFace::quad(0, 3, 2, 1),
            // Side faces (triangles connecting to apex, CW from outside)
            EditFace::tri(0, 1, 4), // Front (-Z side)
            EditFace::tri(1, 2, 4), // Right (+X side)
            EditFace::tri(2, 3, 4), // Back (+Z side)
            EditFace::tri(3, 0, 4), // Left (-X side)
        ];

        Self { vertices, faces }
    }

    /// Create a pentagon-based prism
    pub fn pent(radius: f32, height: f32) -> Self {
        Self::ngon_prism(5, radius, height)
    }

    /// Create a hexagon-based prism
    pub fn hex(radius: f32, height: f32) -> Self {
        Self::ngon_prism(6, radius, height)
    }

    /// Create an N-sided prism (generalized)
    pub fn ngon_prism(sides: usize, radius: f32, height: f32) -> Self {
        use crate::rasterizer::Vec2;
        use std::f32::consts::PI;

        let sides = sides.max(3);
        let mut vertices = Vec::new();
        let mut faces = Vec::new();

        // Bottom ring
        let bottom_start = vertices.len();
        for i in 0..sides {
            let angle = (i as f32 / sides as f32) * 2.0 * PI;
            let x = angle.cos() * radius;
            let z = angle.sin() * radius;
            vertices.push(Vertex::new(Vec3::new(x, 0.0, z), Vec2::new(0.5 + angle.cos() * 0.5, 0.5 + angle.sin() * 0.5), Vec3::new(0.0, -1.0, 0.0)));
        }

        // Top ring
        let top_start = vertices.len();
        for i in 0..sides {
            let angle = (i as f32 / sides as f32) * 2.0 * PI;
            let x = angle.cos() * radius;
            let z = angle.sin() * radius;
            vertices.push(Vertex::new(Vec3::new(x, height, z), Vec2::new(0.5 + angle.cos() * 0.5, 0.5 + angle.sin() * 0.5), Vec3::new(0.0, 1.0, 0.0)));
        }

        // Bottom cap face (single n-gon, CW winding for rasterizer)
        let bottom_cap_verts: Vec<usize> = (0..sides).rev().map(|i| bottom_start + i).collect();
        faces.push(EditFace::ngon(&bottom_cap_verts));

        // Top cap face (single n-gon, CW winding for rasterizer)
        let top_cap_verts: Vec<usize> = (0..sides).map(|i| top_start + i).collect();
        faces.push(EditFace::ngon(&top_cap_verts));

        // Side faces (quads, CW winding for rasterizer)
        for i in 0..sides {
            let next = (i + 1) % sides;
            faces.push(EditFace::quad(
                bottom_start + i,
                bottom_start + next,
                top_start + next,
                top_start + i,
            ));
        }

        Self { vertices, faces }
    }

    /// Merge another mesh into this one (for adding primitives)
    pub fn merge(&mut self, other: &EditableMesh, offset: Vec3) {
        let vertex_offset = self.vertices.len();

        // Add vertices with position offset
        for v in &other.vertices {
            let mut new_v = v.clone();
            new_v.pos.x += offset.x;
            new_v.pos.y += offset.y;
            new_v.pos.z += offset.z;
            self.vertices.push(new_v);
        }

        // Add faces with index offset
        for f in &other.faces {
            let new_verts: Vec<usize> = f.vertices.iter().map(|&v| v + vertex_offset).collect();
            self.faces.push(EditFace {
                vertices: new_verts,
                texture_id: f.texture_id,
                black_transparent: f.black_transparent,
                blend_mode: f.blend_mode,
            });
        }
    }

    pub fn vertex_count(&self) -> usize {
        self.vertices.len()
    }

    pub fn face_count(&self) -> usize {
        self.faces.len()
    }

    /// Get all vertex indices used by a face (as a slice)
    pub fn face_vertices(&self, face_idx: usize) -> Option<&[usize]> {
        self.faces.get(face_idx).map(|f| f.vertices.as_slice())
    }

    /// Compute centroid of a face (works for any n-gon)
    pub fn face_centroid(&self, face_idx: usize) -> Option<Vec3> {
        let face = self.faces.get(face_idx)?;
        if face.vertices.is_empty() {
            return None;
        }

        let mut sum = Vec3::ZERO;
        let mut count = 0;
        for &vi in &face.vertices {
            if let Some(v) = self.vertices.get(vi) {
                sum.x += v.pos.x;
                sum.y += v.pos.y;
                sum.z += v.pos.z;
                count += 1;
            }
        }

        if count > 0 {
            Some(Vec3::new(sum.x / count as f32, sum.y / count as f32, sum.z / count as f32))
        } else {
            None
        }
    }

    /// Compute face normal for CW-wound faces (pointing outward)
    /// Uses first 3 vertices for normal calculation (works for n-gons)
    pub fn face_normal(&self, face_idx: usize) -> Option<Vec3> {
        let face = self.faces.get(face_idx)?;
        if face.vertices.len() < 3 {
            return Some(Vec3::new(0.0, 1.0, 0.0)); // Default up for degenerate
        }

        let v0 = self.vertices.get(face.vertices[0])?.pos;
        let v1 = self.vertices.get(face.vertices[1])?.pos;
        let v2 = self.vertices.get(face.vertices[2])?.pos;

        // Edge vectors
        let e1 = v1 - v0;
        let e2 = v2 - v0;

        // Cross product: e2 × e1 for CW winding (reversed from CCW convention)
        // This gives outward-facing normal for CW-wound faces
        let normal = e2.cross(e1);

        // Normalize
        let len = normal.len();
        if len > 0.0001 {
            Some(Vec3::new(normal.x / len, normal.y / len, normal.z / len))
        } else {
            Some(Vec3::new(0.0, 1.0, 0.0)) // Default up if degenerate
        }
    }

    /// Find all vertices at approximately the same position as the given vertex
    /// Returns indices of coincident vertices (including the input vertex)
    pub fn find_coincident_vertices(&self, idx: usize, epsilon: f32) -> Vec<usize> {
        let Some(pos) = self.vertices.get(idx).map(|v| v.pos) else {
            return vec![];
        };

        self.vertices.iter().enumerate()
            .filter(|(_, v)| {
                let d = v.pos - pos;
                (d.x * d.x + d.y * d.y + d.z * d.z).sqrt() < epsilon
            })
            .map(|(i, _)| i)
            .collect()
    }

    /// Expand a set of vertex indices to include all coincident vertices
    pub fn expand_to_coincident(&self, indices: &[usize], epsilon: f32) -> Vec<usize> {
        let mut result = std::collections::HashSet::new();
        for &idx in indices {
            for coincident in self.find_coincident_vertices(idx, epsilon) {
                result.insert(coincident);
            }
        }
        result.into_iter().collect()
    }

    /// Extrude selected faces by a given amount along their normals
    /// Returns the indices of the new top faces (for selection update)
    pub fn extrude_faces(&mut self, face_indices: &[usize], amount: f32) -> Vec<usize> {
        use std::collections::{HashMap, HashSet};

        if face_indices.is_empty() || amount.abs() < 0.001 {
            return face_indices.to_vec();
        }

        // Collect all unique vertices from selected faces
        let mut vertex_set: Vec<usize> = face_indices.iter()
            .filter_map(|&fi| self.faces.get(fi))
            .flat_map(|f| f.vertices.iter().cloned())
            .collect();
        vertex_set.sort();
        vertex_set.dedup();

        // Compute average normal for extrusion direction
        let mut avg_normal = Vec3::ZERO;
        for &fi in face_indices {
            if let Some(n) = self.face_normal(fi) {
                avg_normal.x += n.x;
                avg_normal.y += n.y;
                avg_normal.z += n.z;
            }
        }
        let len = (avg_normal.x * avg_normal.x + avg_normal.y * avg_normal.y + avg_normal.z * avg_normal.z).sqrt();
        if len > 0.0001 {
            avg_normal.x /= len;
            avg_normal.y /= len;
            avg_normal.z /= len;
        } else {
            avg_normal = Vec3::new(0.0, 1.0, 0.0);
        }

        // Create new vertices (copies of originals, offset by extrusion)
        let mut old_to_new: HashMap<usize, usize> = HashMap::new();
        for &vi in &vertex_set {
            if let Some(old_vert) = self.vertices.get(vi) {
                let new_vert = Vertex::new(
                    Vec3::new(
                        old_vert.pos.x + avg_normal.x * amount,
                        old_vert.pos.y + avg_normal.y * amount,
                        old_vert.pos.z + avg_normal.z * amount,
                    ),
                    old_vert.uv,
                    old_vert.normal,
                );
                let new_idx = self.vertices.len();
                self.vertices.push(new_vert);
                old_to_new.insert(vi, new_idx);
            }
        }

        // Collect directed edges from selected faces, preserving winding order
        // Each edge stored as (v_from, v_to) in face winding order
        let mut directed_edges: Vec<(usize, usize)> = Vec::new();
        for &fi in face_indices {
            if let Some(face) = self.faces.get(fi) {
                // Collect edges from n-gon face
                for edge in face.edges() {
                    directed_edges.push(edge);
                }
            }
        }

        // Find boundary edges: edges where the reverse direction doesn't exist
        // (internal edges have both directions from adjacent faces)
        let edge_set: HashSet<(usize, usize)> = directed_edges.iter().cloned().collect();
        let boundary_edges: Vec<(usize, usize)> = directed_edges.iter()
            .filter(|(a, b)| !edge_set.contains(&(*b, *a)))
            .cloned()
            .collect();

        // Create side faces (quads) for each boundary edge
        // The edge (v0, v1) is in the winding order of the original face
        for (v0_old, v1_old) in boundary_edges {
            if let (Some(&v0_new), Some(&v1_new)) = (old_to_new.get(&v0_old), old_to_new.get(&v1_old)) {
                // Get positions
                let p0_old = self.vertices[v0_old].pos;
                let p1_old = self.vertices[v1_old].pos;
                let p0_new = self.vertices[v0_new].pos;
                let p1_new = self.vertices[v1_new].pos;

                // Compute the face normal from the actual quad geometry
                let e1 = p1_new - p1_old;
                let e2 = p0_new - p1_old;
                let side_normal = e2.cross(e1).normalize();

                // Get UVs for side face
                let uv00 = Vec2::new(0.0, 0.0);
                let uv01 = Vec2::new(0.0, 1.0);
                let uv11 = Vec2::new(1.0, 1.0);
                let uv10 = Vec2::new(1.0, 0.0);

                // Create 4 vertices for the quad with the computed normal
                // Quad: v1_old -> v1_new -> v0_new -> v0_old (CW when viewed from outside)
                let sv0 = Vertex::new(p1_old, uv00, side_normal);
                let sv1 = Vertex::new(p1_new, uv01, side_normal);
                let sv2 = Vertex::new(p0_new, uv11, side_normal);
                let sv3 = Vertex::new(p0_old, uv10, side_normal);

                let si0 = self.vertices.len();
                self.vertices.push(sv0);
                self.vertices.push(sv1);
                self.vertices.push(sv2);
                self.vertices.push(sv3);

                // Single quad face (triangulated at render time)
                self.faces.push(EditFace::quad(si0, si0 + 1, si0 + 2, si0 + 3));
            }
        }

        // Update original faces to use new (extruded) vertices
        let mut new_top_faces = Vec::new();
        for &fi in face_indices {
            if let Some(face) = self.faces.get_mut(fi) {
                let mut all_mapped = true;
                let new_verts: Vec<usize> = face.vertices.iter()
                    .map(|&v| {
                        if let Some(&nv) = old_to_new.get(&v) {
                            nv
                        } else {
                            all_mapped = false;
                            v
                        }
                    })
                    .collect();

                if all_mapped {
                    face.vertices = new_verts;
                    new_top_faces.push(fi);
                }
            }
        }

        new_top_faces
    }

    /// Save mesh to file (compressed RON format with brotli)
    #[cfg(not(target_arch = "wasm32"))]
    pub fn save_to_file(&self, path: &std::path::Path) -> Result<(), MeshEditorError> {
        use std::io::Cursor;
        let config = ron::ser::PrettyConfig::default();
        let ron_data = ron::ser::to_string_pretty(self, config)
            .map_err(|e| MeshEditorError::Serialization(e.to_string()))?;

        // Compress with brotli (quality 6, window 22 - good balance of speed/ratio)
        let mut compressed = Vec::new();
        brotli::BrotliCompress(
            &mut Cursor::new(ron_data.as_bytes()),
            &mut compressed,
            &brotli::enc::BrotliEncoderParams {
                quality: 6,
                lgwin: 22,
                ..Default::default()
            },
        ).map_err(|e| MeshEditorError::Io(format!("compression failed: {}", e)))?;

        std::fs::write(path, compressed)
            .map_err(|e| MeshEditorError::Io(e.to_string()))?;
        Ok(())
    }

    /// Load mesh from file (supports both compressed and uncompressed RON)
    #[cfg(not(target_arch = "wasm32"))]
    pub fn load_from_file(path: &std::path::Path) -> Result<Self, MeshEditorError> {
        use std::io::Cursor;
        let bytes = std::fs::read(path)
            .map_err(|e| MeshEditorError::Io(e.to_string()))?;

        // Detect format: RON files start with '(' or whitespace, brotli is binary
        let is_plain_ron = bytes.first().map(|&b| b == b'(' || b == b' ' || b == b'\n' || b == b'\r' || b == b'\t').unwrap_or(false);

        let ron_data = if is_plain_ron {
            String::from_utf8(bytes)
                .map_err(|e| MeshEditorError::Io(format!("invalid UTF-8: {}", e)))?
        } else {
            let mut decompressed = Vec::new();
            brotli::BrotliDecompress(&mut Cursor::new(&bytes), &mut decompressed)
                .map_err(|e| MeshEditorError::Io(format!("decompression failed: {}", e)))?;
            String::from_utf8(decompressed)
                .map_err(|e| MeshEditorError::Io(format!("invalid UTF-8: {}", e)))?
        };

        let mesh: EditableMesh = ron::from_str(&ron_data)
            .map_err(|e| MeshEditorError::Serialization(e.to_string()))?;
        Ok(mesh)
    }

    /// Convert to render data (vertices and faces for the rasterizer) - no texture
    ///
    /// N-gon faces are triangulated here using fan triangulation.
    pub fn to_render_data(&self) -> (Vec<crate::rasterizer::Vertex>, Vec<crate::rasterizer::Face>) {
        use crate::rasterizer::{Vertex as RasterVertex, Face as RasterFace};

        let raster_vertices: Vec<RasterVertex> = self.vertices.iter().map(|v| {
            RasterVertex {
                pos: v.pos,
                uv: v.uv,
                normal: v.normal,
                color: v.color,
                bone_index: None,
            }
        }).collect();

        // Triangulate n-gon faces
        let mut raster_faces: Vec<RasterFace> = Vec::new();
        for edit_face in &self.faces {
            for [v0, v1, v2] in edit_face.triangulate() {
                raster_faces.push(RasterFace {
                    v0,
                    v1,
                    v2,
                    texture_id: None,
                    black_transparent: edit_face.black_transparent,
                    blend_mode: edit_face.blend_mode,
                });
            }
        }

        (raster_vertices, raster_faces)
    }

    /// Convert to render data with texture atlas (texture_id = 0 for all faces)
    ///
    /// N-gon faces are triangulated here using fan triangulation.
    pub fn to_render_data_textured(&self) -> (Vec<crate::rasterizer::Vertex>, Vec<crate::rasterizer::Face>) {
        use crate::rasterizer::{Vertex as RasterVertex, Face as RasterFace};

        let raster_vertices: Vec<RasterVertex> = self.vertices.iter().map(|v| {
            RasterVertex {
                pos: v.pos,
                uv: v.uv,
                normal: v.normal,
                color: v.color,
                bone_index: None,
            }
        }).collect();

        // Triangulate n-gon faces
        let mut raster_faces: Vec<RasterFace> = Vec::new();
        for edit_face in &self.faces {
            for [v0, v1, v2] in edit_face.triangulate() {
                raster_faces.push(RasterFace {
                    v0,
                    v1,
                    v2,
                    texture_id: edit_face.texture_id.or(Some(0)), // Use face texture or atlas
                    black_transparent: edit_face.black_transparent,
                    blend_mode: edit_face.blend_mode,
                });
            }
        }

        (raster_vertices, raster_faces)
    }

    /// Merge vertices that are within a distance threshold.
    /// Returns the number of vertices that were merged.
    ///
    /// This is useful after mirror operations to weld center vertices,
    /// or for general cleanup of coincident vertices.
    pub fn merge_by_distance(&mut self, threshold: f32) -> usize {
        use std::collections::{HashMap, HashSet};

        let threshold_sq = threshold * threshold;
        let n = self.vertices.len();

        // Build groups of vertices that should be merged
        // Using union-find style grouping
        let mut parent: Vec<usize> = (0..n).collect();

        fn find(parent: &mut [usize], i: usize) -> usize {
            if parent[i] != i {
                parent[i] = find(parent, parent[i]);
            }
            parent[i]
        }

        fn union(parent: &mut [usize], i: usize, j: usize) {
            let pi = find(parent, i);
            let pj = find(parent, j);
            if pi != pj {
                // Always merge to the lower index (keeps first vertex)
                if pi < pj {
                    parent[pj] = pi;
                } else {
                    parent[pi] = pj;
                }
            }
        }

        // Find all pairs within threshold
        for i in 0..n {
            for j in (i + 1)..n {
                let diff = self.vertices[i].pos - self.vertices[j].pos;
                let dist_sq = diff.dot(diff);
                if dist_sq <= threshold_sq {
                    union(&mut parent, i, j);
                }
            }
        }

        // Flatten parent array
        for i in 0..n {
            find(&mut parent, i);
        }

        // Build mapping: old index -> new index
        // Group vertices by their root
        let mut root_to_new_idx: HashMap<usize, usize> = HashMap::new();
        let mut old_to_new: Vec<usize> = vec![0; n];
        let mut new_vertices: Vec<Vertex> = Vec::new();

        for i in 0..n {
            let root = parent[i];
            if let Some(&new_idx) = root_to_new_idx.get(&root) {
                old_to_new[i] = new_idx;
            } else {
                let new_idx = new_vertices.len();
                root_to_new_idx.insert(root, new_idx);
                old_to_new[i] = new_idx;
                // Use the root vertex's data (first vertex in the group)
                new_vertices.push(self.vertices[root].clone());
            }
        }

        let merged_count = n - new_vertices.len();

        if merged_count == 0 {
            return 0;
        }

        // Update face vertex indices
        for face in &mut self.faces {
            for vi in &mut face.vertices {
                *vi = old_to_new[*vi];
            }
        }

        // Remove degenerate faces (faces with duplicate vertices after merging)
        self.faces.retain(|face| {
            let unique: HashSet<_> = face.vertices.iter().collect();
            unique.len() >= 3 // Need at least 3 unique vertices
        });

        // Replace vertices
        self.vertices = new_vertices;

        merged_count
    }

    /// Merge selected vertices to their center point.
    /// Returns the index of the merged vertex, or None if less than 2 vertices selected.
    pub fn merge_to_center(&mut self, vertex_indices: &[usize]) -> Option<usize> {
        use std::collections::HashSet;

        if vertex_indices.len() < 2 {
            return None;
        }

        // Calculate center position
        let mut center = Vec3::ZERO;
        let mut count = 0;
        for &idx in vertex_indices {
            if let Some(vert) = self.vertices.get(idx) {
                center = center + vert.pos;
                count += 1;
            }
        }
        if count == 0 {
            return None;
        }
        center = center * (1.0 / count as f32);

        // Keep the first vertex, move it to center
        let keep_idx = vertex_indices[0];
        if let Some(vert) = self.vertices.get_mut(keep_idx) {
            vert.pos = center;
        }

        // Build set of vertices to remove
        let remove_set: HashSet<usize> = vertex_indices[1..].iter().copied().collect();

        // Remap face vertex references
        for face in &mut self.faces {
            for vi in &mut face.vertices {
                if remove_set.contains(vi) {
                    *vi = keep_idx;
                }
            }
        }

        // Remove degenerate faces
        self.faces.retain(|face| {
            let unique: HashSet<_> = face.vertices.iter().collect();
            unique.len() >= 3
        });

        // Note: This leaves orphaned vertices. Call compact_vertices() if needed.
        Some(keep_idx)
    }

    /// Remove unused vertices (vertices not referenced by any face).
    /// Returns the number of vertices removed.
    pub fn compact_vertices(&mut self) -> usize {
        use std::collections::HashSet;

        // Find all vertices referenced by faces
        let used: HashSet<usize> = self.faces.iter()
            .flat_map(|f| f.vertices.iter().copied())
            .collect();

        if used.len() == self.vertices.len() {
            return 0; // All vertices are used
        }

        // Build mapping: old index -> new index
        let mut old_to_new: Vec<Option<usize>> = vec![None; self.vertices.len()];
        let mut new_vertices: Vec<Vertex> = Vec::new();

        for (old_idx, vert) in self.vertices.iter().enumerate() {
            if used.contains(&old_idx) {
                old_to_new[old_idx] = Some(new_vertices.len());
                new_vertices.push(vert.clone());
            }
        }

        let removed = self.vertices.len() - new_vertices.len();

        // Update face indices
        for face in &mut self.faces {
            for vi in &mut face.vertices {
                if let Some(new_idx) = old_to_new[*vi] {
                    *vi = new_idx;
                }
            }
        }

        self.vertices = new_vertices;
        removed
    }

    /// Apply mirror: duplicate geometry from one side to the other.
    /// This bakes the virtual mirror into actual geometry.
    ///
    /// - Vertices on the center plane (within threshold) are shared
    /// - Vertices on the +X side are duplicated to -X (or appropriate axis)
    /// - Mirrored faces have reversed winding order for correct normals
    pub fn apply_mirror(&mut self, axis: super::state::Axis, threshold: f32) {
        use std::collections::HashMap;
        use super::state::Axis;

        // Helper to mirror a position
        let mirror_pos = |pos: Vec3| -> Vec3 {
            match axis {
                Axis::X => Vec3::new(-pos.x, pos.y, pos.z),
                Axis::Y => Vec3::new(pos.x, -pos.y, pos.z),
                Axis::Z => Vec3::new(pos.x, pos.y, -pos.z),
            }
        };

        // Helper to mirror a normal
        let mirror_normal = |n: Vec3| -> Vec3 {
            match axis {
                Axis::X => Vec3::new(-n.x, n.y, n.z),
                Axis::Y => Vec3::new(n.x, -n.y, n.z),
                Axis::Z => Vec3::new(n.x, n.y, -n.z),
            }
        };

        // Helper to check if position is on the center plane
        let is_on_plane = |pos: Vec3| -> bool {
            match axis {
                Axis::X => pos.x.abs() <= threshold,
                Axis::Y => pos.y.abs() <= threshold,
                Axis::Z => pos.z.abs() <= threshold,
            }
        };

        // Helper to check if position is on the positive side
        let is_positive_side = |pos: Vec3| -> bool {
            match axis {
                Axis::X => pos.x > threshold,
                Axis::Y => pos.y > threshold,
                Axis::Z => pos.z > threshold,
            }
        };

        // Map: original vertex index -> mirrored vertex index
        // For center vertices, maps to itself
        let mut vertex_map: HashMap<usize, usize> = HashMap::new();
        let mut new_vertices: Vec<Vertex> = Vec::new();

        // Process each vertex
        for (idx, vert) in self.vertices.iter().enumerate() {
            if is_on_plane(vert.pos) {
                // Center vertex - maps to itself, snap to plane
                vertex_map.insert(idx, idx);
                // Note: could also snap position to exact 0 on the axis here
            } else if is_positive_side(vert.pos) {
                // Positive side - create mirrored copy
                let mirrored_vert = Vertex {
                    pos: mirror_pos(vert.pos),
                    uv: vert.uv, // Keep same UV
                    normal: mirror_normal(vert.normal),
                    color: vert.color,
                    bone_index: vert.bone_index,
                };
                let new_idx = self.vertices.len() + new_vertices.len();
                new_vertices.push(mirrored_vert);
                vertex_map.insert(idx, new_idx);
            }
            // Negative side vertices are not processed (they'll be deleted or kept as-is)
        }

        // Add new mirrored vertices
        self.vertices.extend(new_vertices);

        // Create mirrored faces
        let mut new_faces: Vec<EditFace> = Vec::new();
        for face in &self.faces {
            // Check if this face is on the positive side (all vertices on positive or center)
            let all_positive_or_center = face.vertices.iter().all(|&vi| {
                self.vertices.get(vi)
                    .map(|v| is_positive_side(v.pos) || is_on_plane(v.pos))
                    .unwrap_or(false)
            });

            if all_positive_or_center {
                // Check if all vertices are just on center (no need to mirror)
                let all_center = face.vertices.iter().all(|&vi| {
                    self.vertices.get(vi)
                        .map(|v| is_on_plane(v.pos))
                        .unwrap_or(false)
                });

                if all_center {
                    // Face is entirely on the center plane - skip mirroring
                    continue;
                }

                // Create mirrored face with reversed winding
                let mirrored_verts: Vec<usize> = face.vertices.iter()
                    .filter_map(|&vi| vertex_map.get(&vi).copied())
                    .rev() // Reverse winding order
                    .collect();

                if mirrored_verts.len() == face.vertices.len() {
                    new_faces.push(EditFace {
                        vertices: mirrored_verts,
                        texture_id: face.texture_id,
                        black_transparent: face.black_transparent,
                        blend_mode: face.blend_mode,
                    });
                }
            }
        }

        // Add mirrored faces
        self.faces.extend(new_faces);
    }

    /// Get all faces that contain a given edge (pair of vertex indices).
    /// Returns face indices.
    fn faces_with_edge(&self, v0: usize, v1: usize) -> Vec<usize> {
        self.faces.iter().enumerate()
            .filter(|(_, face)| {
                let verts = &face.vertices;
                let n = verts.len();
                for i in 0..n {
                    let a = verts[i];
                    let b = verts[(i + 1) % n];
                    if (a == v0 && b == v1) || (a == v1 && b == v0) {
                        return true;
                    }
                }
                false
            })
            .map(|(i, _)| i)
            .collect()
    }

    /// Given an edge (v0, v1) and a quad face containing it, find the "opposite" edge.
    /// Returns None if the face is not a quad or doesn't contain the edge.
    fn opposite_edge_in_quad(&self, face_idx: usize, v0: usize, v1: usize) -> Option<(usize, usize)> {
        let face = &self.faces[face_idx];
        if face.vertices.len() != 4 {
            return None; // Only works for quads
        }

        let verts = &face.vertices;
        // Find the position of our edge
        for i in 0..4 {
            let a = verts[i];
            let b = verts[(i + 1) % 4];
            if (a == v0 && b == v1) || (a == v1 && b == v0) {
                // Found the edge, opposite edge is 2 positions away
                let c = verts[(i + 2) % 4];
                let d = verts[(i + 3) % 4];
                return Some((c, d));
            }
        }
        None
    }

    /// Select an edge loop starting from an edge (v0, v1).
    /// Returns all edges (vertex pairs) in the loop.
    /// Edge loops follow through connected edges that form a continuous strip.
    /// At each vertex, we continue to the edge "across" the quad (perpendicular traversal).
    pub fn select_edge_loop(&self, v0: usize, v1: usize) -> Vec<(usize, usize)> {
        let mut loop_edges = vec![(v0, v1)];
        let mut visited_edges: std::collections::HashSet<(usize, usize)> = std::collections::HashSet::new();

        // Normalize edge representation for visited set
        let normalize = |a: usize, b: usize| -> (usize, usize) {
            if a < b { (a, b) } else { (b, a) }
        };

        visited_edges.insert(normalize(v0, v1));

        // Extend in both directions from the starting edge
        // Direction 0: extend from v1
        // Direction 1: extend from v0
        for (start_v, end_v) in [(v0, v1), (v1, v0)] {
            let mut prev_v = start_v;
            let mut curr_v = end_v;

            loop {
                // Find the next vertex by looking at quads containing curr_v
                // We want a quad where prev_v-curr_v is an edge, and continue to the next edge
                let mut next_v: Option<usize> = None;

                for face in &self.faces {
                    if face.vertices.len() != 4 {
                        continue;
                    }

                    let verts = &face.vertices;
                    // Check if this quad contains edge prev_v-curr_v
                    let mut curr_pos: Option<usize> = None;
                    for i in 0..4 {
                        if verts[i] == curr_v {
                            let next_i = (i + 1) % 4;
                            let prev_i = (i + 3) % 4;
                            if verts[next_i] == prev_v || verts[prev_i] == prev_v {
                                curr_pos = Some(i);
                                break;
                            }
                        }
                    }

                    if let Some(pos) = curr_pos {
                        // Found the quad. curr_v is at position `pos`.
                        // The edge goes to prev_v which is either at pos+1 or pos-1.
                        // We want to continue to the vertex OPPOSITE to prev_v in the quad.
                        // In a quad [0,1,2,3], if curr_v is at pos 1 and prev_v is at pos 0,
                        // then the "continuation" vertex is at pos 2 (curr_v's other neighbor).
                        let neighbor1 = verts[(pos + 1) % 4];
                        let neighbor2 = verts[(pos + 3) % 4];

                        let next_candidate = if neighbor1 != prev_v {
                            neighbor1
                        } else {
                            neighbor2
                        };

                        // Check if this edge is already visited
                        let edge = normalize(curr_v, next_candidate);
                        if !visited_edges.contains(&edge) {
                            next_v = Some(next_candidate);
                            break;
                        }
                    }
                }

                match next_v {
                    Some(nv) => {
                        let edge = (curr_v, nv);
                        visited_edges.insert(normalize(curr_v, nv));
                        loop_edges.push(edge);
                        prev_v = curr_v;
                        curr_v = nv;
                    }
                    None => break,
                }
            }
        }

        loop_edges
    }

    /// Select a face loop starting from a face and an edge direction.
    /// Face loops are strips of faces connected through opposite edges.
    pub fn select_face_loop(&self, start_face: usize, edge_v0: usize, edge_v1: usize) -> Vec<usize> {
        let mut loop_faces = vec![start_face];
        let mut visited: std::collections::HashSet<usize> = std::collections::HashSet::new();
        visited.insert(start_face);

        eprintln!("[FACE LOOP] Starting face {}, edge ({}, {})", start_face, edge_v0, edge_v1);

        // Get the opposite edge for the second direction
        let opposite_start_edge = self.opposite_edge_in_quad(start_face, edge_v0, edge_v1);
        eprintln!("[FACE LOOP] Opposite start edge: {:?}", opposite_start_edge);

        // Traverse in both directions
        // Direction 0: use the original edge direction
        // Direction 1: use the opposite edge direction
        for direction in 0..2 {
            eprintln!("[FACE LOOP] === Direction {} ===", direction);

            let mut current_face = start_face;
            let mut current_edge = if direction == 0 {
                (edge_v0, edge_v1)
            } else {
                match opposite_start_edge {
                    Some(e) => e,
                    None => {
                        eprintln!("[FACE LOOP] No opposite edge for direction 1, skipping");
                        continue;
                    }
                }
            };

            loop {
                eprintln!("[FACE LOOP] Current face {}, edge ({}, {})", current_face, current_edge.0, current_edge.1);

                // Find the opposite edge in the current face
                let opposite = match self.opposite_edge_in_quad(current_face, current_edge.0, current_edge.1) {
                    Some(e) => e,
                    None => {
                        eprintln!("[FACE LOOP] Face {} is not a quad or edge not found, stopping", current_face);
                        break;
                    }
                };

                eprintln!("[FACE LOOP] Opposite edge: ({}, {})", opposite.0, opposite.1);

                // Find the face on the other side of this edge
                let adjacent_faces = self.faces_with_edge(opposite.0, opposite.1);
                eprintln!("[FACE LOOP] Adjacent faces to opposite edge: {:?}", adjacent_faces);

                let next_face = adjacent_faces.iter()
                    .find(|&&f| f != current_face && !visited.contains(&f));

                match next_face {
                    Some(&face_idx) => {
                        eprintln!("[FACE LOOP] Found next face: {}", face_idx);
                        visited.insert(face_idx);
                        loop_faces.push(face_idx);
                        current_face = face_idx;
                        current_edge = opposite;
                    }
                    None => {
                        eprintln!("[FACE LOOP] No unvisited adjacent face found, stopping");
                        break;
                    }
                }
            }
        }

        eprintln!("[FACE LOOP] Final result: {} faces", loop_faces.len());
        loop_faces
    }

    /// Get vertices from edge loop (flattens edge pairs to unique vertex indices)
    pub fn vertices_from_edge_loop(&self, edges: &[(usize, usize)]) -> Vec<usize> {
        let mut vertices: Vec<usize> = Vec::new();
        let mut seen: std::collections::HashSet<usize> = std::collections::HashSet::new();

        for &(v0, v1) in edges {
            if seen.insert(v0) {
                vertices.push(v0);
            }
            if seen.insert(v1) {
                vertices.push(v1);
            }
        }

        vertices
    }
}

impl Default for EditableMesh {
    fn default() -> Self {
        Self::new()
    }
}

/// Binary bone assignments (face index -> bone index)
/// PS1-style: each face is 100% assigned to one bone
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BoneAssignments {
    face_to_bone: Vec<Option<usize>>,
}

impl BoneAssignments {
    pub fn new() -> Self {
        Self {
            face_to_bone: Vec::new(),
        }
    }

    pub fn with_face_count(count: usize) -> Self {
        Self {
            face_to_bone: vec![None; count],
        }
    }

    /// Assign faces to a bone (binary weight)
    pub fn assign_faces(&mut self, face_indices: &[usize], bone_index: usize) {
        for &face_idx in face_indices {
            if face_idx < self.face_to_bone.len() {
                self.face_to_bone[face_idx] = Some(bone_index);
            }
        }
    }

    /// Unassign faces (remove bone assignment)
    pub fn unassign_faces(&mut self, face_indices: &[usize]) {
        for &face_idx in face_indices {
            if face_idx < self.face_to_bone.len() {
                self.face_to_bone[face_idx] = None;
            }
        }
    }

    /// Get bone assignment for a face
    pub fn get_bone_for_face(&self, face_idx: usize) -> Option<usize> {
        self.face_to_bone.get(face_idx).copied().flatten()
    }

    /// Get all faces assigned to a bone
    pub fn get_faces_for_bone(&self, bone_index: usize) -> Vec<usize> {
        self.face_to_bone
            .iter()
            .enumerate()
            .filter_map(|(i, b)| {
                if *b == Some(bone_index) {
                    Some(i)
                } else {
                    None
                }
            })
            .collect()
    }

    /// Get all unassigned faces
    pub fn unassigned_faces(&self) -> Vec<usize> {
        self.face_to_bone
            .iter()
            .enumerate()
            .filter_map(|(i, b)| if b.is_none() { Some(i) } else { None })
            .collect()
    }

    /// Get count of faces assigned to each bone
    pub fn bone_face_counts(&self, bone_count: usize) -> Vec<usize> {
        let mut counts = vec![0; bone_count];
        for bone_idx in self.face_to_bone.iter().flatten() {
            if *bone_idx < bone_count {
                counts[*bone_idx] += 1;
            }
        }
        counts
    }

    /// Resize assignments array when mesh changes
    pub fn resize(&mut self, new_face_count: usize) {
        self.face_to_bone.resize(new_face_count, None);
    }
}

impl Default for BoneAssignments {
    fn default() -> Self {
        Self::new()
    }
}

/// Error types for mesh editor operations
#[derive(Debug)]
pub enum MeshEditorError {
    Io(String),
    Serialization(String),
    Validation(String),
    InvalidBoneIndex(usize),
    InvalidFaceIndex(usize),
}

impl std::fmt::Display for MeshEditorError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            MeshEditorError::Io(e) => write!(f, "IO error: {}", e),
            MeshEditorError::Serialization(e) => write!(f, "Serialization error: {}", e),
            MeshEditorError::Validation(e) => write!(f, "Validation error: {}", e),
            MeshEditorError::InvalidBoneIndex(i) => write!(f, "Invalid bone index: {}", i),
            MeshEditorError::InvalidFaceIndex(i) => write!(f, "Invalid face index: {}", i),
        }
    }
}

impl std::error::Error for MeshEditorError {}
