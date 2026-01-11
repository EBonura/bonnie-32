//! Mesh Browser
//!
//! Modal dialog for browsing and previewing OBJ mesh files.

use macroquad::prelude::*;
use crate::ui::{Rect, UiContext, draw_icon_centered, draw_scrollable_list, icon, icon_button, icon_button_active, ACCENT_COLOR, TEXT_COLOR};
use crate::rasterizer::{Framebuffer, Camera, Color as RasterColor, Vec3, RasterSettings, render_mesh, render_mesh_15, draw_floor_grid, ClutDepth};
use crate::world::SECTOR_SIZE;
use super::mesh_editor::{EditableMesh, TextureAtlas};
use super::obj_import::ObjImporter;
use std::path::PathBuf;

/// Info about a mesh file (and its associated texture)
#[derive(Debug, Clone)]
pub struct MeshInfo {
    /// Display name (file stem)
    pub name: String,
    /// Full path to the OBJ file
    pub path: PathBuf,
    /// Primary texture path (PNG with same name)
    pub texture_path: Option<PathBuf>,
    /// Additional texture paths (_tex0.png, _tex1.png, etc.)
    pub additional_textures: Vec<PathBuf>,
    /// Vertex count (from parsing)
    pub vertex_count: usize,
    /// Face count (from parsing)
    pub face_count: usize,
}

/// Discover mesh files in a directory
#[cfg(not(target_arch = "wasm32"))]
pub fn discover_meshes() -> Vec<MeshInfo> {
    let meshes_dir = PathBuf::from("assets/meshes");
    let mut meshes = Vec::new();

    // Helper to find associated textures for an OBJ file
    // Returns (primary_texture, additional_textures)
    fn find_textures(obj_path: &PathBuf) -> (Option<PathBuf>, Vec<PathBuf>) {
        let stem = match obj_path.file_stem() {
            Some(s) => s.to_string_lossy().to_string(),
            None => return (None, Vec::new()),
        };
        let parent = match obj_path.parent() {
            Some(p) => p,
            None => return (None, Vec::new()),
        };

        let mut primary = None;
        let mut additional = Vec::new();

        // Check for primary texture (same name.png)
        let png_path = parent.join(format!("{}.png", stem));
        if png_path.exists() {
            primary = Some(png_path);
        }

        // Check for additional textures (_tex0.png, _tex1.png, etc.)
        for i in 0..16 {
            let tex_path = parent.join(format!("{}_tex{}.png", stem, i));
            if tex_path.exists() {
                additional.push(tex_path);
            } else if i > 0 {
                // Stop at first gap after tex0
                break;
            }
        }

        (primary, additional)
    }

    // Helper to scan a directory for OBJ files
    fn scan_dir(dir: &PathBuf, meshes: &mut Vec<MeshInfo>) {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    // Recursively scan subdirectories
                    scan_dir(&path, meshes);
                } else if path.extension().map_or(false, |ext| ext == "obj") {
                    let name = path.file_stem()
                        .map(|s| s.to_string_lossy().to_string())
                        .unwrap_or_else(|| "unknown".to_string());

                    // Find associated textures
                    let (texture_path, additional_textures) = find_textures(&path);

                    // Parse to get vertex/face counts
                    let (vertex_count, face_count) = if let Ok(mesh) = ObjImporter::load_from_file(&path) {
                        (mesh.vertices.len(), mesh.faces.len())
                    } else {
                        (0, 0)
                    };

                    meshes.push(MeshInfo { name, path, texture_path, additional_textures, vertex_count, face_count });
                }
            }
        }
    }

    scan_dir(&meshes_dir, &mut meshes);

    // Sort by name
    meshes.sort_by(|a, b| a.name.cmp(&b.name));
    meshes
}

#[cfg(target_arch = "wasm32")]
pub fn discover_meshes() -> Vec<MeshInfo> {
    // WASM: return empty, load async from manifest
    Vec::new()
}

/// Load mesh list from manifest asynchronously (for WASM)
pub async fn load_mesh_list() -> Vec<MeshInfo> {
    use macroquad::prelude::*;

    // Load and parse manifest
    let manifest = match load_string("assets/meshes/manifest.txt").await {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Failed to load meshes manifest: {}", e);
            return Vec::new();
        }
    };

    let mut meshes = Vec::new();

    for line in manifest.lines() {
        let line = line.trim();
        if line.is_empty() || !line.ends_with(".obj") {
            continue;
        }

        let name = line
            .strip_suffix(".obj")
            .unwrap_or(line)
            .to_string();
        let path = PathBuf::from(format!("assets/meshes/{}", line));

        // We don't have vertex/face counts until we load the mesh
        // For WASM, texture path is discovered at load time
        meshes.push(MeshInfo { name, path, texture_path: None, additional_textures: Vec::new(), vertex_count: 0, face_count: 0 });
    }

    meshes
}

/// Load a specific mesh by path (for WASM async loading)
pub async fn load_mesh(path: &PathBuf) -> Option<EditableMesh> {
    use macroquad::prelude::*;

    let path_str = path.to_string_lossy().replace('\\', "/");
    eprintln!("[mesh_browser] Loading mesh from: {}", path_str);

    match load_string(&path_str).await {
        Ok(contents) => {
            eprintln!("[mesh_browser] Loaded {} bytes, parsing OBJ...", contents.len());
            match ObjImporter::parse(&contents) {
                Ok(mesh) => {
                    eprintln!("[mesh_browser] Parsed: {} vertices, {} faces", mesh.vertices.len(), mesh.faces.len());
                    Some(mesh)
                }
                Err(e) => {
                    eprintln!("[mesh_browser] OBJ parse error: {}", e);
                    None
                }
            }
        }
        Err(e) => {
            eprintln!("[mesh_browser] Failed to load mesh file {}: {}", path_str, e);
            None
        }
    }
}

