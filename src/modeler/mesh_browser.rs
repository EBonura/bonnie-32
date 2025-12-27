//! Mesh Browser
//!
//! Modal dialog for browsing and previewing OBJ mesh files.

use macroquad::prelude::*;
use crate::ui::{Rect, UiContext, draw_icon_centered, draw_scrollable_list, ACCENT_COLOR, TEXT_COLOR};
use crate::rasterizer::{Framebuffer, Camera, Color as RasterColor, Vec3, RasterSettings, render_mesh, world_to_screen};
use super::mesh_editor::EditableMesh;
use super::obj_import::ObjImporter;
use std::path::PathBuf;

/// Info about a mesh file
#[derive(Debug, Clone)]
pub struct MeshInfo {
    /// Display name (file stem)
    pub name: String,
    /// Full path to the file
    pub path: PathBuf,
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

    if let Ok(entries) = std::fs::read_dir(&meshes_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().map_or(false, |ext| ext == "obj") {
                let name = path.file_stem()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_else(|| "unknown".to_string());

                // Parse to get vertex/face counts
                let (vertex_count, face_count) = if let Ok(mesh) = ObjImporter::load_from_file(&path) {
                    (mesh.vertices.len(), mesh.faces.len())
                } else {
                    (0, 0)
                };

                meshes.push(MeshInfo { name, path, vertex_count, face_count });
            }
        }
    }

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
        meshes.push(MeshInfo { name, path, vertex_count: 0, face_count: 0 });
    }

    meshes
}

