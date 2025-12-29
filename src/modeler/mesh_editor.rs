//! Mesh editor for importing OBJ files and assigning faces to bones
//! PS1-style skeletal animation with binary bone weights
//!
//! Also includes PicoCAD-style mesh organization with named objects and texture atlas.

use crate::rasterizer::{Vec3, Face, Vertex, Color as RasterColor, Color15, Texture15, BlendMode};
use serde::{Deserialize, Serialize};
use std::path::Path;

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
    /// Whether this object is visible in the viewport
    pub visible: bool,
    /// Whether this object is locked (can't be selected/edited)
    pub locked: bool,
    /// Color tint for identification in viewport (optional)
    pub color: Option<[u8; 3]>,
}

impl MeshObject {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            mesh: EditableMesh::new(),
            visible: true,
            locked: false,
            color: None,
        }
    }

    pub fn with_mesh(name: impl Into<String>, mesh: EditableMesh) -> Self {
        Self {
            name: name.into(),
            mesh,
            visible: true,
            locked: false,
            color: None,
        }
    }

    pub fn cube(name: impl Into<String>, size: f32) -> Self {
        Self::with_mesh(name, EditableMesh::cube(size))
    }
}

/// A complete PicoCAD-style project with multiple objects and texture atlas
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MeshProject {
    /// Project name
    pub name: String,
    /// All mesh objects in the project
    pub objects: Vec<MeshObject>,
    /// The texture atlas (serialized as raw RGBA)
    pub atlas: TextureAtlas,
    /// Currently selected object index
    #[serde(skip)]
    pub selected_object: Option<usize>,
}

impl MeshProject {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            objects: vec![MeshObject::cube("object", 50.0)],
            atlas: TextureAtlas::new(128, 128),
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

    /// Save project to file (.ron format)
    #[cfg(not(target_arch = "wasm32"))]
    pub fn save_to_file(&self, path: &Path) -> Result<(), MeshEditorError> {
        let config = ron::ser::PrettyConfig::new()
            .depth_limit(4)
            .indentor("  ".to_string());
        let ron_data = ron::ser::to_string_pretty(self, config)
            .map_err(|e| MeshEditorError::Serialization(e.to_string()))?;

        std::fs::write(path, ron_data)
            .map_err(|e| MeshEditorError::Io(e.to_string()))?;
        Ok(())
    }