/// State for the mesh browser dialog
pub struct MeshBrowser {
    /// Whether the browser is open
    pub open: bool,
    /// List of available meshes
    pub meshes: Vec<MeshInfo>,
    /// Currently selected index
    pub selected_index: Option<usize>,
    /// Currently loaded preview mesh
    pub preview_mesh: Option<EditableMesh>,
    /// Orbit camera state for preview
    pub orbit_yaw: f32,
    pub orbit_pitch: f32,
    pub orbit_distance: f32,
    pub orbit_center: Vec3,
    /// Mouse state for orbit control
    pub dragging: bool,
    pub last_mouse: (f32, f32),
    /// Scroll offset for the list
    pub scroll_offset: f32,
    /// Scale multiplier for imported meshes (OBJ meshes are often small)
    pub import_scale: f32,
    /// Whether to flip normals on import (for meshes with inverted winding)
    pub flip_normals: bool,
    /// Whether to flip mesh horizontally (mirror X)
    pub flip_horizontal: bool,
    /// Whether to flip mesh vertically (mirror Y)
    pub flip_vertical: bool,
    /// Path pending async load (WASM)
    pub pending_load_path: Option<PathBuf>,
    /// Whether we need to async load the mesh list (WASM)
    pub pending_load_list: bool,
    /// Preview texture atlases (primary + additional _texN.png files)
    pub preview_atlases: Vec<TextureAtlas>,
    /// Whether to show texture in preview (if available)
    pub show_texture: bool,
    /// Scroll offset for texture preview panel
    pub texture_scroll_offset: f32,
    /// CLUT depth override for import (None = auto-detect based on color count)
    pub clut_depth_override: Option<ClutDepth>,
}

impl Default for MeshBrowser {
    fn default() -> Self {
        Self {
            open: false,
            meshes: Vec::new(),
            selected_index: None,
            preview_mesh: None,
            orbit_yaw: 0.5,
            orbit_pitch: 0.3,
            // Scale: 1024 units = 1 meter
            orbit_distance: 4096.0, // 4 meters back
            orbit_center: Vec3::new(0.0, 1024.0, 0.0), // 1 meter height
            dragging: false,
            last_mouse: (0.0, 0.0),
            scroll_offset: 0.0,
            // OBJ meshes are typically ~1 unit = 1 meter in source
            // Scale to 1024 units = 1 meter (our scale)
            import_scale: 1024.0,
            flip_normals: false,
            flip_horizontal: false,
            flip_vertical: false,
            pending_load_path: None,
            pending_load_list: false,
            preview_atlases: Vec::new(),
            show_texture: true, // Show textures by default
            texture_scroll_offset: 0.0,
            clut_depth_override: None, // Auto-detect by default
        }
    }
}

impl MeshBrowser {
    /// Open the browser with the given list of meshes
    pub fn open(&mut self, meshes: Vec<MeshInfo>) {
        self.open = true;
        self.meshes = meshes;
        self.selected_index = None;
        self.preview_mesh = None;
        self.preview_atlases.clear();
        self.scroll_offset = 0.0;
        self.texture_scroll_offset = 0.0;
    }

    /// Close the browser
    pub fn close(&mut self) {
        self.open = false;
        self.preview_mesh = None;
        self.preview_atlases.clear();
    }

    /// Set the preview mesh (resets camera view)
    pub fn set_preview(&mut self, mesh: EditableMesh) {
        self.update_preview_camera(&mesh);
        self.preview_mesh = Some(mesh);
        // Reset view angle on initial load
        self.orbit_yaw = 0.8;
        self.orbit_pitch = 0.3;
    }

    /// Update preview mesh without resetting camera angles (for scale/flip changes)
    pub fn update_preview(&mut self, mesh: EditableMesh) {
        self.update_preview_camera(&mesh);
        self.preview_mesh = Some(mesh);
        // Keep current orbit_yaw and orbit_pitch
    }

    /// Update camera center and distance based on mesh bounds
    fn update_preview_camera(&mut self, mesh: &EditableMesh) {
        // Calculate bounding box to center camera (integer positions)
        let mut min_x = i32::MAX;
        let mut min_y = i32::MAX;
        let mut min_z = i32::MAX;
        let mut max_x = i32::MIN;
        let mut max_y = i32::MIN;
        let mut max_z = i32::MIN;

        for vertex in &mesh.vertices {
            min_x = min_x.min(vertex.pos.x);
            min_y = min_y.min(vertex.pos.y);
            min_z = min_z.min(vertex.pos.z);
            max_x = max_x.max(vertex.pos.x);
            max_y = max_y.max(vertex.pos.y);
            max_z = max_z.max(vertex.pos.z);
        }

        // Calculate center (convert to float for camera)
        if min_x != i32::MAX {
            let scale = crate::rasterizer::INT_SCALE as f32;
            self.orbit_center = Vec3::new(
                (min_x + max_x) as f32 / 2.0 / scale,
                (min_y + max_y) as f32 / 2.0 / scale,
                (min_z + max_z) as f32 / 2.0 / scale,
            );

            // Set distance based on mesh size (convert to float)
            let size_x = (max_x - min_x) as f32 / scale;
            let size_y = (max_y - min_y) as f32 / scale;
            let size_z = (max_z - min_z) as f32 / scale;
            let diagonal = (size_x * size_x + size_y * size_y + size_z * size_z).sqrt();
            // Min distance 512 (128 float units), scale by 2x model size
            self.orbit_distance = diagonal.max(512.0) * 2.0;
        } else {
            // Fallback: 1024 units = 1 meter
            self.orbit_center = Vec3::new(0.0, 1024.0, 0.0);
            self.orbit_distance = 4096.0;
        }
    }

