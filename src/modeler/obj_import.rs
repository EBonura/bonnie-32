//! OBJ file import for ps1-mesh-gen outputs
//! Supports basic OBJ format: vertices (v), texture coords (vt), normals (vn), faces (f)
//! Also includes MTL parsing and PNG texture loading with quantization.

use crate::rasterizer::{Vec2, Vec3, Vertex, ClutDepth, Clut};
use super::mesh_editor::{EditableMesh, IndexedAtlas, TextureAtlas, EditFace};
use std::path::Path;

/// OBJ file importer
pub struct ObjImporter;

impl ObjImporter {
    /// Load an OBJ file and convert to EditableMesh
    pub fn load_from_file(path: &Path) -> Result<EditableMesh, ObjError> {
        let contents = std::fs::read_to_string(path)
            .map_err(|e| ObjError::Io(format!("Failed to read file: {}", e)))?;

        Self::parse(&contents)
    }

    /// Parse OBJ file contents
    pub fn parse(contents: &str) -> Result<EditableMesh, ObjError> {
        let mut positions: Vec<Vec3> = Vec::new();
        let mut tex_coords: Vec<Vec2> = Vec::new();
        let mut normals: Vec<Vec3> = Vec::new();

        let mut vertices: Vec<Vertex> = Vec::new();
        let mut faces: Vec<EditFace> = Vec::new();

        // Track unique vertex combinations (pos_idx, tc_idx, norm_idx) -> vertex_idx
        let mut vertex_cache: std::collections::HashMap<(usize, usize, usize), usize> =
            std::collections::HashMap::new();

        for (line_num, line) in contents.lines().enumerate() {
            let line = line.trim();

            // Skip empty lines and comments
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.is_empty() {
                continue;
            }

            match parts[0] {
                "v" => {
                    // Vertex position: v x y z
                    if parts.len() < 4 {
                        return Err(ObjError::Parse(format!(
                            "Line {}: Invalid vertex position (expected 3 values)",
                            line_num + 1
                        )));
                    }
                    let x = Self::parse_float(parts[1], line_num)?;
                    let y = Self::parse_float(parts[2], line_num)?;
                    let z = Self::parse_float(parts[3], line_num)?;
                    positions.push(Vec3::new(x, y, z));
                }

                "vt" => {
                    // Texture coordinate: vt u v
                    if parts.len() < 3 {
                        return Err(ObjError::Parse(format!(
                            "Line {}: Invalid texture coordinate (expected 2 values)",
                            line_num + 1
                        )));
                    }
                    let u = Self::parse_float(parts[1], line_num)?;
                    let v = Self::parse_float(parts[2], line_num)?;
                    tex_coords.push(Vec2::new(u, v));
                }

                "vn" => {
                    // Normal: vn x y z
                    if parts.len() < 4 {
                        return Err(ObjError::Parse(format!(
                            "Line {}: Invalid normal (expected 3 values)",
                            line_num + 1
                        )));
                    }
                    let x = Self::parse_float(parts[1], line_num)?;
                    let y = Self::parse_float(parts[2], line_num)?;
                    let z = Self::parse_float(parts[3], line_num)?;
                    normals.push(Vec3::new(x, y, z));
                }

                "f" => {
                    // Face: f v1/vt1/vn1 v2/vt2/vn2 v3/vt3/vn3
                    if parts.len() < 4 {
                        return Err(ObjError::Parse(format!(
                            "Line {}: Face must have at least 3 vertices",
                            line_num + 1
                        )));
                    }

                    // Parse face vertices
                    let mut face_verts = Vec::new();
                    for i in 1..parts.len() {
                        let vertex_idx = Self::parse_face_vertex(
                            parts[i],
                            line_num,
                            &positions,
                            &tex_coords,
                            &normals,
                            &mut vertices,
                            &mut vertex_cache,
                        )?;
                        face_verts.push(vertex_idx);
                    }

                    // Triangulate if needed (OBJ can have quads or n-gons)
                    // Fan triangulation from first vertex
                    // Note: OBJ uses CCW winding, but our rasterizer expects CW, so we swap v1/v2
                    for i in 1..(face_verts.len() - 1) {
                        faces.push(EditFace::tri(
                            face_verts[0],
                            face_verts[i + 1],  // swapped to flip winding
                            face_verts[i],      // swapped to flip winding
                        ));
                    }
                }

                _ => {
                    // Ignore other OBJ commands (o, g, s, usemtl, etc.)
                }
            }
        }

        if vertices.is_empty() {
            return Err(ObjError::Parse("No vertices found in OBJ file".to_string()));
        }

        if faces.is_empty() {
            return Err(ObjError::Parse("No faces found in OBJ file".to_string()));
        }

        Ok(EditableMesh::from_parts(vertices, faces))
    }

