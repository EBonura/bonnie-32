//! Asset Browser
//!
//! Modal dialog for browsing and previewing saved assets.
//! Assets are component-based 3D objects with embedded mesh + component definitions.

use macroquad::prelude::*;
use crate::ui::{Rect, UiContext, draw_icon_centered, draw_scrollable_list, ACCENT_COLOR};
use crate::rasterizer::{Framebuffer, Camera, Color as RasterColor, Vec3, RasterSettings, render_mesh, render_mesh_15, draw_floor_grid};
use crate::world::SECTOR_SIZE;
use crate::asset::{Asset, ASSETS_DIR};
use std::path::PathBuf;

/// Info about an asset file
#[derive(Debug, Clone)]
pub struct AssetInfo {
    /// Display name (file stem)
    pub name: String,
    /// Full path to the file
    pub path: PathBuf,
}

/// Discover asset files in the assets directory
#[cfg(not(target_arch = "wasm32"))]
pub fn discover_assets() -> Vec<AssetInfo> {
    let assets_dir = PathBuf::from(ASSETS_DIR);
    let mut assets = Vec::new();

    if let Ok(entries) = std::fs::read_dir(&assets_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().map_or(false, |ext| ext == "ron") {
                let name = path.file_stem()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_else(|| "unknown".to_string());
                assets.push(AssetInfo { name, path });
            }
        }
    }

    // Sort by name
    assets.sort_by(|a, b| a.name.cmp(&b.name));
    assets
}

#[cfg(target_arch = "wasm32")]
pub fn discover_assets() -> Vec<AssetInfo> {
    // WASM: return empty, load async from manifest
    Vec::new()
}

/// Load asset list from manifest asynchronously (for WASM)
pub async fn load_asset_list() -> Vec<AssetInfo> {
    use macroquad::prelude::*;

    // Load and parse manifest
    let manifest = match load_string(&format!("{}/manifest.txt", ASSETS_DIR)).await {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Failed to load assets manifest: {}", e);
            return Vec::new();
        }
    };

    let mut assets = Vec::new();

    for line in manifest.lines() {
        let line = line.trim();
        if line.is_empty() || !line.ends_with(".ron") {
            continue;
        }

        let name = line
            .strip_suffix(".ron")
            .unwrap_or(line)
            .to_string();
        let path = PathBuf::from(format!("{}/{}", ASSETS_DIR, line));

        assets.push(AssetInfo { name, path });
    }

    assets
}

/// Load a specific asset by path (for WASM async loading)
/// Supports both compressed (brotli) and uncompressed RON files
pub async fn load_asset_async(path: &PathBuf) -> Option<Asset> {
    use macroquad::prelude::*;

    let path_str = path.to_string_lossy().replace('\\', "/");

    // Load as binary to support both compressed and uncompressed
    let bytes = match load_file(&path_str).await {
        Ok(b) => b,
        Err(_) => return None,
    };

    Asset::load_from_bytes(&bytes).ok()
}

/// State for the asset browser dialog
pub struct AssetBrowser {
    /// Whether the browser is open
    pub open: bool,
    /// List of available assets
    pub assets: Vec<AssetInfo>,
    /// Currently selected index
    pub selected_index: Option<usize>,
    /// Currently loaded preview asset
    pub preview_asset: Option<Asset>,
    /// CLUTs for preview rendering (one per object, indexed by object index)
    pub preview_cluts: Vec<crate::rasterizer::Clut>,
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
    /// Path pending async load (WASM)
    pub pending_load_path: Option<PathBuf>,
    /// Whether we need to async load the asset list (WASM)
    pub pending_load_list: bool,
}

impl Default for AssetBrowser {
    fn default() -> Self {
        Self {
            open: false,
            assets: Vec::new(),
            selected_index: None,
            preview_asset: None,
            preview_cluts: Vec::new(),
            orbit_yaw: 0.5,
            orbit_pitch: 0.3,
            // Scale: 1024 units = 1 meter
            orbit_distance: 4096.0, // 4 meters back
            orbit_center: Vec3::new(0.0, 1024.0, 0.0), // 1 meter height
            dragging: false,
            last_mouse: (0.0, 0.0),
            scroll_offset: 0.0,
            pending_load_path: None,
            pending_load_list: false,
        }
    }
}

impl AssetBrowser {
    /// Open the browser with the given list of assets
    pub fn open(&mut self, assets: Vec<AssetInfo>) {
        self.open = true;
        self.assets = assets;
        self.selected_index = None;
        self.preview_asset = None;
        self.preview_cluts.clear();
        self.scroll_offset = 0.0;
    }