    /// Get the currently selected mesh info
    pub fn selected_mesh(&self) -> Option<&MeshInfo> {
        self.selected_index.and_then(|i| self.meshes.get(i))
    }

    /// Set the preview texture atlases (primary first, then additional)
    pub fn set_preview_atlases(&mut self, atlases: Vec<TextureAtlas>) {
        self.preview_atlases = atlases;
        self.texture_scroll_offset = 0.0;
    }

    /// Get the primary preview atlas (for 3D rendering)
    pub fn preview_atlas(&self) -> Option<&TextureAtlas> {
        self.preview_atlases.first()
    }
}

/// Result from drawing the mesh browser
#[derive(Debug, Clone, PartialEq)]
pub enum MeshBrowserAction {
    None,
    /// User selected a mesh to preview
    SelectPreview(usize),
    /// User wants to open the selected mesh
    OpenMesh,
    /// User cancelled
    Cancel,
    /// Preview settings changed (scale or flip), reload preview
    ReloadPreview,
}

/// Draw the mesh browser modal dialog
pub fn draw_mesh_browser(
    ctx: &mut UiContext,
    browser: &mut MeshBrowser,
    icon_font: Option<&Font>,
    fb: &mut Framebuffer,
) -> MeshBrowserAction {
    if !browser.open {
        return MeshBrowserAction::None;
    }

    let mut action = MeshBrowserAction::None;

    // Darken background
    draw_rectangle(0.0, 0.0, screen_width(), screen_height(), Color::from_rgba(0, 0, 0, 180));

    // Dialog dimensions (centered, ~80% of screen)
    let dialog_w = (screen_width() * 0.8).min(900.0);
    let dialog_h = (screen_height() * 0.8).min(600.0);
    let dialog_x = (screen_width() - dialog_w) / 2.0;
    let dialog_y = (screen_height() - dialog_h) / 2.0;

    // Draw dialog background
    draw_rectangle(dialog_x, dialog_y, dialog_w, dialog_h, Color::from_rgba(35, 35, 40, 255));
    draw_rectangle_lines(dialog_x, dialog_y, dialog_w, dialog_h, 2.0, Color::from_rgba(60, 60, 70, 255));

    // Header
    let header_h = 40.0;
    draw_rectangle(dialog_x, dialog_y, dialog_w, header_h, Color::from_rgba(45, 45, 55, 255));
    draw_text("Browse Meshes", dialog_x + 16.0, dialog_y + 26.0, 20.0, WHITE);

    // Close button
    let close_rect = Rect::new(dialog_x + dialog_w - 36.0, dialog_y + 4.0, 32.0, 32.0);
    if draw_close_button(ctx, close_rect, icon_font) {
        action = MeshBrowserAction::Cancel;
    }

    // Content area
    let content_y = dialog_y + header_h + 8.0;
    let content_h = dialog_h - header_h - 60.0;
    let list_w = 200.0;

    // List panel (left)
    let list_rect = Rect::new(dialog_x + 8.0, content_y, list_w, content_h);
    let item_h = 28.0;

    // Build display names with texture indicator
    let items: Vec<String> = browser.meshes.iter().map(|m| {
        let tex_count = (if m.texture_path.is_some() { 1 } else { 0 }) + m.additional_textures.len();
        if tex_count > 1 {
            format!("{} [{}]", m.name, tex_count) // Show texture count for multi-texture
        } else if tex_count == 1 {
            format!("{} \u{e002}", m.name) // Image icon for single texture
        } else {
            m.name.clone()
        }
    }).collect();

    let list_result = draw_scrollable_list(
        ctx,
        list_rect,
        &items,
        browser.selected_index,
        &mut browser.scroll_offset,
        item_h,
        None,
    );

    if let Some(clicked_idx) = list_result.clicked {
        if browser.selected_index != Some(clicked_idx) {
            browser.selected_index = Some(clicked_idx);
            action = MeshBrowserAction::SelectPreview(clicked_idx);
        }
    }

    // Preview panels - split into 3D view and texture preview
    let tex_panel_w = 200.0; // Wider panel for better texture visibility
    let preview_x = dialog_x + list_w + 16.0;
    let preview_w = dialog_w - list_w - tex_panel_w - 32.0;
    let preview_rect = Rect::new(preview_x, content_y, preview_w, content_h);

    draw_rectangle(preview_rect.x, preview_rect.y, preview_rect.w, preview_rect.h, Color::from_rgba(20, 20, 25, 255));

    let has_preview = browser.preview_mesh.is_some();
    let has_selection = browser.selected_index.is_some();

    if has_preview {
        draw_orbit_preview(ctx, browser, preview_rect, fb);

        // Draw stats at bottom of preview (two lines: counts + bounding box)
        if let Some(idx) = browser.selected_index {
            if let Some(info) = browser.meshes.get(idx) {
                let stats_h = 38.0; // Two lines of text
                let stats_y = preview_rect.bottom() - stats_h;
                draw_rectangle(preview_rect.x, stats_y, preview_rect.w, stats_h, Color::from_rgba(30, 30, 35, 200));

                // Line 1: Vertex and face counts
                let stats_text = format!(
                    "Vertices: {}  Faces: {}",
                    info.vertex_count, info.face_count
                );
                draw_text(&stats_text, preview_rect.x + 8.0, stats_y + 14.0, 12.0, Color::from_rgba(180, 180, 180, 255));

                // Line 2: Bounding box dimensions (computed from preview mesh)
                if let Some(mesh) = &browser.preview_mesh {
                    let (min, max) = compute_mesh_bounds(mesh);
                    let size_x = max.x - min.x;
                    let size_y = max.y - min.y;
                    let size_z = max.z - min.z;
                    let bbox_text = format!(
                        "Size: {:.0} x {:.0} x {:.0} units",
                        size_x, size_y, size_z
                    );
                    draw_text(&bbox_text, preview_rect.x + 8.0, stats_y + 30.0, 12.0, Color::from_rgba(140, 180, 140, 255));
                }
            }
        }
    } else if has_selection {
        draw_text("Loading preview...", preview_rect.x + 20.0, preview_rect.y + 40.0, 16.0, Color::from_rgba(150, 150, 150, 255));
    } else if browser.meshes.is_empty() {
        draw_text("No meshes found in assets/meshes/", preview_rect.x + 20.0, preview_rect.y + 40.0, 16.0, Color::from_rgba(100, 100, 100, 255));
        draw_text("Add OBJ files to that folder!", preview_rect.x + 20.0, preview_rect.y + 60.0, 14.0, Color::from_rgba(80, 80, 80, 255));
    } else {
        draw_text("Select a mesh to preview", preview_rect.x + 20.0, preview_rect.y + 40.0, 16.0, Color::from_rgba(100, 100, 100, 255));
    }

    // Texture preview panel (right side)
    let tex_panel_x = preview_rect.right() + 8.0;
    let tex_panel_rect = Rect::new(tex_panel_x, content_y, tex_panel_w, content_h);
    draw_rectangle(tex_panel_rect.x, tex_panel_rect.y, tex_panel_rect.w, tex_panel_rect.h, Color::from_rgba(25, 25, 30, 255));
    draw_rectangle_lines(tex_panel_rect.x, tex_panel_rect.y, tex_panel_rect.w, tex_panel_rect.h, 1.0, Color::from_rgba(50, 50, 55, 255));

    // Draw texture label with count
    let tex_count = browser.preview_atlases.len();
    let tex_label = if tex_count > 1 {
        format!("Textures ({})", tex_count)
    } else {
        "Texture".to_string()
    };
    draw_text(&tex_label, tex_panel_rect.x + 8.0, tex_panel_rect.y + 16.0, 12.0, Color::from_rgba(120, 120, 120, 255));

    // Draw textures or "No texture" message
    if !browser.preview_atlases.is_empty() {
        // Content area for textures (below label)
        let tex_content_y = tex_panel_rect.y + 24.0;
        let tex_content_h = content_h - 32.0;
        let tex_content_rect = Rect::new(tex_panel_rect.x + 4.0, tex_content_y, tex_panel_w - 8.0, tex_content_h);

        // Calculate layout: each texture gets a square thumbnail + label
        let thumb_size = tex_content_rect.w - 8.0; // Use full width minus padding
        let item_h = thumb_size + 24.0; // Thumbnail + label space
        let total_height = item_h * tex_count as f32;

        // Handle scrolling if content is taller than panel
        if ctx.mouse.inside(&tex_content_rect) {
            let scroll = ctx.mouse.scroll;
            if scroll != 0.0 {
                let max_scroll = (total_height - tex_content_h).max(0.0);
                browser.texture_scroll_offset = (browser.texture_scroll_offset - scroll * 8.0).clamp(0.0, max_scroll);
            }
        }

        // Enable scissor clipping for texture panel (prevents overflow)
        let dpi = screen_dpi_scale();
        gl_use_default_material();
        unsafe {
            get_internal_gl().quad_gl.scissor(
                Some((
                    (tex_content_rect.x * dpi) as i32,
                    (tex_content_rect.y * dpi) as i32,
                    (tex_content_rect.w * dpi) as i32,
                    (tex_content_h * dpi) as i32,
                ))
            );
        }

        // Draw each texture
        for (i, atlas) in browser.preview_atlases.iter().enumerate() {
            let item_y = tex_content_y + i as f32 * item_h - browser.texture_scroll_offset;

            // Skip if completely off-screen (optimization only, scissor handles actual clipping)
            if item_y + item_h < tex_content_y - 50.0 || item_y > tex_content_y + tex_content_h + 50.0 {
                continue;
            }

            // Convert atlas to RGBA for display
            let atlas_w = atlas.width;
            let atlas_h = atlas.height;
            let mut rgba = vec![0u8; atlas_w * atlas_h * 4];
            for y in 0..atlas_h {
                for x in 0..atlas_w {
                    let pixel = atlas.get_pixel(x, y);
                    let idx = (y * atlas_w + x) * 4;
                    rgba[idx] = pixel.r;
                    rgba[idx + 1] = pixel.g;
                    rgba[idx + 2] = pixel.b;
                    rgba[idx + 3] = 255;
                }
            }

            // Create texture
            let tex = Texture2D::from_rgba8(atlas_w as u16, atlas_h as u16, &rgba);
            tex.set_filter(FilterMode::Nearest);

            // Scale to fit thumbnail area (preserve aspect ratio)
            let scale = (thumb_size / atlas_w as f32).min(thumb_size / atlas_h as f32);
            let scaled_w = atlas_w as f32 * scale;
            let scaled_h = atlas_h as f32 * scale;
            let tex_x = tex_content_rect.x + (tex_content_rect.w - scaled_w) * 0.5;
            let tex_y = item_y + 2.0;

            draw_texture_ex(
                &tex,
                tex_x,
                tex_y,
                WHITE,
                DrawTextureParams {
                    dest_size: Some(vec2(scaled_w, scaled_h)),
                    ..Default::default()
                },
            );

            // Draw border
            draw_rectangle_lines(tex_x, tex_y, scaled_w, scaled_h, 1.0, Color::from_rgba(70, 70, 75, 255));

            // Draw label
            let label_y = item_y + thumb_size + 14.0;
            let label = if tex_count > 1 {
                format!("tex{} ({}x{})", i, atlas_w, atlas_h)
            } else {
                format!("{}x{}", atlas_w, atlas_h)
            };
            draw_text(&label, tex_content_rect.x, label_y, 10.0, Color::from_rgba(100, 100, 100, 255));
        }

        // Disable scissor clipping
        unsafe {
            get_internal_gl().quad_gl.scissor(None);
        }
    } else {
        // No texture message
        let msg_y = tex_panel_rect.y + tex_panel_rect.h * 0.5;
        draw_text("No texture", tex_panel_rect.x + 30.0, msg_y - 6.0, 12.0, Color::from_rgba(80, 80, 80, 255));
        draw_text("available", tex_panel_rect.x + 35.0, msg_y + 8.0, 12.0, Color::from_rgba(80, 80, 80, 255));
    }

    // Footer with buttons
    let footer_y = dialog_y + dialog_h - 44.0;
    draw_rectangle(dialog_x, footer_y, dialog_w, 44.0, Color::from_rgba(40, 40, 48, 255));

    // Scale control on the left side of footer
    draw_text("Scale:", dialog_x + 12.0, footer_y + 22.0, 14.0, TEXT_COLOR);
    let scale_minus_rect = Rect::new(dialog_x + 60.0, footer_y + 8.0, 28.0, 28.0);
    let scale_plus_rect = Rect::new(dialog_x + 150.0, footer_y + 8.0, 28.0, 28.0);

    if icon_button(ctx, scale_minus_rect, icon::MINUS, icon_font, "Decrease Scale (halve)") {
        // Allow scaling down to 0.001 for very large source models
        browser.import_scale = (browser.import_scale / 2.0).max(0.001);
        if browser.preview_mesh.is_some() {
            action = MeshBrowserAction::ReloadPreview;
        }
    }

    // Display current scale (use appropriate format based on value)
    let scale_text = if browser.import_scale >= 1.0 {
        format!("{:.0}", browser.import_scale)
    } else if browser.import_scale >= 0.01 {
        format!("{:.2}", browser.import_scale)
    } else {
        format!("{:.3}", browser.import_scale)
    };
    let text_width = measure_text(&scale_text, None, 14, 1.0).width;
    draw_text(&scale_text, dialog_x + 104.0 - text_width / 2.0, footer_y + 22.0, 14.0, TEXT_COLOR);

    if icon_button(ctx, scale_plus_rect, icon::PLUS, icon_font, "Increase Scale (double)") {
        // Allow scaling up to 1,000,000 for very small source models
        browser.import_scale = (browser.import_scale * 2.0).min(1_000_000.0);
        if browser.preview_mesh.is_some() {
            action = MeshBrowserAction::ReloadPreview;
        }
    }

    // Flip normals toggle
    let flip_rect = Rect::new(dialog_x + 190.0, footer_y + 8.0, 28.0, 28.0);
    if icon_button_active(ctx, flip_rect, icon::FLIP_VERTICAL, icon_font, "Flip Normals", browser.flip_normals) {
        browser.flip_normals = !browser.flip_normals;
        if browser.preview_mesh.is_some() {
            action = MeshBrowserAction::ReloadPreview;
        }
    }

    // Texture toggle (only enabled if texture is available)
    let tex_rect = Rect::new(dialog_x + 230.0, footer_y + 8.0, 28.0, 28.0);
    let has_texture = !browser.preview_atlases.is_empty();
    let tex_icon = if browser.show_texture { icon::EYE } else { icon::EYE_OFF };
    let tex_tooltip = if has_texture {
        if browser.show_texture { "Hide Texture" } else { "Show Texture" }
    } else {
        "No Texture Available"
    };
    if has_texture {
        if icon_button_active(ctx, tex_rect, tex_icon, icon_font, tex_tooltip, browser.show_texture) {
            browser.show_texture = !browser.show_texture;
        }
    } else {
        // Draw disabled button
        draw_rectangle(tex_rect.x, tex_rect.y, tex_rect.w, tex_rect.h, Color::from_rgba(35, 35, 40, 255));
        draw_icon_centered(icon_font, icon::EYE_OFF, &tex_rect, 14.0, Color::from_rgba(60, 60, 60, 255));
    }

    // Flip horizontal (mirror X axis) - toggle state and reload
    let flip_h_rect = Rect::new(dialog_x + 270.0, footer_y + 8.0, 28.0, 28.0);
    if icon_button_active(ctx, flip_h_rect, icon::FLIP_HORIZONTAL, icon_font, "Flip Horizontal (mirror X)", browser.flip_horizontal) {
        browser.flip_horizontal = !browser.flip_horizontal;
        if browser.preview_mesh.is_some() {
            action = MeshBrowserAction::ReloadPreview;
        }
    }

    // Flip vertical (mirror Y axis) - toggle state and reload
    let flip_v_rect = Rect::new(dialog_x + 300.0, footer_y + 8.0, 28.0, 28.0);
    if icon_button_active(ctx, flip_v_rect, icon::FLIP_VERTICAL, icon_font, "Flip Vertical (mirror Y)", browser.flip_vertical) {
        browser.flip_vertical = !browser.flip_vertical;
        if browser.preview_mesh.is_some() {
            action = MeshBrowserAction::ReloadPreview;
        }
    }

    // CLUT depth selector (Auto / 4-bit / 8-bit)
    let clut_label_x = dialog_x + 340.0;
    draw_text("CLUT:", clut_label_x, footer_y + 22.0, 12.0, TEXT_COLOR);

    let clut_btn_w = 36.0;
    let clut_btn_h = 20.0;
    let clut_btn_y = footer_y + 12.0;

    // Auto button
    let auto_rect = Rect::new(clut_label_x + 40.0, clut_btn_y, clut_btn_w, clut_btn_h);
    let auto_selected = browser.clut_depth_override.is_none();
    let auto_bg = if auto_selected { ACCENT_COLOR } else { Color::from_rgba(60, 60, 70, 255) };
    draw_rectangle(auto_rect.x, auto_rect.y, auto_rect.w, auto_rect.h, auto_bg);
    draw_text("Auto", auto_rect.x + 4.0, auto_rect.y + 14.0, 11.0, if auto_selected { WHITE } else { TEXT_COLOR });
    if ctx.mouse.inside(&auto_rect) {
        ctx.set_tooltip("Auto-detect CLUT depth based on color count", ctx.mouse.x, ctx.mouse.y);
        if ctx.mouse.left_pressed {
            browser.clut_depth_override = None;
        }
    }

    // 4-bit button
    let bpp4_rect = Rect::new(auto_rect.x + clut_btn_w + 2.0, clut_btn_y, clut_btn_w, clut_btn_h);
    let bpp4_selected = browser.clut_depth_override == Some(ClutDepth::Bpp4);
    let bpp4_bg = if bpp4_selected { ACCENT_COLOR } else { Color::from_rgba(60, 60, 70, 255) };
    draw_rectangle(bpp4_rect.x, bpp4_rect.y, bpp4_rect.w, bpp4_rect.h, bpp4_bg);
    draw_text("4-bit", bpp4_rect.x + 4.0, bpp4_rect.y + 14.0, 11.0, if bpp4_selected { WHITE } else { TEXT_COLOR });
    if ctx.mouse.inside(&bpp4_rect) {
        ctx.set_tooltip("Force 4-bit CLUT (16 colors) - reduces dithering artifacts", ctx.mouse.x, ctx.mouse.y);
        if ctx.mouse.left_pressed {
            browser.clut_depth_override = Some(ClutDepth::Bpp4);
        }
    }

    // 8-bit button
    let bpp8_rect = Rect::new(bpp4_rect.x + clut_btn_w + 2.0, clut_btn_y, clut_btn_w, clut_btn_h);
    let bpp8_selected = browser.clut_depth_override == Some(ClutDepth::Bpp8);
    let bpp8_bg = if bpp8_selected { ACCENT_COLOR } else { Color::from_rgba(60, 60, 70, 255) };
    draw_rectangle(bpp8_rect.x, bpp8_rect.y, bpp8_rect.w, bpp8_rect.h, bpp8_bg);
    draw_text("8-bit", bpp8_rect.x + 4.0, bpp8_rect.y + 14.0, 11.0, if bpp8_selected { WHITE } else { TEXT_COLOR });
    if ctx.mouse.inside(&bpp8_rect) {
        ctx.set_tooltip("Force 8-bit CLUT (256 colors) - preserves more detail", ctx.mouse.x, ctx.mouse.y);
        if ctx.mouse.left_pressed {
            browser.clut_depth_override = Some(ClutDepth::Bpp8);
        }
    }

    // Cancel button
    let cancel_rect = Rect::new(dialog_x + dialog_w - 180.0, footer_y + 8.0, 80.0, 28.0);
    if draw_text_button(ctx, cancel_rect, "Cancel", Color::from_rgba(60, 60, 70, 255)) {
        action = MeshBrowserAction::Cancel;
    }

    // Open button
    let open_rect = Rect::new(dialog_x + dialog_w - 90.0, footer_y + 8.0, 80.0, 28.0);
    let open_enabled = browser.preview_mesh.is_some();
    if draw_text_button_enabled(ctx, open_rect, "Open", ACCENT_COLOR, open_enabled) {
        action = MeshBrowserAction::OpenMesh;
    }

    // Handle Escape to close
    if is_key_pressed(KeyCode::Escape) {
        action = MeshBrowserAction::Cancel;
    }

    action
}