    /// Parse a face vertex string like "1/2/3" or "1//3" or "1"
    fn parse_face_vertex(
        spec: &str,
        line_num: usize,
        positions: &[Vec3],
        tex_coords: &[Vec2],
        normals: &[Vec3],
        vertices: &mut Vec<Vertex>,
        vertex_cache: &mut std::collections::HashMap<(usize, usize, usize), usize>,
    ) -> Result<usize, ObjError> {
        let parts: Vec<&str> = spec.split('/').collect();

        // Parse position index (required)
        let pos_idx = if !parts[0].is_empty() {
            Self::parse_index(parts[0], positions.len(), line_num)?
        } else {
            return Err(ObjError::Parse(format!(
                "Line {}: Missing position index in face",
                line_num + 1
            )));
        };

        // Parse texture coordinate index (optional)
        let tc_idx = if parts.len() > 1 && !parts[1].is_empty() {
            Self::parse_index(parts[1], tex_coords.len(), line_num)?
        } else {
            usize::MAX // Sentinel for missing
        };

        // Parse normal index (optional)
        let norm_idx = if parts.len() > 2 && !parts[2].is_empty() {
            Self::parse_index(parts[2], normals.len(), line_num)?
        } else {
            usize::MAX // Sentinel for missing
        };

        // Check if we've already created a vertex with this combination
        let cache_key = (pos_idx, tc_idx, norm_idx);
        if let Some(&vertex_idx) = vertex_cache.get(&cache_key) {
            return Ok(vertex_idx);
        }

        // Create new vertex
        let pos = positions[pos_idx];
        let uv = if tc_idx != usize::MAX {
            tex_coords[tc_idx]
        } else {
            Vec2::new(0.0, 0.0) // Default UV
        };
        let normal = if norm_idx != usize::MAX {
            normals[norm_idx]
        } else {
            Vec3::ZERO // Default normal (will be computed if needed)
        };

        let vertex = Vertex::new(pos, uv, normal);
        let vertex_idx = vertices.len();
        vertices.push(vertex);
        vertex_cache.insert(cache_key, vertex_idx);

        Ok(vertex_idx)
    }

    /// Parse a float value
    fn parse_float(s: &str, line_num: usize) -> Result<f32, ObjError> {
        s.parse().map_err(|_| {
            ObjError::Parse(format!(
                "Line {}: Invalid float value '{}'",
                line_num + 1,
                s
            ))
        })
    }

    /// Parse an index (handles negative indices for relative indexing)
    fn parse_index(s: &str, count: usize, line_num: usize) -> Result<usize, ObjError> {
        let idx: i32 = s.parse().map_err(|_| {
            ObjError::Parse(format!(
                "Line {}: Invalid index '{}'",
                line_num + 1,
                s
            ))
        })?;

        let result = if idx > 0 {
            // Positive index (1-based)
            (idx - 1) as usize
        } else if idx < 0 {
            // Negative index (relative to current count)
            (count as i32 + idx) as usize
        } else {
            return Err(ObjError::Parse(format!(
                "Line {}: Index cannot be 0",
                line_num + 1
            )));
        };

        if result >= count {
            return Err(ObjError::Parse(format!(
                "Line {}: Index {} out of range (have {} elements)",
                line_num + 1,
                idx,
                count
            )));
        }

        Ok(result)
    }

