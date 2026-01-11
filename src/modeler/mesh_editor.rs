//! Mesh editor for importing OBJ files and assigning faces to bones
//! PS1-style skeletal animation with binary bone weights
//!
//! Also includes PicoCAD-style mesh organization with named objects and texture atlas.
//!
//! Supports both compressed (brotli) and uncompressed RON files.
//! - Reading: Auto-detects format by checking for valid RON start
//! - Writing: Always uses brotli compression

use crate::rasterizer::{Vec3, IntVertex, IVec3, IVec2, INT_SCALE, Color as RasterColor, Color15, Texture15, BlendMode, ClutDepth, ClutId, Clut, IndexedTexture, fixed_sin, fixed_cos, TRIG_SCALE, TRIG_TABLE_SIZE};
use serde::{Deserialize, Serialize};
use std::path::Path;
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
    /// The texture atlas (serialized as raw RGBA) - kept for backwards compat
    pub atlas: TextureAtlas,

    // ---- PS1 CLUT System ----

    /// Optional indexed atlas (stores palette indices instead of colors)
    /// When present, this is the authoritative texture data
    #[serde(default)]
    pub indexed_atlas: Option<IndexedAtlas>,

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
        Self {
            name: name.into(),
            // Default cube: 1024 units = 1 meter (SECTOR_SIZE)
            objects: vec![MeshObject::cube("object", 1024.0)],
            atlas: TextureAtlas::new(128, 128),
            indexed_atlas: None,
            clut_pool: ClutPool::default(),
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
            blend_mode: crate::rasterizer::BlendMode::Opaque,
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
            blend_mode: crate::rasterizer::BlendMode::Opaque,
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
// Indexed Atlas (parallel to TextureAtlas, stores palette indices)
// ============================================================================

/// Indexed texture atlas storing palette indices instead of colors
///
/// Works alongside TextureAtlas - the RGBA atlas is kept for backwards
/// compatibility and preview, while this stores the actual indexed data.
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
    /// Vertices with integer coordinates (PS1-style quantization)
    pub vertices: Vec<IntVertex>,
    pub faces: Vec<EditFace>,
}

impl EditableMesh {
    pub fn new() -> Self {
        Self {
            vertices: Vec::new(),
            faces: Vec::new(),
        }
    }

    pub fn from_parts(vertices: Vec<IntVertex>, faces: Vec<EditFace>) -> Self {
        Self { vertices, faces }
    }