/// Draw the orbit preview of a mesh
fn draw_orbit_preview(
    ctx: &mut UiContext,
    browser: &mut MeshBrowser,
    rect: Rect,
    fb: &mut Framebuffer,
) {
    let mesh = match &browser.preview_mesh {
        Some(m) => m,
        None => return,
    };

    // Handle mouse drag for orbit
    if ctx.mouse.inside(&rect) {
        if ctx.mouse.left_down {
            if browser.dragging {
                let dx = ctx.mouse.x - browser.last_mouse.0;
                let dy = ctx.mouse.y - browser.last_mouse.1;
                browser.orbit_yaw += dx * 0.01;
                browser.orbit_pitch = (browser.orbit_pitch + dy * 0.01).clamp(-1.4, 1.4);
            }
            browser.dragging = true;
            browser.last_mouse = (ctx.mouse.x, ctx.mouse.y);
        } else {
            browser.dragging = false;
        }

        // Scroll to zoom (use ctx.mouse.scroll to respect modal blocking)
        // Minimum 0.3 to stay safely outside the 0.1 near plane
        // Use 2% steps (0.02) for fine control - 10% was too sensitive
        // Max 5000 to handle large scaled meshes (100x scale = ~200 unit meshes)
        let scroll = ctx.mouse.scroll;
        if scroll != 0.0 {
            browser.orbit_distance = (browser.orbit_distance * (1.0 - scroll * 0.02)).clamp(0.3, 5000.0);
        }
    } else {
        browser.dragging = false;
    }

    // Calculate camera position from orbit
    let cos_pitch = browser.orbit_pitch.cos();
    let sin_pitch = browser.orbit_pitch.sin();
    let cos_yaw = browser.orbit_yaw.cos();
    let sin_yaw = browser.orbit_yaw.sin();

    let offset_x = browser.orbit_distance * cos_pitch * sin_yaw;
    let offset_y = browser.orbit_distance * sin_pitch;
    let offset_z = browser.orbit_distance * cos_pitch * cos_yaw;

    let cam_pos = browser.orbit_center + Vec3::new(offset_x, offset_y, offset_z);

    // Create camera
    let mut camera = Camera::new();
    // Convert position to IVec3 (INT_SCALE = 4)
    camera.position = crate::rasterizer::IVec3::new(
        (cam_pos.x * crate::world::INT_SCALE as f32) as i32,
        (cam_pos.y * crate::world::INT_SCALE as f32) as i32,
        (cam_pos.z * crate::world::INT_SCALE as f32) as i32,
    );

    // Calculate rotation from direction
    let dir = browser.orbit_center - cam_pos;
    let len = dir.len();
    let n = dir * (1.0 / len);
    camera.rotation_x = crate::rasterizer::radians_to_bam((-n.y).asin());
    camera.rotation_y = crate::rasterizer::radians_to_bam(n.x.atan2(n.z));
    camera.update_basis();

    // Resize framebuffer
    let preview_h = rect.h - 24.0;
    let target_w = (rect.w as usize).min(640);
    let target_h = (preview_h as usize).min(target_w * 3 / 4);
    fb.resize(target_w, target_h);
    fb.clear(RasterColor::new(25, 25, 35));

    // Render the mesh
    let settings = RasterSettings::default();

    if !mesh.vertices.is_empty() {
        // Check if we should render with texture (use primary atlas for preview)
        if browser.show_texture && browser.preview_atlas().is_some() {
            let atlas = browser.preview_atlas().unwrap();
            let (vertices, faces) = mesh.to_render_data_textured();

            if settings.use_rgb555 {
                let atlas_texture_15 = atlas.to_raster_texture_15();
                let textures_15 = [atlas_texture_15];
                render_mesh_15(fb, &vertices, &faces, &textures_15, None, &camera, &settings, None);
            } else {
                let atlas_texture = atlas.to_raster_texture();
                let textures = [atlas_texture];
                render_mesh(fb, &vertices, &faces, &textures, &camera, &settings);
            }
        } else {
            // Render without texture (triangulate n-gon faces)
            let (vertices, faces) = mesh.to_render_data();
            if settings.use_rgb555 {
                render_mesh_15(fb, &vertices, &faces, &[], None, &camera, &settings, None);
            } else {
                render_mesh(fb, &vertices, &faces, &[], &camera, &settings);
            }
        }
    }

    // Draw a simple floor plane indicator
    draw_preview_grid(fb, &camera);

    // Draw framebuffer to screen
    let fb_texture = Texture2D::from_rgba8(fb.width as u16, fb.height as u16, &fb.pixels);
    fb_texture.set_filter(FilterMode::Nearest);

    draw_texture_ex(
        &fb_texture,
        rect.x,
        rect.y,
        WHITE,
        DrawTextureParams {
            dest_size: Some(vec2(rect.w, preview_h)),
            ..Default::default()
        },
    );
}