    /// Look for associated texture file (PNG with same name as OBJ)
    pub fn find_texture_for_obj(obj_path: &Path) -> Option<std::path::PathBuf> {
        let png_path = obj_path.with_extension("png");
        if png_path.exists() {
            return Some(png_path);
        }
        None
    }

    /// Load a PNG texture and convert to TextureAtlas (scaled to fit)
    #[cfg(not(target_arch = "wasm32"))]
    pub fn load_png_to_atlas(png_path: &Path) -> Result<TextureAtlas, ObjError> {
        use image::GenericImageView;

        let img = image::open(png_path)
            .map_err(|e| ObjError::Io(format!("Failed to load PNG: {}", e)))?;

        let (width, height) = img.dimensions();

        // Determine target atlas size (must be power of 2, max 512)
        let dim = match width.max(height) {
            0..=64 => 64,
            65..=128 => 128,
            129..=256 => 256,
            _ => 512,
        };

        // Create atlas and copy pixels (resizing if needed)
        let mut atlas = TextureAtlas::new(dim, dim);
        let rgba = img.to_rgba8();

        // Simple nearest-neighbor scaling
        for y in 0..dim {
            for x in 0..dim {
                let src_x = (x * width as usize / dim).min(width as usize - 1);
                let src_y = (y * height as usize / dim).min(height as usize - 1);
                let pixel = rgba.get_pixel(src_x as u32, src_y as u32);

                // Convert to our Color format
                // Alpha 0 = transparent (BlendMode::Erase)
                let blend = if pixel[3] == 0 {
                    crate::rasterizer::BlendMode::Erase
                } else {
                    crate::rasterizer::BlendMode::Opaque
                };
                let color = crate::rasterizer::Color::with_blend(pixel[0], pixel[1], pixel[2], blend);
                atlas.set_pixel(x, y, color);
            }
        }

        Ok(atlas)
    }

    /// Load a PNG and auto-quantize with optimal CLUT depth (4-bit if ≤15 colors, 8-bit otherwise)
    #[cfg(not(target_arch = "wasm32"))]
    pub fn load_png_and_quantize_auto(
        png_path: &Path,
        name: &str,
    ) -> Result<(TextureAtlas, IndexedAtlas, Clut, usize), ObjError> {
        use image::GenericImageView;

        let img = image::open(png_path)
            .map_err(|e| ObjError::Io(format!("Failed to load PNG: {}", e)))?;

        let (width, height) = img.dimensions();

        // Determine target atlas size (power of 2, max 512)
        let dim = match width.max(height) {
            0..=64 => 64,
            65..=128 => 128,
            129..=256 => 256,
            _ => 512,
        };

        // Scale image to target size
        let rgba = img.to_rgba8();
        let mut pixels = Vec::with_capacity(dim * dim * 4);

        for y in 0..dim {
            for x in 0..dim {
                let src_x = (x * width as usize / dim).min(width as usize - 1);
                let src_y = (y * height as usize / dim).min(height as usize - 1);
                let pixel = rgba.get_pixel(src_x as u32, src_y as u32);
                pixels.push(pixel[0]);
                pixels.push(pixel[1]);
                pixels.push(pixel[2]);
                pixels.push(pixel[3]);
            }
        }

        // Count unique colors to determine optimal depth
        let unique_colors = super::quantize::count_unique_colors(&pixels);
        let depth = super::quantize::optimal_clut_depth(unique_colors);

        // Use quantize module to create indexed atlas + CLUT
        let result = super::quantize::quantize_image(&pixels, dim, dim, depth, name);

        // Also create a regular TextureAtlas for backwards compatibility
        let mut atlas = TextureAtlas::new(dim, dim);
        for y in 0..dim {
            for x in 0..dim {
                let idx = (y * dim + x) * 4;
                let blend = if pixels[idx + 3] == 0 {
                    crate::rasterizer::BlendMode::Erase
                } else {
                    crate::rasterizer::BlendMode::Opaque
                };
                let color = crate::rasterizer::Color::with_blend(
                    pixels[idx],
                    pixels[idx + 1],
                    pixels[idx + 2],
                    blend,
                );
                atlas.set_pixel(x, y, color);
            }
        }

        // Convert IndexedTexture to IndexedAtlas
        let indexed_atlas = IndexedAtlas {
            width: result.texture.width,
            height: result.texture.height,
            depth: result.texture.depth,
            indices: result.texture.indices,
            default_clut: crate::rasterizer::ClutId::NONE, // Will be set when added to pool
        };

        Ok((atlas, indexed_atlas, result.clut, unique_colors))
    }