    /// Load project from file (.ron format)
    #[cfg(not(target_arch = "wasm32"))]
    pub fn load_from_file(path: &Path) -> Result<Self, MeshEditorError> {
        let ron_data = std::fs::read_to_string(path)
            .map_err(|e| MeshEditorError::Io(e.to_string()))?;

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
// Texture Atlas (PicoCAD-style 128x128 pixel texture)
// ============================================================================

/// A small texture atlas for low-poly models (like picoCAD's 128x120)
#[derive(Clone, Debug)]
pub struct TextureAtlas {
    pub width: usize,
    pub height: usize,
    /// RGB pixel data + blend mode (stored as 4 bytes: R, G, B, blend_mode_u8)
    pub pixels: Vec<u8>,
}

impl TextureAtlas {
    pub fn new(width: usize, height: usize) -> Self {
        // Initialize with grey (like Blender's default material)
        let mut pixels = vec![0u8; width * height * 4];
        for i in 0..(width * height) {
            pixels[i * 4] = 128;     // R - grey
            pixels[i * 4 + 1] = 128; // G - grey
            pixels[i * 4 + 2] = 128; // B - grey
            pixels[i * 4 + 3] = 0;   // BlendMode::Opaque
        }
        Self { width, height, pixels }
    }

    /// Convert BlendMode to u8 for storage
    fn blend_to_u8(blend: crate::rasterizer::BlendMode) -> u8 {
        match blend {
            crate::rasterizer::BlendMode::Opaque => 0,
            crate::rasterizer::BlendMode::Average => 1,
            crate::rasterizer::BlendMode::Add => 2,
            crate::rasterizer::BlendMode::Subtract => 3,
            crate::rasterizer::BlendMode::AddQuarter => 4,
            crate::rasterizer::BlendMode::Erase => 5,
        }
    }

    /// Convert u8 to BlendMode
    fn u8_to_blend(v: u8) -> crate::rasterizer::BlendMode {
        match v {
            0 => crate::rasterizer::BlendMode::Opaque,
            1 => crate::rasterizer::BlendMode::Average,
            2 => crate::rasterizer::BlendMode::Add,
            3 => crate::rasterizer::BlendMode::Subtract,
            4 => crate::rasterizer::BlendMode::AddQuarter,
            _ => crate::rasterizer::BlendMode::Erase, // Default to transparent
        }
    }

    /// Set a pixel color at (x, y)
    pub fn set_pixel(&mut self, x: usize, y: usize, color: RasterColor) {
        if x < self.width && y < self.height {
            let idx = (y * self.width + x) * 4;
            self.pixels[idx] = color.r;
            self.pixels[idx + 1] = color.g;
            self.pixels[idx + 2] = color.b;
            self.pixels[idx + 3] = Self::blend_to_u8(color.blend);
        }
    }

    /// Get pixel color at (x, y)
    pub fn get_pixel(&self, x: usize, y: usize) -> RasterColor {
        if x < self.width && y < self.height {
            let idx = (y * self.width + x) * 4;
            RasterColor::with_blend(
                self.pixels[idx],
                self.pixels[idx + 1],
                self.pixels[idx + 2],
                Self::u8_to_blend(self.pixels[idx + 3]),
            )
        } else {
            RasterColor::TRANSPARENT
        }
    }

    /// Set a pixel with blend mode
    /// Note: This stores the color WITH the blend mode intact - blending happens at render time
    pub fn set_pixel_blended(&mut self, x: usize, y: usize, color: RasterColor, mode: crate::rasterizer::BlendMode) {
        if x >= self.width || y >= self.height { return; }
        // Store the color with the specified blend mode (don't blend now, blend at render time)
        let color_with_mode = RasterColor::with_blend(color.r, color.g, color.b, mode);
        self.set_pixel(x, y, color_with_mode);
    }

    /// Fill a rectangle with a color
    pub fn fill_rect(&mut self, x: usize, y: usize, w: usize, h: usize, color: RasterColor) {
        for py in y..(y + h).min(self.height) {
            for px in x..(x + w).min(self.width) {
                self.set_pixel(px, py, color);
            }
        }
    }

    /// Draw a line (Bresenham's algorithm)
    pub fn draw_line(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, color: RasterColor) {
        let dx = (x1 - x0).abs();
        let dy = -(y1 - y0).abs();
        let sx = if x0 < x1 { 1 } else { -1 };
        let sy = if y0 < y1 { 1 } else { -1 };
        let mut err = dx + dy;
        let mut x = x0;
        let mut y = y0;

        loop {
            if x >= 0 && y >= 0 {
                self.set_pixel(x as usize, y as usize, color);
            }
            if x == x1 && y == y1 { break; }
            let e2 = 2 * err;
            if e2 >= dy {
                err += dy;
                x += sx;
            }
            if e2 <= dx {
                err += dx;
                y += sy;
            }
        }
    }

    /// Clear to a solid color
    pub fn clear(&mut self, color: RasterColor) {
        for y in 0..self.height {
            for x in 0..self.width {
                self.set_pixel(x, y, color);
            }
        }
    }

    /// Flood fill starting from (x, y) with the given color
    /// Uses a simple stack-based algorithm to avoid recursion depth issues
    pub fn flood_fill(&mut self, x: usize, y: usize, fill_color: RasterColor) {
        if x >= self.width || y >= self.height {
            return;
        }

        let target_color = self.get_pixel(x, y);

        // Don't fill if the target is already the fill color (same RGB and blend mode)
        if target_color.r == fill_color.r
            && target_color.g == fill_color.g
            && target_color.b == fill_color.b
            && target_color.blend == fill_color.blend {
            return;
        }

        let mut stack = vec![(x, y)];

        while let Some((cx, cy)) = stack.pop() {
            if cx >= self.width || cy >= self.height {
                continue;
            }

            let current = self.get_pixel(cx, cy);

            // Check if this pixel matches the target color
            if current.r != target_color.r
                || current.g != target_color.g
                || current.b != target_color.b
                || current.blend != target_color.blend {
                continue;
            }

            // Fill this pixel
            self.set_pixel(cx, cy, fill_color);

            // Add neighbors to stack
            if cx > 0 { stack.push((cx - 1, cy)); }
            if cx + 1 < self.width { stack.push((cx + 1, cy)); }
            if cy > 0 { stack.push((cx, cy - 1)); }
            if cy + 1 < self.height { stack.push((cx, cy + 1)); }
        }
    }

    /// Convert to a rasterizer Texture for rendering
    pub fn to_raster_texture(&self) -> crate::rasterizer::Texture {
        let pixels: Vec<RasterColor> = (0..self.height)
            .flat_map(|y| (0..self.width).map(move |x| self.get_pixel(x, y)))
            .collect();

        crate::rasterizer::Texture {
            width: self.width,
            height: self.height,
            pixels,
            name: String::from("atlas"),
        }
    }

    /// Convert to a Texture15 (RGB555) for PS1-authentic rendering
    pub fn to_raster_texture_15(&self) -> Texture15 {
        let pixels: Vec<Color15> = (0..self.height)
            .flat_map(|y| {
                (0..self.width).map(move |x| {
                    let idx = (y * self.width + x) * 4;
                    let r = self.pixels[idx];
                    let g = self.pixels[idx + 1];
                    let b = self.pixels[idx + 2];
                    let blend_mode = Self::u8_to_blend(self.pixels[idx + 3]);

                    // Map to Color15:
                    // - BlendMode::Erase -> transparent (0x0000)
                    // - Non-Opaque -> semi-transparent bit set
                    if blend_mode == BlendMode::Erase {
                        Color15::TRANSPARENT
                    } else {
                        let semi = blend_mode != BlendMode::Opaque;
                        Color15::from_rgb888_semi(r, g, b, semi)
                    }
                })
            })
            .collect();

        Texture15 {
            width: self.width,
            height: self.height,
            pixels,
            name: String::from("atlas"),
        }
    }

    /// Resize the atlas to new dimensions, preserving existing content where possible
    /// Content at (x, y) is preserved if x < new_width and y < new_height
    /// New areas are filled with grey (default color)
    pub fn resize(&mut self, new_width: usize, new_height: usize) {
        if new_width == self.width && new_height == self.height {
            return;
        }

        let mut new_pixels = vec![0u8; new_width * new_height * 4];

        // Initialize new pixels with grey default
        for i in 0..(new_width * new_height) {
            new_pixels[i * 4] = 128;     // R - grey
            new_pixels[i * 4 + 1] = 128; // G - grey
            new_pixels[i * 4 + 2] = 128; // B - grey
            new_pixels[i * 4 + 3] = 0;   // BlendMode::Opaque
        }

        // Copy existing content that fits
        let copy_w = self.width.min(new_width);
        let copy_h = self.height.min(new_height);

        for y in 0..copy_h {
            for x in 0..copy_w {
                let old_idx = (y * self.width + x) * 4;
                let new_idx = (y * new_width + x) * 4;
                new_pixels[new_idx] = self.pixels[old_idx];
                new_pixels[new_idx + 1] = self.pixels[old_idx + 1];
                new_pixels[new_idx + 2] = self.pixels[old_idx + 2];
                new_pixels[new_idx + 3] = self.pixels[old_idx + 3];
            }
        }

        self.width = new_width;
        self.height = new_height;
        self.pixels = new_pixels;
    }
}

// Serialize TextureAtlas as base64-encoded PNG would be ideal, but for simplicity
// we'll serialize as raw dimensions + pixel data
impl Serialize for TextureAtlas {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut state = serializer.serialize_struct("TextureAtlas", 3)?;
        state.serialize_field("width", &self.width)?;
        state.serialize_field("height", &self.height)?;
        // Encode as base64 string for compactness
        use base64::{Engine as _, engine::general_purpose::STANDARD};
        let encoded = STANDARD.encode(&self.pixels);
        state.serialize_field("pixels", &encoded)?;
        state.end()
    }
}

impl<'de> Deserialize<'de> for TextureAtlas {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct AtlasData {
            width: usize,
            height: usize,
            pixels: String,
        }
        let data = AtlasData::deserialize(deserializer)?;
        use base64::{Engine as _, engine::general_purpose::STANDARD};
        let pixels = STANDARD.decode(&data.pixels)
            .map_err(serde::de::Error::custom)?;
        Ok(TextureAtlas {
            width: data.width,
            height: data.height,
            pixels,
        })
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