/// Compute the bounding box of a mesh, returning (min, max) corners in float coords
fn compute_mesh_bounds(mesh: &EditableMesh) -> (Vec3, Vec3) {
    // Use integer min/max since positions are IVec3
    let mut min_x = i32::MAX;
    let mut min_y = i32::MAX;
    let mut min_z = i32::MAX;
    let mut max_x = i32::MIN;
    let mut max_y = i32::MIN;
    let mut max_z = i32::MIN;

    for vertex in &mesh.vertices {
        min_x = min_x.min(vertex.pos.x);
        min_y = min_y.min(vertex.pos.y);
        min_z = min_z.min(vertex.pos.z);
        max_x = max_x.max(vertex.pos.x);
        max_y = max_y.max(vertex.pos.y);
        max_z = max_z.max(vertex.pos.z);
    }

    // Handle empty mesh case and convert to float
    let scale = crate::rasterizer::INT_SCALE as f32;
    if min_x == i32::MAX {
        (Vec3::ZERO, Vec3::ZERO)
    } else {
        (
            Vec3::new(min_x as f32 / scale, min_y as f32 / scale, min_z as f32 / scale),
            Vec3::new(max_x as f32 / scale, max_y as f32 / scale, max_z as f32 / scale),
        )
    }
}

/// Draw a floor grid matching the world editor
/// Uses SECTOR_SIZE (1024 units) per grid cell - same scale everywhere
fn draw_preview_grid(fb: &mut Framebuffer, camera: &Camera) {
    let grid_color = RasterColor::new(50, 50, 60);
    let x_axis_color = RasterColor::new(100, 60, 60); // Red-ish for X axis
    let z_axis_color = RasterColor::new(60, 60, 100); // Blue-ish for Z axis

    // 4096 integer units per grid cell, 10 cells in each direction
    draw_floor_grid(fb, camera, 0, crate::world::SECTOR_SIZE_INT, crate::world::SECTOR_SIZE_INT * 10, grid_color, x_axis_color, z_axis_color);
}