    /// Load a PNG and quantize it to indexed format with CLUT
    #[cfg(not(target_arch = "wasm32"))]
    pub fn load_png_and_quantize(
        png_path: &Path,
        depth: ClutDepth,
        name: &str,
    ) -> Result<(TextureAtlas, IndexedAtlas, Clut), ObjError> {
        use image::GenericImageView;

        let img = image::open(png_path)
            .map_err(|e| ObjError::Io(format!("Failed to load PNG: {}", e)))?;

        let (width, height) = img.dimensions();

        // Determine target atlas size (power of 2, max 512)
        let dim = match width.max(height) {
            0..=64 => 64,
            65..=128 => 128,
            129..=256 => 256,
            _ => 512,
        };

        // Scale image to target size
        let rgba = img.to_rgba8();
        let mut pixels = Vec::with_capacity(dim * dim * 4);

        for y in 0..dim {
            for x in 0..dim {
                let src_x = (x * width as usize / dim).min(width as usize - 1);
                let src_y = (y * height as usize / dim).min(height as usize - 1);
                let pixel = rgba.get_pixel(src_x as u32, src_y as u32);
                pixels.push(pixel[0]);
                pixels.push(pixel[1]);
                pixels.push(pixel[2]);
                pixels.push(pixel[3]);
            }
        }

        // Use quantize module to create indexed atlas + CLUT
        let result = super::quantize::quantize_image(&pixels, dim, dim, depth, name);

        // Also create a regular TextureAtlas for backwards compatibility
        let mut atlas = TextureAtlas::new(dim, dim);
        for y in 0..dim {
            for x in 0..dim {
                let idx = (y * dim + x) * 4;
                let blend = if pixels[idx + 3] == 0 {
                    crate::rasterizer::BlendMode::Erase
                } else {
                    crate::rasterizer::BlendMode::Opaque
                };
                let color = crate::rasterizer::Color::with_blend(
                    pixels[idx],
                    pixels[idx + 1],
                    pixels[idx + 2],
                    blend,
                );
                atlas.set_pixel(x, y, color);
            }
        }

        // Convert IndexedTexture to IndexedAtlas
        let indexed_atlas = IndexedAtlas {
            width: result.texture.width,
            height: result.texture.height,
            depth: result.texture.depth,
            indices: result.texture.indices,
            default_clut: crate::rasterizer::ClutId::NONE, // Will be set when added to pool
        };

        Ok((atlas, indexed_atlas, result.clut))
    }