    /// Close the browser
    pub fn close(&mut self) {
        self.open = false;
        self.preview_asset = None;
        self.preview_cluts.clear();
    }

    /// Set the preview asset with texture resolution
    ///
    /// The texture library is used to resolve TextureRef::Id references
    /// so that preview rendering shows the correct textures.
    pub fn set_preview(&mut self, mut asset: Asset, texture_library: &crate::texture::TextureLibrary) {
        use crate::rasterizer::Clut;
        use super::mesh_editor::{TextureRef, checkerboard_clut};

        // Clear old CLUTs and prepare for new ones
        self.preview_cluts.clear();

        // Resolve texture references using the library and build CLUTs
        if let Some(objects) = asset.mesh_mut() {
            for obj in objects.iter_mut() {
                match &obj.texture_ref {
                    TextureRef::Id(id) => {
                        if let Some(tex) = texture_library.get_by_id(*id) {
                            // Copy texture data to atlas
                            obj.atlas.width = tex.width;
                            obj.atlas.height = tex.height;
                            obj.atlas.depth = tex.depth;
                            obj.atlas.indices = tex.indices.clone();
                            // Create CLUT with the texture's palette
                            let mut clut = Clut::new_4bit("preview");
                            clut.colors = tex.palette.clone();
                            clut.depth = tex.depth;
                            self.preview_cluts.push(clut);
                        } else {
                            // Texture not found, use checkerboard
                            self.preview_cluts.push(checkerboard_clut().clone());
                        }
                    }
                    TextureRef::Checkerboard | TextureRef::None => {
                        self.preview_cluts.push(checkerboard_clut().clone());
                    }
                    TextureRef::Embedded(_embedded) => {
                        // For embedded, we'd need a CLUT pool - use checkerboard for now
                        self.preview_cluts.push(checkerboard_clut().clone());
                    }
                }
            }
        }

        let asset = asset; // Make immutable again
        // Calculate bounding box to center camera (across all mesh objects)
        let mut min_y = f32::MAX;
        let mut max_y = f32::MIN;
        let mut min_x = f32::MAX;
        let mut max_x = f32::MIN;
        let mut min_z = f32::MAX;
        let mut max_z = f32::MIN;

        if let Some(objects) = asset.mesh() {
            for obj in objects {
                for vertex in &obj.mesh.vertices {
                    min_x = min_x.min(vertex.pos.x);
                    max_x = max_x.max(vertex.pos.x);
                    min_y = min_y.min(vertex.pos.y);
                    max_y = max_y.max(vertex.pos.y);
                    min_z = min_z.min(vertex.pos.z);
                    max_z = max_z.max(vertex.pos.z);
                }
            }
        }

        // Calculate center
        if min_y != f32::MAX {
            let center_x = (min_x + max_x) / 2.0;
            let center_y = (min_y + max_y) / 2.0;
            let center_z = (min_z + max_z) / 2.0;
            self.orbit_center = Vec3::new(center_x, center_y, center_z);

            // Set distance based on model size
            let size_x = max_x - min_x;
            let size_y = max_y - min_y;
            let size_z = max_z - min_z;
            let diagonal = (size_x * size_x + size_y * size_y + size_z * size_z).sqrt();
            // Min distance 2 meters (2048 units), scale by 1.5x model size
            self.orbit_distance = diagonal.max(2048.0) * 1.5;
        } else {
            // Fallback: 1024 units = 1 meter
            self.orbit_center = Vec3::new(0.0, 1024.0, 0.0);
            self.orbit_distance = 4096.0;
        }

        self.preview_asset = Some(asset);
        self.orbit_yaw = 0.8;
        self.orbit_pitch = 0.3;
    }

    /// Get the currently selected asset info
    pub fn selected_asset(&self) -> Option<&AssetInfo> {
        self.selected_index.and_then(|i| self.assets.get(i))
    }
}

/// Result from drawing the asset browser
#[derive(Debug, Clone, PartialEq)]
pub enum AssetBrowserAction {
    None,
    /// User selected an asset to preview
    SelectPreview(usize),
    /// User wants to open the selected asset
    OpenAsset,
    /// User wants to start with a new empty asset
    NewAsset,
    /// User cancelled
    Cancel,
}