    /// Save mesh editor model to file (.ron format)
    pub fn save_to_file(&self, path: &Path) -> Result<(), MeshEditorError> {
        let config = ron::ser::PrettyConfig::new()
            .depth_limit(4)
            .indentor("  ".to_string());
        let ron_data = ron::ser::to_string_pretty(self, config)
            .map_err(|e| MeshEditorError::Serialization(e.to_string()))?;

        std::fs::write(path, ron_data)
            .map_err(|e| MeshEditorError::Io(e.to_string()))?;
        Ok(())
    }

    /// Load mesh editor model from file (.ron format)
    pub fn load_from_file(path: &Path) -> Result<Self, MeshEditorError> {
        let ron_data = std::fs::read_to_string(path)
            .map_err(|e| MeshEditorError::Io(e.to_string()))?;

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

/// Editable mesh (vertices and faces from OBJ import)
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EditableMesh {
    pub vertices: Vec<Vertex>,
    pub faces: Vec<Face>,
}

impl EditableMesh {
    pub fn new() -> Self {
        Self {
            vertices: Vec::new(),
            faces: Vec::new(),
        }
    }

    pub fn from_parts(vertices: Vec<Vertex>, faces: Vec<Face>) -> Self {
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

        // Note: Rasterizer expects CW winding (swap v1/v2 from CCW to CW)
        let faces = vec![
            // Front
            Face::new(0, 2, 1),
            Face::new(0, 3, 2),
            // Back
            Face::new(4, 6, 5),
            Face::new(4, 7, 6),
            // Top
            Face::new(8, 10, 9),
            Face::new(8, 11, 10),
            // Bottom
            Face::new(12, 14, 13),
            Face::new(12, 15, 14),
            // Right
            Face::new(16, 18, 17),
            Face::new(16, 19, 18),
            // Left
            Face::new(20, 22, 21),
            Face::new(20, 23, 22),
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

        // Note: Rasterizer expects CW winding (swap v1/v2 from CCW to CW)
        let faces = vec![
            Face::new(0, 2, 1),
            Face::new(0, 3, 2),
        ];

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

        // Faces (CW winding)
        let faces = vec![
            // Bottom (reversed for correct facing)
            Face::new(0, 1, 2),
            // Top
            Face::new(3, 5, 4),
            // Side 1 (back)
            Face::new(0, 4, 1),
            Face::new(0, 3, 4),
            // Side 2 (right)
            Face::new(1, 5, 2),
            Face::new(1, 4, 5),
            // Side 3 (left)
            Face::new(2, 3, 0),
            Face::new(2, 5, 3),
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

        // Bottom center vertex
        let bottom_center = vertices.len();
        vertices.push(Vertex::new(Vec3::new(0.0, 0.0, 0.0), Vec2::new(0.5, 0.5), Vec3::new(0.0, -1.0, 0.0)));

        // Top center vertex
        let top_center = vertices.len();
        vertices.push(Vertex::new(Vec3::new(0.0, height, 0.0), Vec2::new(0.5, 0.5), Vec3::new(0.0, 1.0, 0.0)));

        // Ring vertices
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

        // Bottom cap faces
        for i in 0..segments {
            let next = (i + 1) % segments;
            faces.push(Face::new(
                bottom_center,
                bottom_ring_start + next,
                bottom_ring_start + i,
            ));
        }

        // Top cap faces
        for i in 0..segments {
            let next = (i + 1) % segments;
            faces.push(Face::new(
                top_center,
                top_ring_start + i,
                top_ring_start + next,
            ));
        }

        // Side faces
        for i in 0..segments {
            let next = (i + 1) % segments;
            // Two triangles per quad
            faces.push(Face::new(
                side_bottom_start + i,
                side_top_start + next,
                side_bottom_start + next,
            ));
            faces.push(Face::new(
                side_bottom_start + i,
                side_top_start + i,
                side_top_start + next,
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

        // Faces (CW winding)
        let faces = vec![
            // Base (two triangles)
            Face::new(0, 2, 1),
            Face::new(0, 3, 2),
            // Front side
            Face::new(0, 1, 4),
            // Right side
            Face::new(1, 2, 4),
            // Back side
            Face::new(2, 3, 4),
            // Left side
            Face::new(3, 0, 4),
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

        // Bottom center
        let bottom_center = vertices.len();
        vertices.push(Vertex::new(Vec3::new(0.0, 0.0, 0.0), Vec2::new(0.5, 0.5), Vec3::new(0.0, -1.0, 0.0)));

        // Top center
        let top_center = vertices.len();
        vertices.push(Vertex::new(Vec3::new(0.0, height, 0.0), Vec2::new(0.5, 0.5), Vec3::new(0.0, 1.0, 0.0)));

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

        // Bottom cap
        for i in 0..sides {
            let next = (i + 1) % sides;
            faces.push(Face::new(bottom_center, bottom_start + next, bottom_start + i));
        }

        // Top cap
        for i in 0..sides {
            let next = (i + 1) % sides;
            faces.push(Face::new(top_center, top_start + i, top_start + next));
        }

        // Sides (need separate vertices for proper normals in real impl, but this works for low-poly)
        for i in 0..sides {
            let next = (i + 1) % sides;
            // Two triangles per side quad
            faces.push(Face::new(bottom_start + i, top_start + next, bottom_start + next));
            faces.push(Face::new(bottom_start + i, top_start + i, top_start + next));
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
            self.faces.push(Face::new(
                f.v0 + vertex_offset,
                f.v1 + vertex_offset,
                f.v2 + vertex_offset,
            ));
        }
    }

    pub fn vertex_count(&self) -> usize {
        self.vertices.len()
    }

    pub fn face_count(&self) -> usize {
        self.faces.len()
    }

    /// Get all vertex indices used by a face
    pub fn face_vertices(&self, face_idx: usize) -> Option<[usize; 3]> {
        self.faces.get(face_idx).map(|f| [f.v0, f.v1, f.v2])
    }

    /// Compute centroid of a face
    pub fn face_centroid(&self, face_idx: usize) -> Option<Vec3> {
        let face = self.faces.get(face_idx)?;
        let v0 = self.vertices.get(face.v0)?.pos;
        let v1 = self.vertices.get(face.v1)?.pos;
        let v2 = self.vertices.get(face.v2)?.pos;
        Some(Vec3::new(
            (v0.x + v1.x + v2.x) / 3.0,
            (v0.y + v1.y + v2.y) / 3.0,
            (v0.z + v1.z + v2.z) / 3.0,
        ))
    }

    /// Compute face normal for CW-wound faces (pointing outward)
    pub fn face_normal(&self, face_idx: usize) -> Option<Vec3> {
        let face = self.faces.get(face_idx)?;
        let v0 = self.vertices.get(face.v0)?.pos;
        let v1 = self.vertices.get(face.v1)?.pos;
        let v2 = self.vertices.get(face.v2)?.pos;

        // Edge vectors
        let e1 = v1 - v0;
        let e2 = v2 - v0;

        // Cross product: e2 × e1 for CW winding (reversed from CCW convention)
        // This gives outward-facing normal for CW-wound triangles
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
        use crate::rasterizer::Vec2;
        use std::collections::{HashMap, HashSet};

        if face_indices.is_empty() || amount.abs() < 0.001 {
            return face_indices.to_vec();
        }

        // Collect all unique vertices from selected faces
        let mut vertex_set: Vec<usize> = face_indices.iter()
            .filter_map(|&fi| self.face_vertices(fi))
            .flat_map(|[v0, v1, v2]| vec![v0, v1, v2])
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
            if let Some([v0, v1, v2]) = self.face_vertices(fi) {
                // Edges in winding order: v0->v1, v1->v2, v2->v0
                directed_edges.push((v0, v1));
                directed_edges.push((v1, v2));
                directed_edges.push((v2, v0));
            }
        }

        // Find boundary edges: edges where the reverse direction doesn't exist
        // (internal edges have both directions from adjacent faces)
        let edge_set: HashSet<(usize, usize)> = directed_edges.iter().cloned().collect();
        let boundary_edges: Vec<(usize, usize)> = directed_edges.iter()
            .filter(|(a, b)| !edge_set.contains(&(*b, *a)))
            .cloned()
            .collect();

        // Create side faces for each boundary edge
        // The edge (v0, v1) is in the winding order of the original face
        for (v0_old, v1_old) in boundary_edges {
            if let (Some(&v0_new), Some(&v1_new)) = (old_to_new.get(&v0_old), old_to_new.get(&v1_old)) {
                // Get positions
                let p0_old = self.vertices[v0_old].pos;
                let p1_old = self.vertices[v1_old].pos;
                let p0_new = self.vertices[v0_new].pos;
                let p1_new = self.vertices[v1_new].pos;

                // Build quad: v1_old -> v1_new -> v0_new -> v0_old (CW when viewed from outside)
                // This creates a quad where:
                // - Bottom edge: v1_old to v0_old (reverse of boundary edge direction)
                // - Top edge: v1_new to v0_new
                // - Left edge: v0_old to v0_new (extrusion direction)
                // - Right edge: v1_old to v1_new (extrusion direction)

                // Compute the face normal from the actual quad geometry
                // First triangle is: sv0(p1_old), sv1(p1_new), sv2(p0_new)
                // For CW winding, normal = e2.cross(e1) where:
                // e1 = sv1 - sv0 = p1_new - p1_old
                // e2 = sv2 - sv0 = p0_new - p1_old
                let e1 = p1_new - p1_old;
                let e2 = p0_new - p1_old;
                let side_normal = e2.cross(e1).normalize();

                // Get UVs for side face
                let uv00 = Vec2::new(0.0, 0.0);
                let uv01 = Vec2::new(0.0, 1.0);
                let uv11 = Vec2::new(1.0, 1.0);
                let uv10 = Vec2::new(1.0, 0.0);

                // Create 4 vertices for the quad with the computed normal
                let sv0 = Vertex::new(p1_old, uv00, side_normal);
                let sv1 = Vertex::new(p1_new, uv01, side_normal);
                let sv2 = Vertex::new(p0_new, uv11, side_normal);
                let sv3 = Vertex::new(p0_old, uv10, side_normal);

                let si0 = self.vertices.len();
                self.vertices.push(sv0);
                self.vertices.push(sv1);
                self.vertices.push(sv2);
                self.vertices.push(sv3);

                // Two triangles for the quad (CW winding for our rasterizer)
                // Quad is: sv0(v1_old), sv1(v1_new), sv2(v0_new), sv3(v0_old)
                // Triangle 1: sv0, sv1, sv2
                // Triangle 2: sv0, sv2, sv3
                self.faces.push(Face::new(si0, si0 + 1, si0 + 2));
                self.faces.push(Face::new(si0, si0 + 2, si0 + 3));
            }
        }

        // Update original faces to use new (extruded) vertices
        let mut new_top_faces = Vec::new();
        for &fi in face_indices {
            if let Some(face) = self.faces.get_mut(fi) {
                if let (Some(&nv0), Some(&nv1), Some(&nv2)) = (
                    old_to_new.get(&face.v0),
                    old_to_new.get(&face.v1),
                    old_to_new.get(&face.v2),
                ) {
                    face.v0 = nv0;
                    face.v1 = nv1;
                    face.v2 = nv2;
                    new_top_faces.push(fi);
                }
            }
        }

        new_top_faces
    }

    /// Save mesh to file (.ron format)
    #[cfg(not(target_arch = "wasm32"))]
    pub fn save_to_file(&self, path: &std::path::Path) -> Result<(), MeshEditorError> {
        let config = ron::ser::PrettyConfig::default();
        let ron_data = ron::ser::to_string_pretty(self, config)
            .map_err(|e| MeshEditorError::Serialization(e.to_string()))?;

        std::fs::write(path, ron_data)
            .map_err(|e| MeshEditorError::Io(e.to_string()))?;
        Ok(())
    }

    /// Load mesh from file (.ron format)
    #[cfg(not(target_arch = "wasm32"))]
    pub fn load_from_file(path: &std::path::Path) -> Result<Self, MeshEditorError> {
        let ron_data = std::fs::read_to_string(path)
            .map_err(|e| MeshEditorError::Io(e.to_string()))?;

        let mesh: EditableMesh = ron::from_str(&ron_data)
            .map_err(|e| MeshEditorError::Serialization(e.to_string()))?;
        Ok(mesh)
    }

    /// Convert to render data (vertices and faces for the rasterizer) - no texture
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

        let raster_faces: Vec<RasterFace> = self.faces.iter().map(|f| {
            RasterFace {
                v0: f.v0,
                v1: f.v1,
                v2: f.v2,
                texture_id: None,
                black_transparent: f.black_transparent,
            }
        }).collect();

        (raster_vertices, raster_faces)
    }

    /// Convert to render data with texture atlas (texture_id = 0 for all faces)
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

        let raster_faces: Vec<RasterFace> = self.faces.iter().map(|f| {
            RasterFace {
                v0: f.v0,
                v1: f.v1,
                v2: f.v2,
                texture_id: Some(0), // Use texture atlas (index 0)
                black_transparent: f.black_transparent,
            }
        }).collect();

        (raster_vertices, raster_faces)
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