/// Draw a close button (X)
fn draw_close_button(ctx: &mut UiContext, rect: Rect, icon_font: Option<&Font>) -> bool {
    let hovered = ctx.mouse.inside(&rect);
    let clicked = hovered && ctx.mouse.left_pressed;

    if hovered {
        draw_rectangle(rect.x, rect.y, rect.w, rect.h, Color::from_rgba(80, 40, 40, 255));
    }

    // Draw circle-X icon
    draw_icon_centered(icon_font, crate::ui::icon::CIRCLE_X, &rect, 16.0, WHITE);

    clicked
}

/// Draw a text button
fn draw_text_button(ctx: &mut UiContext, rect: Rect, text: &str, bg_color: Color) -> bool {
    draw_text_button_enabled(ctx, rect, text, bg_color, true)
}

/// Draw a text button with enabled state
fn draw_text_button_enabled(ctx: &mut UiContext, rect: Rect, text: &str, bg_color: Color, enabled: bool) -> bool {
    let hovered = enabled && ctx.mouse.inside(&rect);
    let clicked = hovered && ctx.mouse.left_pressed;

    let color = if !enabled {
        Color::from_rgba(50, 50, 55, 255)
    } else if hovered {
        Color::new(bg_color.r * 1.2, bg_color.g * 1.2, bg_color.b * 1.2, bg_color.a)
    } else {
        bg_color
    };

    draw_rectangle(rect.x, rect.y, rect.w, rect.h, color);

    let text_color = if enabled { WHITE } else { Color::from_rgba(100, 100, 100, 255) };
    let dims = measure_text(text, None, 14, 1.0);
    let tx = rect.x + (rect.w - dims.width) / 2.0;
    let ty = rect.y + (rect.h + dims.height) / 2.0 - 2.0;
    draw_text(text, tx, ty, 14.0, text_color);

    clicked
}