/// Draw the asset browser modal dialog
pub fn draw_asset_browser(
    ctx: &mut UiContext,
    browser: &mut AssetBrowser,
    icon_font: Option<&Font>,
    fb: &mut Framebuffer,
) -> AssetBrowserAction {
    if !browser.open {
        return AssetBrowserAction::None;
    }

    let mut action = AssetBrowserAction::None;

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
    draw_text("Browse Assets", dialog_x + 16.0, dialog_y + 26.0, 20.0, WHITE);

    // Close button
    let close_rect = Rect::new(dialog_x + dialog_w - 36.0, dialog_y + 4.0, 32.0, 32.0);
    if draw_close_button(ctx, close_rect, icon_font) {
        action = AssetBrowserAction::Cancel;
    }

    // Content area
    let content_y = dialog_y + header_h + 8.0;
    let content_h = dialog_h - header_h - 60.0;
    let list_w = 200.0;

    // List panel (left)
    let list_rect = Rect::new(dialog_x + 8.0, content_y, list_w, content_h);
    let item_h = 28.0;

    let items: Vec<String> = browser.assets.iter().map(|a| a.name.clone()).collect();

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
            action = AssetBrowserAction::SelectPreview(clicked_idx);
        }
    }

    // Preview panel (right)
    let preview_x = dialog_x + list_w + 16.0;
    let preview_w = dialog_w - list_w - 24.0;
    let preview_rect = Rect::new(preview_x, content_y, preview_w, content_h);

    draw_rectangle(preview_rect.x, preview_rect.y, preview_rect.w, preview_rect.h, Color::from_rgba(20, 20, 25, 255));

    let has_preview = browser.preview_asset.is_some();
    let has_selection = browser.selected_index.is_some();

    if has_preview {
        draw_orbit_preview(ctx, browser, preview_rect, fb);

        // Draw stats at bottom of preview
        if let Some(asset) = &browser.preview_asset {
            let stats_y = preview_rect.bottom() - 24.0;
            draw_rectangle(preview_rect.x, stats_y, preview_rect.w, 24.0, Color::from_rgba(30, 30, 35, 200));

            let obj_count = asset.mesh().map(|m| m.len()).unwrap_or(0);
            let comp_count = asset.components.len();
            let stats_text = format!(
                "Vertices: {}  Faces: {}  Objects: {}  Components: {}",
                asset.total_vertices(), asset.total_faces(), obj_count, comp_count
            );
            draw_text(&stats_text, preview_rect.x + 8.0, stats_y + 17.0, 14.0, Color::from_rgba(180, 180, 180, 255));
        }
    } else if has_selection {
        draw_text("Loading preview...", preview_rect.x + 20.0, preview_rect.y + 40.0, 16.0, Color::from_rgba(150, 150, 150, 255));
    } else if browser.assets.is_empty() {
        draw_text(&format!("No assets found in {}/", ASSETS_DIR), preview_rect.x + 20.0, preview_rect.y + 40.0, 16.0, Color::from_rgba(100, 100, 100, 255));
        draw_text("Save an asset first!", preview_rect.x + 20.0, preview_rect.y + 60.0, 14.0, Color::from_rgba(80, 80, 80, 255));
    } else {
        draw_text("Select an asset to preview", preview_rect.x + 20.0, preview_rect.y + 40.0, 16.0, Color::from_rgba(100, 100, 100, 255));
    }

    // Footer with buttons
    let footer_y = dialog_y + dialog_h - 44.0;
    draw_rectangle(dialog_x, footer_y, dialog_w, 44.0, Color::from_rgba(40, 40, 48, 255));

    // New button (left side)
    let new_rect = Rect::new(dialog_x + 10.0, footer_y + 8.0, 80.0, 28.0);
    if draw_text_button(ctx, new_rect, "New", Color::from_rgba(60, 60, 70, 255)) {
        action = AssetBrowserAction::NewAsset;
    }

    // Cancel button
    let cancel_rect = Rect::new(dialog_x + dialog_w - 180.0, footer_y + 8.0, 80.0, 28.0);
    if draw_text_button(ctx, cancel_rect, "Cancel", Color::from_rgba(60, 60, 70, 255)) {
        action = AssetBrowserAction::Cancel;
    }

    // Open button
    let open_rect = Rect::new(dialog_x + dialog_w - 90.0, footer_y + 8.0, 80.0, 28.0);
    let open_enabled = browser.preview_asset.is_some();
    if draw_text_button_enabled(ctx, open_rect, "Open", ACCENT_COLOR, open_enabled) {
        action = AssetBrowserAction::OpenAsset;
    }

    // Handle Escape to close
    if is_key_pressed(KeyCode::Escape) {
        action = AssetBrowserAction::Cancel;
    }

    action
}