/// Load a specific mesh by path (for WASM async loading)
pub async fn load_mesh(path: &PathBuf) -> Option<EditableMesh> {
    use macroquad::prelude::*;

    let path_str = path.to_string_lossy().replace('\\', "/");
    match load_string(&path_str).await {
        Ok(contents) => ObjImporter::parse(&contents).ok(),
        Err(_) => None,
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
    /// Path pending async load (WASM)
    pub pending_load_path: Option<PathBuf>,
    /// Whether we need to async load the mesh list (WASM)
    pub pending_load_list: bool,
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
            orbit_distance: 300.0,
            orbit_center: Vec3::new(0.0, 0.0, 0.0),
            dragging: false,
            last_mouse: (0.0, 0.0),
            scroll_offset: 0.0,
            import_scale: 100.0, // OBJ meshes are typically ~1 unit, scale up to match world
            pending_load_path: None,
            pending_load_list: false,
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
        self.scroll_offset = 0.0;
    }

    /// Close the browser
    pub fn close(&mut self) {
        self.open = false;
        self.preview_mesh = None;
    }

    /// Set the preview mesh
    pub fn set_preview(&mut self, mesh: EditableMesh) {
        // Calculate bounding box to center camera
        let mut min = Vec3::new(f32::MAX, f32::MAX, f32::MAX);
        let mut max = Vec3::new(f32::MIN, f32::MIN, f32::MIN);

        for vertex in &mesh.vertices {
            min.x = min.x.min(vertex.pos.x);
            min.y = min.y.min(vertex.pos.y);
            min.z = min.z.min(vertex.pos.z);
            max.x = max.x.max(vertex.pos.x);
            max.y = max.y.max(vertex.pos.y);
            max.z = max.z.max(vertex.pos.z);
        }

        // Calculate center
        if min.x != f32::MAX {
            self.orbit_center = Vec3::new(
                (min.x + max.x) / 2.0,
                (min.y + max.y) / 2.0,
                (min.z + max.z) / 2.0,
            );

            // Set distance based on mesh size
            let size_x = max.x - min.x;
            let size_y = max.y - min.y;
            let size_z = max.z - min.z;
            let diagonal = (size_x * size_x + size_y * size_y + size_z * size_z).sqrt();
            self.orbit_distance = diagonal.max(0.5) * 2.0;
        } else {
            self.orbit_center = Vec3::new(0.0, 0.0, 0.0);
            self.orbit_distance = 2.0;
        }

        self.preview_mesh = Some(mesh);
        self.orbit_yaw = 0.8;
        self.orbit_pitch = 0.3;
    }

    /// Get the currently selected mesh info
    pub fn selected_mesh(&self) -> Option<&MeshInfo> {
        self.selected_index.and_then(|i| self.meshes.get(i))
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

    let items: Vec<String> = browser.meshes.iter().map(|m| m.name.clone()).collect();

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

    // Preview panel (right)
    let preview_x = dialog_x + list_w + 16.0;
    let preview_w = dialog_w - list_w - 24.0;
    let preview_rect = Rect::new(preview_x, content_y, preview_w, content_h);

    draw_rectangle(preview_rect.x, preview_rect.y, preview_rect.w, preview_rect.h, Color::from_rgba(20, 20, 25, 255));

    let has_preview = browser.preview_mesh.is_some();
    let has_selection = browser.selected_index.is_some();

    if has_preview {
        draw_orbit_preview(ctx, browser, preview_rect, fb);

        // Draw stats at bottom of preview
        if let Some(idx) = browser.selected_index {
            if let Some(info) = browser.meshes.get(idx) {
                let stats_y = preview_rect.bottom() - 24.0;
                draw_rectangle(preview_rect.x, stats_y, preview_rect.w, 24.0, Color::from_rgba(30, 30, 35, 200));

                let stats_text = format!(
                    "Vertices: {}  Faces: {}",
                    info.vertex_count, info.face_count
                );
                draw_text(&stats_text, preview_rect.x + 8.0, stats_y + 17.0, 14.0, Color::from_rgba(180, 180, 180, 255));
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

    // Footer with buttons
    let footer_y = dialog_y + dialog_h - 44.0;
    draw_rectangle(dialog_x, footer_y, dialog_w, 44.0, Color::from_rgba(40, 40, 48, 255));

    // Scale control on the left side of footer
    draw_text("Scale:", dialog_x + 12.0, footer_y + 22.0, 14.0, TEXT_COLOR);
    let scale_minus_rect = Rect::new(dialog_x + 60.0, footer_y + 8.0, 28.0, 28.0);
    let scale_plus_rect = Rect::new(dialog_x + 150.0, footer_y + 8.0, 28.0, 28.0);

    if draw_text_button(ctx, scale_minus_rect, "-", Color::from_rgba(60, 60, 70, 255)) {
        browser.import_scale = (browser.import_scale / 2.0).max(1.0);
    }

    // Display current scale
    let scale_text = format!("{:.0}", browser.import_scale);
    let text_width = measure_text(&scale_text, None, 14, 1.0).width;
    draw_text(&scale_text, dialog_x + 104.0 - text_width / 2.0, footer_y + 22.0, 14.0, TEXT_COLOR);

    if draw_text_button(ctx, scale_plus_rect, "+", Color::from_rgba(60, 60, 70, 255)) {
        browser.import_scale = (browser.import_scale * 2.0).min(1000.0);
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
        let scroll = ctx.mouse.scroll;
        if scroll != 0.0 {
            browser.orbit_distance = (browser.orbit_distance * (1.0 - scroll * 0.1)).clamp(0.3, 500.0);
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
    camera.position = cam_pos;

    // Calculate rotation from direction
    let dir = browser.orbit_center - cam_pos;
    let len = dir.len();
    let n = dir * (1.0 / len);
    camera.rotation_x = (-n.y).asin();
    camera.rotation_y = n.x.atan2(n.z);
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
        render_mesh(fb, &mesh.vertices, &mesh.faces, &[], &camera, &settings);
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

/// Draw a simple grid on the floor (Y=0 plane) with depth testing
fn draw_preview_grid(fb: &mut Framebuffer, camera: &Camera) {
    let grid_color = RasterColor::new(50, 50, 60);
    let grid_size = 2.0;
    let grid_step = 0.25;
    let half = grid_size / 2.0;

    // Helper to draw a 3D line with depth testing
    let draw_line_depth = |fb: &mut Framebuffer, p0: Vec3, p1: Vec3, color: RasterColor| {
        // Calculate relative positions and depths
        let rel0 = p0 - camera.position;
        let rel1 = p1 - camera.position;
        let z0 = rel0.dot(camera.basis_z);
        let z1 = rel1.dot(camera.basis_z);

        // Skip if both behind camera
        if z0 <= 0.1 && z1 <= 0.1 {
            return;
        }

        let screen0 = world_to_screen(p0, camera.position, camera.basis_x, camera.basis_y, camera.basis_z, fb.width, fb.height);
        let screen1 = world_to_screen(p1, camera.position, camera.basis_x, camera.basis_y, camera.basis_z, fb.width, fb.height);

        if let (Some((x0, y0)), Some((x1, y1))) = (screen0, screen1) {
            fb.draw_line_3d(x0 as i32, y0 as i32, z0, x1 as i32, y1 as i32, z1, color);
        }
    };

    // Draw grid lines
    let mut x = -half;
    while x <= half {
        draw_line_depth(fb, Vec3::new(x, 0.0, -half), Vec3::new(x, 0.0, half), grid_color);
        x += grid_step;
    }

    let mut z = -half;
    while z <= half {
        draw_line_depth(fb, Vec3::new(-half, 0.0, z), Vec3::new(half, 0.0, z), grid_color);
        z += grid_step;
    }
}

/// Draw a close button (X)
fn draw_close_button(ctx: &mut UiContext, rect: Rect, icon_font: Option<&Font>) -> bool {
    let hovered = ctx.mouse.inside(&rect);
    let clicked = hovered && ctx.mouse.left_pressed;

    if hovered {
        draw_rectangle(rect.x, rect.y, rect.w, rect.h, Color::from_rgba(80, 40, 40, 255));
    }

    let x_char = '\u{e1c9}'; // Lucide X icon
    draw_icon_centered(icon_font, x_char, &rect, 16.0, WHITE);

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