    /// Create a cube primitive centered at origin
    pub fn cube(size: f32) -> Self {
        let half_int = (size / 2.0 * INT_SCALE as f32).round() as i32;
        let scale = INT_SCALE as i32;

        let vertices = vec![
            // Front face (normal +Z)
            IntVertex { pos: IVec3::new(-half_int, -half_int,  half_int), uv: IVec2::new(0, 255), normal: IVec3::new(0, 0, scale), color: RasterColor::WHITE, bone_index: None },
            IntVertex { pos: IVec3::new( half_int, -half_int,  half_int), uv: IVec2::new(255, 255), normal: IVec3::new(0, 0, scale), color: RasterColor::WHITE, bone_index: None },
            IntVertex { pos: IVec3::new( half_int,  half_int,  half_int), uv: IVec2::new(255, 0), normal: IVec3::new(0, 0, scale), color: RasterColor::WHITE, bone_index: None },
            IntVertex { pos: IVec3::new(-half_int,  half_int,  half_int), uv: IVec2::new(0, 0), normal: IVec3::new(0, 0, scale), color: RasterColor::WHITE, bone_index: None },
            // Back face (normal -Z)
            IntVertex { pos: IVec3::new( half_int, -half_int, -half_int), uv: IVec2::new(0, 255), normal: IVec3::new(0, 0, -scale), color: RasterColor::WHITE, bone_index: None },
            IntVertex { pos: IVec3::new(-half_int, -half_int, -half_int), uv: IVec2::new(255, 255), normal: IVec3::new(0, 0, -scale), color: RasterColor::WHITE, bone_index: None },
            IntVertex { pos: IVec3::new(-half_int,  half_int, -half_int), uv: IVec2::new(255, 0), normal: IVec3::new(0, 0, -scale), color: RasterColor::WHITE, bone_index: None },
            IntVertex { pos: IVec3::new( half_int,  half_int, -half_int), uv: IVec2::new(0, 0), normal: IVec3::new(0, 0, -scale), color: RasterColor::WHITE, bone_index: None },
            // Top face (normal +Y)
            IntVertex { pos: IVec3::new(-half_int,  half_int,  half_int), uv: IVec2::new(0, 255), normal: IVec3::new(0, scale, 0), color: RasterColor::WHITE, bone_index: None },
            IntVertex { pos: IVec3::new( half_int,  half_int,  half_int), uv: IVec2::new(255, 255), normal: IVec3::new(0, scale, 0), color: RasterColor::WHITE, bone_index: None },
            IntVertex { pos: IVec3::new( half_int,  half_int, -half_int), uv: IVec2::new(255, 0), normal: IVec3::new(0, scale, 0), color: RasterColor::WHITE, bone_index: None },
            IntVertex { pos: IVec3::new(-half_int,  half_int, -half_int), uv: IVec2::new(0, 0), normal: IVec3::new(0, scale, 0), color: RasterColor::WHITE, bone_index: None },
            // Bottom face (normal -Y)
            IntVertex { pos: IVec3::new(-half_int, -half_int, -half_int), uv: IVec2::new(0, 255), normal: IVec3::new(0, -scale, 0), color: RasterColor::WHITE, bone_index: None },
            IntVertex { pos: IVec3::new( half_int, -half_int, -half_int), uv: IVec2::new(255, 255), normal: IVec3::new(0, -scale, 0), color: RasterColor::WHITE, bone_index: None },
            IntVertex { pos: IVec3::new( half_int, -half_int,  half_int), uv: IVec2::new(255, 0), normal: IVec3::new(0, -scale, 0), color: RasterColor::WHITE, bone_index: None },
            IntVertex { pos: IVec3::new(-half_int, -half_int,  half_int), uv: IVec2::new(0, 0), normal: IVec3::new(0, -scale, 0), color: RasterColor::WHITE, bone_index: None },
            // Right face (normal +X)
            IntVertex { pos: IVec3::new( half_int, -half_int,  half_int), uv: IVec2::new(0, 255), normal: IVec3::new(scale, 0, 0), color: RasterColor::WHITE, bone_index: None },
            IntVertex { pos: IVec3::new( half_int, -half_int, -half_int), uv: IVec2::new(255, 255), normal: IVec3::new(scale, 0, 0), color: RasterColor::WHITE, bone_index: None },
            IntVertex { pos: IVec3::new( half_int,  half_int, -half_int), uv: IVec2::new(255, 0), normal: IVec3::new(scale, 0, 0), color: RasterColor::WHITE, bone_index: None },
            IntVertex { pos: IVec3::new( half_int,  half_int,  half_int), uv: IVec2::new(0, 0), normal: IVec3::new(scale, 0, 0), color: RasterColor::WHITE, bone_index: None },
            // Left face (normal -X)
            IntVertex { pos: IVec3::new(-half_int, -half_int, -half_int), uv: IVec2::new(0, 255), normal: IVec3::new(-scale, 0, 0), color: RasterColor::WHITE, bone_index: None },
            IntVertex { pos: IVec3::new(-half_int, -half_int,  half_int), uv: IVec2::new(255, 255), normal: IVec3::new(-scale, 0, 0), color: RasterColor::WHITE, bone_index: None },
            IntVertex { pos: IVec3::new(-half_int,  half_int,  half_int), uv: IVec2::new(255, 0), normal: IVec3::new(-scale, 0, 0), color: RasterColor::WHITE, bone_index: None },
            IntVertex { pos: IVec3::new(-half_int,  half_int, -half_int), uv: IVec2::new(0, 0), normal: IVec3::new(-scale, 0, 0), color: RasterColor::WHITE, bone_index: None },
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
        let half_int = (size / 2.0 * INT_SCALE as f32).round() as i32;
        let scale = INT_SCALE as i32;

        let vertices = vec![
            IntVertex { pos: IVec3::new(-half_int, 0, -half_int), uv: IVec2::new(0, 0), normal: IVec3::new(0, scale, 0), color: RasterColor::WHITE, bone_index: None },
            IntVertex { pos: IVec3::new( half_int, 0, -half_int), uv: IVec2::new(255, 0), normal: IVec3::new(0, scale, 0), color: RasterColor::WHITE, bone_index: None },
            IntVertex { pos: IVec3::new( half_int, 0,  half_int), uv: IVec2::new(255, 255), normal: IVec3::new(0, scale, 0), color: RasterColor::WHITE, bone_index: None },
            IntVertex { pos: IVec3::new(-half_int, 0,  half_int), uv: IVec2::new(0, 255), normal: IVec3::new(0, scale, 0), color: RasterColor::WHITE, bone_index: None },
        ];

        // Single quad face (CCW winding when viewed from above)
        let faces = vec![EditFace::quad(0, 1, 2, 3)];

        Self { vertices, faces }
    }

    /// Create a triangular prism (wedge) primitive
    pub fn prism(size: f32, height: f32) -> Self {
        let half_int = (size / 2.0 * INT_SCALE as f32).round() as i32;
        let h_int = (height * INT_SCALE as f32).round() as i32;
        let scale = INT_SCALE as i32;

        // 6 vertices: 3 on bottom, 3 on top
        let vertices = vec![
            // Bottom triangle (Y=0)
            IntVertex { pos: IVec3::new(-half_int, 0, -half_int), uv: IVec2::new(0, 255), normal: IVec3::new(0, -scale, 0), color: RasterColor::WHITE, bone_index: None },
            IntVertex { pos: IVec3::new( half_int, 0, -half_int), uv: IVec2::new(255, 255), normal: IVec3::new(0, -scale, 0), color: RasterColor::WHITE, bone_index: None },
            IntVertex { pos: IVec3::new( 0,        0,  half_int), uv: IVec2::new(127, 0), normal: IVec3::new(0, -scale, 0), color: RasterColor::WHITE, bone_index: None },
            // Top triangle (Y=height)
            IntVertex { pos: IVec3::new(-half_int, h_int, -half_int), uv: IVec2::new(0, 255), normal: IVec3::new(0, scale, 0), color: RasterColor::WHITE, bone_index: None },
            IntVertex { pos: IVec3::new( half_int, h_int, -half_int), uv: IVec2::new(255, 255), normal: IVec3::new(0, scale, 0), color: RasterColor::WHITE, bone_index: None },
            IntVertex { pos: IVec3::new( 0,        h_int,  half_int), uv: IVec2::new(127, 0), normal: IVec3::new(0, scale, 0), color: RasterColor::WHITE, bone_index: None },
        ];

        // Faces (CCW winding when viewed from outside)
        let faces = vec![
            // Bottom and top triangles
            EditFace::tri(0, 2, 1),   // Bottom (CCW from below)
            EditFace::tri(3, 5, 4),   // Top (CCW from above)
            // Side faces are quads (CCW from outside)
            EditFace::quad(0, 1, 4, 3), // Back face
            EditFace::quad(1, 2, 5, 4), // Right face
            EditFace::quad(2, 0, 3, 5), // Left face
        ];

        Self { vertices, faces }
    }

    /// Create a cylinder primitive with given segments
    /// Uses fixed-point sin/cos lookup tables for PS1-authentic geometry
    pub fn cylinder(radius: f32, height: f32, segments: usize) -> Self {
        let segments = segments.max(3);
        let mut vertices = Vec::new();
        let mut faces = Vec::new();
        let scale = INT_SCALE as i32;
        let h_int = (height * INT_SCALE as f32).round() as i32;
        let radius_int = (radius * INT_SCALE as f32).round() as i64;

        // Ring vertices for caps
        let bottom_ring_start = vertices.len();
        for i in 0..segments {
            // Convert segment index to angle (0-4095 range)
            let angle_idx = ((i * TRIG_TABLE_SIZE) / segments) as u16;
            let cos_a = fixed_cos(angle_idx) as i64;
            let sin_a = fixed_sin(angle_idx) as i64;

            let x_int = ((cos_a * radius_int) / TRIG_SCALE as i64) as i32;
            let z_int = ((sin_a * radius_int) / TRIG_SCALE as i64) as i32;
            // UV: map cos/sin from [-4096,4096] to [0,255] via (val/4096 + 1) * 127.5
            let u = ((cos_a + TRIG_SCALE as i64) * 127 / TRIG_SCALE as i64).clamp(0, 255) as u8;
            let v = ((sin_a + TRIG_SCALE as i64) * 127 / TRIG_SCALE as i64).clamp(0, 255) as u8;

            // Bottom ring (for cap)
            vertices.push(IntVertex { pos: IVec3::new(x_int, 0, z_int), uv: IVec2::new(u, v), normal: IVec3::new(0, -scale, 0), color: RasterColor::WHITE, bone_index: None });
        }

        let top_ring_start = vertices.len();
        for i in 0..segments {
            let angle_idx = ((i * TRIG_TABLE_SIZE) / segments) as u16;
            let cos_a = fixed_cos(angle_idx) as i64;
            let sin_a = fixed_sin(angle_idx) as i64;

            let x_int = ((cos_a * radius_int) / TRIG_SCALE as i64) as i32;
            let z_int = ((sin_a * radius_int) / TRIG_SCALE as i64) as i32;
            let u = ((cos_a + TRIG_SCALE as i64) * 127 / TRIG_SCALE as i64).clamp(0, 255) as u8;
            let v = ((sin_a + TRIG_SCALE as i64) * 127 / TRIG_SCALE as i64).clamp(0, 255) as u8;

            // Top ring (for cap)
            vertices.push(IntVertex { pos: IVec3::new(x_int, h_int, z_int), uv: IVec2::new(u, v), normal: IVec3::new(0, scale, 0), color: RasterColor::WHITE, bone_index: None });
        }

        // Side vertices (need separate for proper normals)
        let side_bottom_start = vertices.len();
        for i in 0..segments {
            let angle_idx = ((i * TRIG_TABLE_SIZE) / segments) as u16;
            let cos_a = fixed_cos(angle_idx) as i64;
            let sin_a = fixed_sin(angle_idx) as i64;

            let x_int = ((cos_a * radius_int) / TRIG_SCALE as i64) as i32;
            let z_int = ((sin_a * radius_int) / TRIG_SCALE as i64) as i32;
            // Normal points outward (cos, 0, sin) scaled by INT_SCALE
            let normal_x = ((cos_a * INT_SCALE as i64) / TRIG_SCALE as i64) as i32;
            let normal_z = ((sin_a * INT_SCALE as i64) / TRIG_SCALE as i64) as i32;
            let u = ((i * 255) / segments) as u8;

            vertices.push(IntVertex { pos: IVec3::new(x_int, 0, z_int), uv: IVec2::new(u, 255), normal: IVec3::new(normal_x, 0, normal_z), color: RasterColor::WHITE, bone_index: None });
        }

        let side_top_start = vertices.len();
        for i in 0..segments {
            let angle_idx = ((i * TRIG_TABLE_SIZE) / segments) as u16;
            let cos_a = fixed_cos(angle_idx) as i64;
            let sin_a = fixed_sin(angle_idx) as i64;

            let x_int = ((cos_a * radius_int) / TRIG_SCALE as i64) as i32;
            let z_int = ((sin_a * radius_int) / TRIG_SCALE as i64) as i32;
            let normal_x = ((cos_a * INT_SCALE as i64) / TRIG_SCALE as i64) as i32;
            let normal_z = ((sin_a * INT_SCALE as i64) / TRIG_SCALE as i64) as i32;
            let u = ((i * 255) / segments) as u8;

            vertices.push(IntVertex { pos: IVec3::new(x_int, h_int, z_int), uv: IVec2::new(u, 0), normal: IVec3::new(normal_x, 0, normal_z), color: RasterColor::WHITE, bone_index: None });
        }

        // Bottom cap face (single n-gon, CCW from below)
        // Vertices go clockwise when viewed from above, so CCW from below
        let bottom_cap_verts: Vec<usize> = (0..segments).map(|i| bottom_ring_start + i).collect();
        faces.push(EditFace::ngon(&bottom_cap_verts));

        // Top cap face (single n-gon, CCW from above)
        // Vertices go counter-clockwise when viewed from above
        let top_cap_verts: Vec<usize> = (0..segments).rev().map(|i| top_ring_start + i).collect();
        faces.push(EditFace::ngon(&top_cap_verts));

        // Side faces (quads, CCW from outside)
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
        let half_int = (base_size / 2.0 * INT_SCALE as f32).round() as i32;
        let h_int = (height * INT_SCALE as f32).round() as i32;
        let scale = INT_SCALE as i32;

        // 5 vertices: 4 base corners + 1 apex
        let vertices = vec![
            // Base corners (Y=0)
            IntVertex { pos: IVec3::new(-half_int, 0, -half_int), uv: IVec2::new(0, 0), normal: IVec3::new(0, -scale, 0), color: RasterColor::WHITE, bone_index: None },
            IntVertex { pos: IVec3::new( half_int, 0, -half_int), uv: IVec2::new(255, 0), normal: IVec3::new(0, -scale, 0), color: RasterColor::WHITE, bone_index: None },
            IntVertex { pos: IVec3::new( half_int, 0,  half_int), uv: IVec2::new(255, 255), normal: IVec3::new(0, -scale, 0), color: RasterColor::WHITE, bone_index: None },
            IntVertex { pos: IVec3::new(-half_int, 0,  half_int), uv: IVec2::new(0, 255), normal: IVec3::new(0, -scale, 0), color: RasterColor::WHITE, bone_index: None },
            // Apex
            IntVertex { pos: IVec3::new(0, h_int, 0), uv: IVec2::new(127, 127), normal: IVec3::new(0, scale, 0), color: RasterColor::WHITE, bone_index: None },
        ];

        // Faces (CCW winding when viewed from outside)
        let faces = vec![
            // Base (quad, CCW from below)
            EditFace::quad(0, 1, 2, 3),
            // Side faces (triangles connecting to apex, CCW from outside)
            EditFace::tri(0, 4, 1), // Front (-Z side)
            EditFace::tri(1, 4, 2), // Right (+X side)
            EditFace::tri(2, 4, 3), // Back (+Z side)
            EditFace::tri(3, 4, 0), // Left (-X side)
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
    /// Uses fixed-point sin/cos lookup tables for PS1-authentic geometry
    pub fn ngon_prism(sides: usize, radius: f32, height: f32) -> Self {
        let sides = sides.max(3);
        let mut vertices = Vec::new();
        let mut faces = Vec::new();
        let scale = INT_SCALE as i32;
        let h_int = (height * INT_SCALE as f32).round() as i32;
        let radius_int = (radius * INT_SCALE as f32).round() as i64;

        // Bottom ring
        let bottom_start = vertices.len();
        for i in 0..sides {
            let angle_idx = ((i * TRIG_TABLE_SIZE) / sides) as u16;
            let cos_a = fixed_cos(angle_idx) as i64;
            let sin_a = fixed_sin(angle_idx) as i64;

            let x_int = ((cos_a * radius_int) / TRIG_SCALE as i64) as i32;
            let z_int = ((sin_a * radius_int) / TRIG_SCALE as i64) as i32;
            let u = ((cos_a + TRIG_SCALE as i64) * 127 / TRIG_SCALE as i64).clamp(0, 255) as u8;
            let v = ((sin_a + TRIG_SCALE as i64) * 127 / TRIG_SCALE as i64).clamp(0, 255) as u8;
            vertices.push(IntVertex { pos: IVec3::new(x_int, 0, z_int), uv: IVec2::new(u, v), normal: IVec3::new(0, -scale, 0), color: RasterColor::WHITE, bone_index: None });
        }

        // Top ring
        let top_start = vertices.len();
        for i in 0..sides {
            let angle_idx = ((i * TRIG_TABLE_SIZE) / sides) as u16;
            let cos_a = fixed_cos(angle_idx) as i64;
            let sin_a = fixed_sin(angle_idx) as i64;

            let x_int = ((cos_a * radius_int) / TRIG_SCALE as i64) as i32;
            let z_int = ((sin_a * radius_int) / TRIG_SCALE as i64) as i32;
            let u = ((cos_a + TRIG_SCALE as i64) * 127 / TRIG_SCALE as i64).clamp(0, 255) as u8;
            let v = ((sin_a + TRIG_SCALE as i64) * 127 / TRIG_SCALE as i64).clamp(0, 255) as u8;
            vertices.push(IntVertex { pos: IVec3::new(x_int, h_int, z_int), uv: IVec2::new(u, v), normal: IVec3::new(0, scale, 0), color: RasterColor::WHITE, bone_index: None });
        }

        // Bottom cap face (single n-gon, CCW from below)
        let bottom_cap_verts: Vec<usize> = (0..sides).map(|i| bottom_start + i).collect();
        faces.push(EditFace::ngon(&bottom_cap_verts));

        // Top cap face (single n-gon, CCW from above)
        let top_cap_verts: Vec<usize> = (0..sides).rev().map(|i| top_start + i).collect();
        faces.push(EditFace::ngon(&top_cap_verts));

        // Side faces (quads, CCW from outside)
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
    pub fn merge(&mut self, other: &EditableMesh, offset: IVec3) {
        let vertex_offset = self.vertices.len();

        // Add vertices with position offset (pure integer addition)
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

        let mut sum_x: i64 = 0;
        let mut sum_y: i64 = 0;
        let mut sum_z: i64 = 0;
        let mut count = 0;
        for &vi in &face.vertices {
            if let Some(v) = self.vertices.get(vi) {
                sum_x += v.pos.x as i64;
                sum_y += v.pos.y as i64;
                sum_z += v.pos.z as i64;
                count += 1;
            }
        }

        if count > 0 {
            // Convert from integer coordinates back to float for the result
            let avg_pos = IVec3::new(
                (sum_x / count as i64) as i32,
                (sum_y / count as i64) as i32,
                (sum_z / count as i64) as i32,
            );
            Some(avg_pos.to_render_f32())
        } else {
            None
        }
    }

    /// Compute face direction as unnormalized integer vector (pure integer math)
    /// Returns the raw cross product - useful for direction checks without sqrt
    /// For CW winding, this points outward from the face
    pub fn face_direction_int(&self, face_idx: usize) -> Option<IVec3> {
        let face = self.faces.get(face_idx)?;
        if face.vertices.len() < 3 {
            return Some(IVec3::new(0, INT_SCALE, 0)); // Default up for degenerate
        }

        let v0 = self.vertices.get(face.vertices[0])?.pos;
        let v1 = self.vertices.get(face.vertices[1])?.pos;
        let v2 = self.vertices.get(face.vertices[2])?.pos;

        // Edge vectors (integer)
        let e1 = v1 - v0;
        let e2 = v2 - v0;

        // Cross product: e2 x e1 for CW winding
        let normal = e2.cross(e1);

        // Return raw direction (unnormalized)
        if normal.length_squared() > 0 {
            Some(normal)
        } else {
            Some(IVec3::new(0, INT_SCALE, 0)) // Default up if degenerate
        }
    }

    /// Compute face normal for CW-wound faces (pointing outward)
    /// Uses first 3 vertices for normal calculation (works for n-gons)
    /// NOTE: Uses float for normalization (requires sqrt)
    pub fn face_normal(&self, face_idx: usize) -> Option<Vec3> {
        let face = self.faces.get(face_idx)?;
        if face.vertices.len() < 3 {
            return Some(Vec3::new(0.0, 1.0, 0.0)); // Default up for degenerate
        }

        // Convert integer positions to float for normal calculation
        let v0 = self.vertices.get(face.vertices[0])?.pos.to_render_f32();
        let v1 = self.vertices.get(face.vertices[1])?.pos.to_render_f32();
        let v2 = self.vertices.get(face.vertices[2])?.pos.to_render_f32();

        // Edge vectors
        let e1 = v1 - v0;
        let e2 = v2 - v0;

        // Cross product: e2 x e1 for CW winding (reversed from CCW convention)
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
    /// tolerance: maximum integer distance squared (e.g., 1 = exact match only)
    pub fn find_coincident_vertices_int(&self, idx: usize, tolerance_sq: i64) -> Vec<usize> {
        let Some(pos) = self.vertices.get(idx).map(|v| v.pos) else {
            return vec![];
        };

        self.vertices.iter().enumerate()
            .filter(|(_, v)| {
                let dx = (v.pos.x - pos.x) as i64;
                let dy = (v.pos.y - pos.y) as i64;
                let dz = (v.pos.z - pos.z) as i64;
                dx * dx + dy * dy + dz * dz <= tolerance_sq
            })
            .map(|(i, _)| i)
            .collect()
    }

    /// Expand a set of vertex indices to include all coincident vertices (integer version)
    /// tolerance: maximum integer distance (1 = exact match only)
    pub fn expand_to_coincident_int(&self, indices: &[usize], tolerance: i32) -> Vec<usize> {
        let tolerance_sq = (tolerance as i64) * (tolerance as i64);
        let mut result = std::collections::HashSet::new();
        for &idx in indices {
            for coincident in self.find_coincident_vertices_int(idx, tolerance_sq) {
                result.insert(coincident);
            }
        }
        result.into_iter().collect()
    }

    /// Extrude selected faces by a given amount (integer units) along their normals
    /// Returns the indices of the new top faces (for selection update)
    pub fn extrude_faces(&mut self, face_indices: &[usize], amount: i32) -> Vec<usize> {
        use std::collections::{HashMap, HashSet};

        if face_indices.is_empty() || amount == 0 {
            return face_indices.to_vec();
        }

        // Collect all unique vertices from selected faces
        let mut vertex_set: Vec<usize> = face_indices.iter()
            .filter_map(|&fi| self.faces.get(fi))
            .flat_map(|f| f.vertices.iter().cloned())
            .collect();
        vertex_set.sort();
        vertex_set.dedup();

        // Compute average normal for extrusion direction (use float for normalization)
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

        // Convert normal * amount to integer offset
        let offset = IVec3::new(
            (avg_normal.x * amount as f32).round() as i32,
            (avg_normal.y * amount as f32).round() as i32,
            (avg_normal.z * amount as f32).round() as i32,
        );

        // Create new vertices (copies of originals, offset by extrusion)
        let mut old_to_new: HashMap<usize, usize> = HashMap::new();
        for &vi in &vertex_set {
            if let Some(old_vert) = self.vertices.get(vi) {
                let new_vert = IntVertex {
                    pos: IVec3::new(
                        old_vert.pos.x + offset.x,
                        old_vert.pos.y + offset.y,
                        old_vert.pos.z + offset.z,
                    ),
                    uv: old_vert.uv,
                    normal: old_vert.normal,
                    color: old_vert.color,
                    bone_index: old_vert.bone_index,
                };
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
                // Get positions (as IVec3)
                let p0_old = self.vertices[v0_old].pos;
                let p1_old = self.vertices[v1_old].pos;
                let p0_new = self.vertices[v0_new].pos;
                let p1_new = self.vertices[v1_new].pos;

                // Compute the face normal from the actual quad geometry (use float for normalize)
                let e1 = (p1_new - p1_old).to_render_f32();
                let e2 = (p0_new - p1_old).to_render_f32();
                let side_normal_f32 = e2.cross(e1).normalize();
                // Convert back to integer normal hint (just store direction, normalized at render)
                let side_normal = IVec3::new(
                    (side_normal_f32.x * INT_SCALE as f32).round() as i32,
                    (side_normal_f32.y * INT_SCALE as f32).round() as i32,
                    (side_normal_f32.z * INT_SCALE as f32).round() as i32,
                );

                // Get UVs for side face (corners of UV space)
                let uv00 = IVec2::new(0, 0);
                let uv01 = IVec2::new(0, 255);
                let uv11 = IVec2::new(255, 255);
                let uv10 = IVec2::new(255, 0);

                // Create 4 vertices for the quad with the computed normal
                // Quad: v1_old -> v1_new -> v0_new -> v0_old (CW when viewed from outside)
                let sv0 = IntVertex { pos: p1_old, uv: uv00, normal: side_normal, color: crate::rasterizer::Color::WHITE, bone_index: None };
                let sv1 = IntVertex { pos: p1_new, uv: uv01, normal: side_normal, color: crate::rasterizer::Color::WHITE, bone_index: None };
                let sv2 = IntVertex { pos: p0_new, uv: uv11, normal: side_normal, color: crate::rasterizer::Color::WHITE, bone_index: None };
                let sv3 = IntVertex { pos: p0_old, uv: uv10, normal: side_normal, color: crate::rasterizer::Color::WHITE, bone_index: None };

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

        // Convert IntVertex to float Vertex for rendering
        let raster_vertices: Vec<RasterVertex> = self.vertices.iter()
            .map(|v| v.to_render_vertex())
            .collect();

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

        // Convert IntVertex to float Vertex for rendering
        let raster_vertices: Vec<RasterVertex> = self.vertices.iter()
            .map(|v| v.to_render_vertex())
            .collect();

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