/// Draw the orbit preview of an asset
fn draw_orbit_preview(
    ctx: &mut UiContext,
    browser: &mut AssetBrowser,
    rect: Rect,
    fb: &mut Framebuffer,
) {
    let asset = match &browser.preview_asset {
        Some(a) => a,
        None => return,
    };

    let objects = match asset.mesh() {
        Some(objs) => objs,
        None => {
            // Asset has no mesh - show placeholder
            draw_text("No mesh", rect.x + rect.w / 2.0 - 30.0, rect.y + rect.h / 2.0, 16.0, Color::from_rgba(100, 100, 100, 255));
            return;
        }
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

        // Scroll to zoom (multiplicative for smooth zooming, wider range)
        let scroll = ctx.mouse.scroll;
        if scroll != 0.0 {
            let zoom_factor = if scroll > 0.0 { 0.9 } else { 1.1 };
            browser.orbit_distance = (browser.orbit_distance * zoom_factor).clamp(10.0, 5000.0);
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

    // Check if any object needs backface culling disabled
    let any_double_sided = objects.iter().any(|obj| obj.visible && obj.double_sided);

    // Render settings - disable backface culling if any object is double-sided
    let mut settings = RasterSettings::default();
    if any_double_sided {
        settings.backface_cull = false;
    }
    let use_rgb555 = settings.use_rgb555;

    // Use preview_cluts which were populated during set_preview with actual palettes
    use crate::modeler::mesh_editor::checkerboard_clut;
    let fallback_clut = checkerboard_clut();

    // Render each visible object with its atlas texture and corresponding CLUT
    for (obj_idx, obj) in objects.iter().enumerate() {
        if !obj.visible {
            continue;
        }

        let (vertices, faces) = obj.mesh.to_render_data_textured();
        if vertices.is_empty() {
            continue;
        }

        // Use the object's CLUT from preview_cluts, or fallback to checkerboard
        let clut = browser.preview_cluts.get(obj_idx).unwrap_or(fallback_clut);

        if use_rgb555 {
            let tex15 = obj.atlas.to_texture15(clut, &format!("atlas_{}", obj_idx));
            let textures_15 = [tex15];
            render_mesh_15(fb, &vertices, &faces, &textures_15, None, &camera, &settings, None);
        } else {
            let tex = obj.atlas.to_raster_texture(clut, &format!("atlas_{}", obj_idx));
            let textures = [tex];
            render_mesh(fb, &vertices, &faces, &textures, &camera, &settings);
        }
    }

    // Draw a simple floor plane indicator using the grid drawing
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

/// Draw a floor grid matching the world editor
/// Uses SECTOR_SIZE (1024 units = 1 meter) per grid cell - same scale everywhere
fn draw_preview_grid(fb: &mut Framebuffer, camera: &Camera) {
    let grid_color = RasterColor::new(50, 50, 60);
    let x_axis_color = RasterColor::new(100, 60, 60); // Red-ish for X axis
    let z_axis_color = RasterColor::new(60, 60, 100); // Blue-ish for Z axis

    // 1024 units per grid cell, 10 cells in each direction (10240 unit extent)
    draw_floor_grid(fb, camera, 0.0, SECTOR_SIZE, SECTOR_SIZE * 10.0, grid_color, x_axis_color, z_axis_color);
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

// ============================================================================
// Legacy aliases for backward compatibility
// ============================================================================

/// Legacy alias for AssetInfo
pub type ModelInfo = AssetInfo;

/// Legacy alias for AssetBrowser
pub type ModelBrowser = AssetBrowser;

/// Legacy alias for AssetBrowserAction
pub type ModelBrowserAction = AssetBrowserAction;

/// Legacy alias for discover_assets
pub fn discover_models() -> Vec<AssetInfo> {
    discover_assets()
}

/// Legacy alias for load_asset_list
pub async fn load_model_list() -> Vec<AssetInfo> {
    load_asset_list().await
}

/// Legacy alias for load_asset_async
pub async fn load_model(path: &PathBuf) -> Option<Asset> {
    load_asset_async(path).await
}

/// Legacy alias for draw_asset_browser
pub fn draw_model_browser(
    ctx: &mut UiContext,
    browser: &mut AssetBrowser,
    icon_font: Option<&Font>,
    fb: &mut Framebuffer,
) -> AssetBrowserAction {
    draw_asset_browser(ctx, browser, icon_font, fb)
}