    /// Complete import: OBJ mesh + PNG texture with quantization
    #[cfg(not(target_arch = "wasm32"))]
    pub fn import_with_texture(
        obj_path: &Path,
        scale: f32,
        quantize_depth: Option<ClutDepth>,
    ) -> Result<ObjImportResult, ObjError> {
        // Load mesh
        let mut mesh = Self::load_from_file(obj_path)?;

        // Apply scale
        for vertex in &mut mesh.vertices {
            vertex.pos = vertex.pos * scale;
        }

        // Compute normals if missing
        Self::compute_face_normals(&mut mesh);

        // Find and load texture
        let texture_path = Self::find_texture_for_obj(obj_path);
        let texture_result = if let Some(ref tex_path) = texture_path {
            if let Some(depth) = quantize_depth {
                // Load and quantize with specified depth
                let name = obj_path.file_stem()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_else(|| "Imported".to_string());
                match Self::load_png_and_quantize(tex_path, depth, &name) {
                    Ok((atlas, indexed, clut)) => {
                        // Count colors for info (since we specified depth manually)
                        let color_count = super::quantize::count_atlas_colors(&atlas);
                        Some(TextureImportResult::Quantized { atlas, indexed, clut, color_count })
                    }
                    Err(_) => None,
                }
            } else {
                // Just load as regular atlas
                match Self::load_png_to_atlas(tex_path) {
                    Ok(atlas) => Some(TextureImportResult::Atlas(atlas)),
                    Err(_) => None,
                }
            }
        } else {
            None
        };

        Ok(ObjImportResult {
            mesh,
            texture: texture_result,
            texture_path,
        })
    }

    /// Complete import with auto-detected CLUT depth based on color count
    /// Uses 4-bit (16 colors) if image has ≤15 unique colors, 8-bit otherwise
    #[cfg(not(target_arch = "wasm32"))]
    pub fn import_with_auto_quantize(
        obj_path: &Path,
        scale: f32,
    ) -> Result<ObjImportResult, ObjError> {
        // Load mesh
        let mut mesh = Self::load_from_file(obj_path)?;

        // Apply scale
        for vertex in &mut mesh.vertices {
            vertex.pos = vertex.pos * scale;
        }

        // Compute normals if missing
        Self::compute_face_normals(&mut mesh);

        // Find and load texture with auto-detection
        let texture_path = Self::find_texture_for_obj(obj_path);
        let texture_result = if let Some(ref tex_path) = texture_path {
            let name = obj_path.file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "Imported".to_string());
            match Self::load_png_and_quantize_auto(tex_path, &name) {
                Ok((atlas, indexed, clut, color_count)) => {
                    Some(TextureImportResult::Quantized { atlas, indexed, clut, color_count })
                }
                Err(_) => None,
            }
        } else {
            None
        };

        Ok(ObjImportResult {
            mesh,
            texture: texture_result,
            texture_path,
        })
    }

    /// Compute face normals for meshes that don't have them
    pub fn compute_face_normals(mesh: &mut EditableMesh) {
        // First pass: collect normals for each face
        let face_normals: Vec<(Vec<usize>, Vec3)> = mesh.faces.iter()
            .filter(|face| face.vertices.len() >= 3)
            .map(|face| {
                let v0 = &mesh.vertices[face.vertices[0]];
                let v1 = &mesh.vertices[face.vertices[1]];
                let v2 = &mesh.vertices[face.vertices[2]];

                // Compute face normal via cross product
                let edge1 = Vec3::new(
                    v1.pos.x - v0.pos.x,
                    v1.pos.y - v0.pos.y,
                    v1.pos.z - v0.pos.z,
                );
                let edge2 = Vec3::new(
                    v2.pos.x - v0.pos.x,
                    v2.pos.y - v0.pos.y,
                    v2.pos.z - v0.pos.z,
                );

                let normal = edge1.cross(edge2).normalize();
                (face.vertices.clone(), normal)
            })
            .collect();

        // Second pass: apply normals to vertices that don't have one
        for (vertices, normal) in face_normals {
            for v_idx in vertices {
                let vertex = &mut mesh.vertices[v_idx];
                if vertex.normal.x == 0.0 && vertex.normal.y == 0.0 && vertex.normal.z == 0.0 {
                    vertex.normal = normal;
                }
            }
        }
    }
}