/// Flip mesh horizontally (mirror all vertices along X axis)
/// Public so it can be called from main.rs during import
pub fn apply_mesh_flip_horizontal(mesh: &mut EditableMesh) {
    // Find the center X to mirror around (integer math)
    let (min_x, max_x) = mesh.vertices.iter()
        .fold((i32::MAX, i32::MIN), |(mi, ma), v| (mi.min(v.pos.x), ma.max(v.pos.x)));
    if min_x == i32::MAX { return; } // Empty mesh

    // Mirror around integer center: new_x = 2*center - x = (min + max) - x
    let sum_x = min_x as i64 + max_x as i64;
    for vertex in &mut mesh.vertices {
        vertex.pos.x = (sum_x - vertex.pos.x as i64) as i32;
    }

    // Reverse face winding to maintain correct normals after mirror
    for face in &mut mesh.faces {
        face.vertices.reverse();
    }
}

/// Flip mesh vertically (mirror all vertices along Y axis)
/// Public so it can be called from main.rs during import
pub fn apply_mesh_flip_vertical(mesh: &mut EditableMesh) {
    // Find the center Y to mirror around (integer math)
    let (min_y, max_y) = mesh.vertices.iter()
        .fold((i32::MAX, i32::MIN), |(mi, ma), v| (mi.min(v.pos.y), ma.max(v.pos.y)));
    if min_y == i32::MAX { return; } // Empty mesh

    // Mirror around integer center: new_y = 2*center - y = (min + max) - y
    let sum_y = min_y as i64 + max_y as i64;
    for vertex in &mut mesh.vertices {
        vertex.pos.y = (sum_y - vertex.pos.y as i64) as i32;
    }

    // Reverse face winding to maintain correct normals after mirror
    for face in &mut mesh.faces {
        face.vertices.reverse();
    }
}