/// Result of texture import (either regular atlas or quantized)
#[derive(Debug, Clone)]
pub enum TextureImportResult {
    /// Regular RGBA atlas
    Atlas(TextureAtlas),
    /// Quantized indexed atlas with CLUT
    Quantized {
        atlas: TextureAtlas,
        indexed: IndexedAtlas,
        clut: Clut,
        /// Number of unique colors detected in the original image
        color_count: usize,
    },
}

/// Result of OBJ import with optional texture
#[derive(Debug)]
pub struct ObjImportResult {
    /// The imported mesh
    pub mesh: EditableMesh,
    /// Optional texture (regular or quantized)
    pub texture: Option<TextureImportResult>,
    /// Path to the texture file (for display/debugging)
    pub texture_path: Option<std::path::PathBuf>,
}

/// Error types for OBJ import
#[derive(Debug)]
pub enum ObjError {
    Io(String),
    Parse(String),
}

impl std::fmt::Display for ObjError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            ObjError::Io(e) => write!(f, "IO error: {}", e),
            ObjError::Parse(e) => write!(f, "Parse error: {}", e),
        }
    }
}

impl std::error::Error for ObjError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_triangle() {
        let obj = r#"
# Simple triangle
v 0.0 0.0 0.0
v 1.0 0.0 0.0
v 0.0 1.0 0.0
vn 0.0 0.0 1.0
f 1//1 2//1 3//1
"#;

        let mesh = ObjImporter::parse(obj).unwrap();
        assert_eq!(mesh.vertices.len(), 3);
        assert_eq!(mesh.faces.len(), 1);
    }

    #[test]
    fn test_parse_quad_triangulation() {
        let obj = r#"
v 0.0 0.0 0.0
v 1.0 0.0 0.0
v 1.0 1.0 0.0
v 0.0 1.0 0.0
f 1 2 3 4
"#;

        let mesh = ObjImporter::parse(obj).unwrap();
        assert_eq!(mesh.vertices.len(), 4);
        assert_eq!(mesh.faces.len(), 2); // Quad split into 2 triangles
    }

    #[test]
    fn test_parse_with_texture_coords() {
        let obj = r#"
v 0.0 0.0 0.0
v 1.0 0.0 0.0
v 0.0 1.0 0.0
vt 0.0 0.0
vt 1.0 0.0
vt 0.0 1.0
f 1/1 2/2 3/3
"#;

        let mesh = ObjImporter::parse(obj).unwrap();
        assert_eq!(mesh.vertices.len(), 3);
        assert_eq!(mesh.vertices[0].uv.x, 0.0);
        assert_eq!(mesh.vertices[1].uv.x, 1.0);
    }

    #[test]
    fn test_load_ps1_mesh_gen_files() {
        // Test with all OBJ files in assets/meshes
        let base_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("assets/meshes");

        let mut count = 0;
        for entry in std::fs::read_dir(&base_path).expect("meshes dir should exist") {
            let entry = entry.unwrap();
            let path = entry.path();

            if path.extension().map_or(false, |e| e == "obj") {
                let mesh = ObjImporter::load_from_file(&path).unwrap();
                let filename = path.file_name().unwrap().to_string_lossy();

                // ps1-mesh-gen outputs ~400-500 faces
                println!(
                    "{}: {} vertices, {} faces",
                    filename,
                    mesh.vertices.len(),
                    mesh.faces.len()
                );
                assert!(mesh.vertices.len() > 50, "Expected vertices in {}", filename);
                assert!(mesh.faces.len() > 50, "Expected faces in {}", filename);
                count += 1;
            }
        }

        assert!(count > 0, "Expected at least one OBJ file in test_meshes");
        println!("Loaded {} OBJ files successfully", count);
    }
}
