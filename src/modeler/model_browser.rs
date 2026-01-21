//! Asset Browser
//!
//! Modal dialog for browsing and previewing saved assets.
//! Assets are component-based 3D objects with embedded mesh + component definitions.
//!
//! Two-section layout matching the level browser:
//! - SAMPLES: bundled read-only sample assets
//! - MY ASSETS: user-created assets (editable, cloud-synced)

use macroquad::prelude::*;
use crate::storage::{Storage, PendingLoad, PendingList};
use crate::ui::{Rect, UiContext, draw_icon_centered, ACCENT_COLOR};
use crate::rasterizer::{Framebuffer, Camera, Color as RasterColor, Vec3, RasterSettings, render_mesh, render_mesh_15, draw_floor_grid};
use crate::world::SECTOR_SIZE;
use crate::asset::{Asset, SAMPLES_ASSETS_DIR, USER_ASSETS_DIR};
use std::path::PathBuf;

/// Category of asset (sample or user-created)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssetCategory {
    /// Bundled sample asset (read-only)
    Sample,
    /// User-created asset (editable, cloud-synced)
    User,
}

/// Info about an asset file
#[derive(Debug, Clone)]
pub struct AssetInfo {
    /// Display name (file stem)
    pub name: String,
    /// Full path to the file
    pub path: PathBuf,
    /// Category (sample or user)
    pub category: AssetCategory,
}

/// Discover sample asset files (read-only bundled assets)
#[cfg(not(target_arch = "wasm32"))]
pub fn discover_sample_assets() -> Vec<AssetInfo> {
    discover_assets_from_dir(SAMPLES_ASSETS_DIR, AssetCategory::Sample)
}

/// Discover user asset files (editable, cloud-synced)
#[cfg(not(target_arch = "wasm32"))]
pub fn discover_user_assets() -> Vec<AssetInfo> {
    discover_assets_from_dir(USER_ASSETS_DIR, AssetCategory::User)
}

/// Discover asset files from both sample and user directories (legacy compatibility)
#[cfg(not(target_arch = "wasm32"))]
pub fn discover_assets() -> Vec<AssetInfo> {
    let mut assets = Vec::new();
    assets.extend(discover_sample_assets());
    assets.extend(discover_user_assets());
    assets
}

/// Discover asset files from a specific directory
#[cfg(not(target_arch = "wasm32"))]
fn discover_assets_from_dir(dir: &str, category: AssetCategory) -> Vec<AssetInfo> {
    let assets_dir = PathBuf::from(dir);
    let mut assets = Vec::new();

    if let Ok(entries) = std::fs::read_dir(&assets_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().map_or(false, |ext| ext == "ron") {
                let name = path.file_stem()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_else(|| "unknown".to_string());
                assets.push(AssetInfo { name, path, category });
            }
        }
    }

    // Sort by name
    assets.sort_by(|a, b| a.name.cmp(&b.name));
    assets
}

#[cfg(target_arch = "wasm32")]
pub fn discover_sample_assets() -> Vec<AssetInfo> {
    Vec::new()
}

#[cfg(target_arch = "wasm32")]
pub fn discover_user_assets() -> Vec<AssetInfo> {
    Vec::new()
}

#[cfg(target_arch = "wasm32")]
pub fn discover_assets() -> Vec<AssetInfo> {
    Vec::new()
}

/// Load sample asset list from manifest asynchronously (for WASM)
pub async fn load_sample_asset_list() -> Vec<AssetInfo> {
    load_asset_list_from_dir(SAMPLES_ASSETS_DIR, AssetCategory::Sample).await
}

/// Load user asset list from manifest asynchronously (for WASM)
pub async fn load_user_asset_list() -> Vec<AssetInfo> {
    load_asset_list_from_dir(USER_ASSETS_DIR, AssetCategory::User).await
}

/// Load asset list from manifest asynchronously (for WASM)
/// Loads from both samples and user directories.
pub async fn load_asset_list() -> Vec<AssetInfo> {
    let mut assets = Vec::new();
    assets.extend(load_sample_asset_list().await);
    assets.extend(load_user_asset_list().await);
    assets
}

/// Load asset list from a specific directory's manifest (for WASM)
async fn load_asset_list_from_dir(dir: &str, category: AssetCategory) -> Vec<AssetInfo> {
    use macroquad::prelude::*;

    // Load and parse manifest
    let manifest = match load_string(&format!("{}/manifest.txt", dir)).await {
        Ok(s) => s,
        Err(_) => {
            // Manifest not found for this directory, skip silently
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
        let path = PathBuf::from(format!("{}/{}", dir, line));

        assets.push(AssetInfo { name, path, category });
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
    /// Sample assets (read-only, bundled)
    pub samples: Vec<AssetInfo>,
    /// User assets (editable, cloud-synced)
    pub user_assets: Vec<AssetInfo>,
    /// Whether samples section is collapsed
    pub samples_collapsed: bool,
    /// Whether user assets section is collapsed
    pub user_collapsed: bool,
    /// Currently selected category
    pub selected_category: Option<AssetCategory>,
    /// Currently selected index within category
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
    /// Pending async preview load (native cloud storage)
    pub pending_preview_load: Option<PendingLoad>,
    /// Pending async user asset list (native cloud storage)
    pub pending_user_list: Option<PendingList>,
    /// Local framebuffer for preview rendering
    preview_fb: Framebuffer,
}

impl Default for AssetBrowser {
    fn default() -> Self {
        Self {
            open: false,
            samples: Vec::new(),
            user_assets: Vec::new(),
            samples_collapsed: false,
            user_collapsed: false,
            selected_category: None,
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
            pending_preview_load: None,
            pending_user_list: None,
            preview_fb: Framebuffer::new(320, 240), // Initial size, will resize as needed
        }
    }
}

impl AssetBrowser {
    /// Open the browser with sample and user assets
    pub fn open_with_assets(&mut self, samples: Vec<AssetInfo>, user_assets: Vec<AssetInfo>) {
        self.open = true;
        self.samples = samples;
        self.user_assets = user_assets;
        self.selected_category = None;
        self.selected_index = None;
        self.preview_asset = None;
        self.preview_cluts.clear();
        self.scroll_offset = 0.0;
    }

    /// Open the browser with just sample assets (legacy compatibility)
    pub fn open(&mut self, samples: Vec<AssetInfo>) {
        self.open_with_assets(samples, Vec::new());
    }

    /// Close the browser
    pub fn close(&mut self) {
        self.open = false;
        self.preview_asset = None;
        self.preview_cluts.clear();
        self.pending_preview_load = None;
    }

    /// Check if the selected asset is a sample (read-only)
    pub fn is_sample_selected(&self) -> bool {
        self.selected_category == Some(AssetCategory::Sample)
    }

    /// Check if the selected asset is a user asset (editable)
    pub fn is_user_selected(&self) -> bool {
        self.selected_category == Some(AssetCategory::User)
    }

    /// Check if a preview is currently being loaded
    pub fn is_loading_preview(&self) -> bool {
        self.pending_preview_load.is_some() || self.pending_load_path.is_some()
    }

    /// Check if user assets are being loaded
    pub fn is_loading_user_assets(&self) -> bool {
        self.pending_user_list.is_some()
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
        match (self.selected_category, self.selected_index) {
            (Some(AssetCategory::Sample), Some(i)) => self.samples.get(i),
            (Some(AssetCategory::User), Some(i)) => self.user_assets.get(i),
            _ => None,
        }
    }
}

/// Result from drawing the asset browser
#[derive(Debug, Clone, PartialEq)]
pub enum AssetBrowserAction {
    None,
    /// User selected an asset to preview (need to load it async)
    SelectPreview(AssetCategory, usize),
    /// User wants to open the selected asset
    OpenAsset,
    /// User wants to create a copy of the sample as a user asset
    OpenCopy,
    /// User wants to delete the selected user asset
    DeleteAsset,
    /// User wants to start with a new empty asset
    NewAsset,
    /// User wants to refresh the asset list
    Refresh,
    /// User cancelled
    Cancel,
}

/// Draw the asset browser modal dialog
pub fn draw_asset_browser(
    ctx: &mut UiContext,
    browser: &mut AssetBrowser,
    storage: &crate::storage::Storage,
    icon_font: Option<&Font>,
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
    draw_text("Asset Browser", dialog_x + 16.0, dialog_y + 26.0, 20.0, WHITE);

    // Close button
    let close_rect = Rect::new(dialog_x + dialog_w - 36.0, dialog_y + 4.0, 32.0, 32.0);
    if draw_close_button(ctx, close_rect, icon_font) {
        action = AssetBrowserAction::Cancel;
    }

    // Content area
    let content_y = dialog_y + header_h + 8.0;
    let content_h = dialog_h - header_h - 60.0; // Leave room for footer
    let list_w = 220.0;

    // List panel (left) - custom two-section list
    let list_rect = Rect::new(dialog_x + 8.0, content_y, list_w, content_h);
    draw_rectangle(list_rect.x, list_rect.y, list_rect.w, list_rect.h, Color::from_rgba(25, 25, 30, 255));

    let item_h = 26.0;
    let section_h = 28.0;
    let has_cloud = storage.has_cloud();

    // Draw two-section list
    let list_action = draw_two_section_list(
        ctx,
        list_rect,
        browser,
        item_h,
        section_h,
        has_cloud,
    );

    // Handle list actions
    if let Some((category, idx)) = list_action {
        if browser.selected_category != Some(category) || browser.selected_index != Some(idx) {
            browser.selected_category = Some(category);
            browser.selected_index = Some(idx);
            action = AssetBrowserAction::SelectPreview(category, idx);
        }
    }

    // Preview panel (right)
    let preview_x = dialog_x + list_w + 16.0;
    let preview_w = dialog_w - list_w - 24.0;
    let preview_rect = Rect::new(preview_x, content_y, preview_w, content_h);

    draw_rectangle(preview_rect.x, preview_rect.y, preview_rect.w, preview_rect.h, Color::from_rgba(20, 20, 25, 255));

    // Draw preview content
    let has_preview = browser.preview_asset.is_some();
    let has_selection = browser.selected_category.is_some();

    if has_preview {
        // Render 3D preview with orbit camera (uses browser's local framebuffer)
        draw_orbit_preview_internal(ctx, browser, preview_rect);

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
    } else if browser.is_loading_preview() {
        // Loading indicator with animated spinner
        let time = get_time() as f32;
        let spinner_chars = ['|', '/', '-', '\\'];
        let spinner_idx = (time * 8.0) as usize % spinner_chars.len();
        let loading_text = format!("{} Loading preview...", spinner_chars[spinner_idx]);
        draw_text(&loading_text, preview_rect.x + 20.0, preview_rect.y + 40.0, 16.0, Color::from_rgba(150, 150, 180, 255));
    } else if has_selection {
        // Selection but no preview loaded yet (shouldn't normally happen)
        draw_text("Select to load preview", preview_rect.x + 20.0, preview_rect.y + 40.0, 16.0, Color::from_rgba(150, 150, 150, 255));
    } else {
        // No selection
        draw_text("Select an asset to preview", preview_rect.x + 20.0, preview_rect.y + 40.0, 16.0, Color::from_rgba(100, 100, 100, 255));
    }

    // Footer with buttons
    let footer_y = dialog_y + dialog_h - 44.0;
    draw_rectangle(dialog_x, footer_y, dialog_w, 44.0, Color::from_rgba(40, 40, 48, 255));

    // New button (left side) - start with empty asset
    let new_rect = Rect::new(dialog_x + 10.0, footer_y + 8.0, 70.0, 28.0);
    if draw_text_button(ctx, new_rect, "New", Color::from_rgba(60, 60, 70, 255)) {
        action = AssetBrowserAction::NewAsset;
    }

    // Delete button (only for user assets)
    let delete_rect = Rect::new(dialog_x + 90.0, footer_y + 8.0, 70.0, 28.0);
    let delete_enabled = browser.is_user_selected() && browser.preview_asset.is_some();
    if draw_text_button_enabled(ctx, delete_rect, "Delete", Color::from_rgba(120, 50, 50, 255), delete_enabled) {
        action = AssetBrowserAction::DeleteAsset;
    }

    // Refresh button - reload asset lists from storage
    let refresh_rect = Rect::new(dialog_x + 170.0, footer_y + 8.0, 70.0, 28.0);
    if draw_text_button(ctx, refresh_rect, "Refresh", Color::from_rgba(60, 60, 70, 255)) {
        action = AssetBrowserAction::Refresh;
    }

    // Cancel button
    let cancel_rect = Rect::new(dialog_x + dialog_w - 270.0, footer_y + 8.0, 70.0, 28.0);
    if draw_text_button(ctx, cancel_rect, "Cancel", Color::from_rgba(60, 60, 70, 255)) {
        action = AssetBrowserAction::Cancel;
    }

    // Open Copy button (only for samples - copies sample to user asset)
    let copy_rect = Rect::new(dialog_x + dialog_w - 190.0, footer_y + 8.0, 90.0, 28.0);
    let copy_enabled = browser.is_sample_selected() && browser.preview_asset.is_some() && storage.can_write();
    if draw_text_button_enabled(ctx, copy_rect, "Open Copy", Color::from_rgba(60, 80, 60, 255), copy_enabled) {
        action = AssetBrowserAction::OpenCopy;
    }

    // Open button (enabled if something is selected and loaded)
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

/// Draw the two-section list (Samples + My Assets)
fn draw_two_section_list(
    ctx: &mut UiContext,
    rect: Rect,
    browser: &mut AssetBrowser,
    item_h: f32,
    section_h: f32,
    has_cloud: bool,
) -> Option<(AssetCategory, usize)> {
    let mut clicked: Option<(AssetCategory, usize)> = None;
    let mut y = rect.y - browser.scroll_offset;

    let section_bg = Color::from_rgba(40, 40, 50, 255);
    let item_bg = Color::from_rgba(30, 30, 38, 255);
    let item_hover = Color::from_rgba(50, 50, 60, 255);
    let item_selected = Color::from_rgba(60, 80, 120, 255);
    let text_color = Color::from_rgba(200, 200, 200, 255);
    let text_dim = Color::from_rgba(140, 140, 140, 255);
    let cloud_color = Color::from_rgba(100, 180, 255, 255);

    // Calculate total content height for scroll
    let samples_content_h = if browser.samples_collapsed { 0.0 } else { browser.samples.len() as f32 * item_h };
    let user_content_h = if browser.user_collapsed { 0.0 } else { browser.user_assets.len() as f32 * item_h };
    let total_h = section_h * 2.0 + samples_content_h + user_content_h;

    // Handle scroll within list bounds
    if ctx.mouse.inside(&rect) && ctx.mouse.scroll != 0.0 {
        browser.scroll_offset = (browser.scroll_offset - ctx.mouse.scroll * 30.0)
            .clamp(0.0, (total_h - rect.h).max(0.0));
    }

    // SAMPLES section header
    let samples_header_rect = Rect::new(rect.x, y, rect.w, section_h);
    if y + section_h > rect.y && y < rect.bottom() {
        draw_rectangle(samples_header_rect.x, samples_header_rect.y.max(rect.y),
                      samples_header_rect.w, section_h.min(rect.bottom() - samples_header_rect.y.max(rect.y)), section_bg);

        let arrow = if browser.samples_collapsed { ">" } else { "v" };
        draw_text(&format!("{} SAMPLE ASSETS ({})", arrow, browser.samples.len()),
                 rect.x + 8.0, y + 18.0, 14.0, text_color);

        // Toggle collapse on click
        if ctx.mouse.inside(&samples_header_rect) && ctx.mouse.left_pressed {
            browser.samples_collapsed = !browser.samples_collapsed;
        }
    }
    y += section_h;

    // SAMPLES items
    if !browser.samples_collapsed {
        for (i, asset) in browser.samples.iter().enumerate() {
            let item_rect = Rect::new(rect.x, y, rect.w, item_h);

            if y + item_h > rect.y && y < rect.bottom() {
                let is_selected = browser.selected_category == Some(AssetCategory::Sample)
                    && browser.selected_index == Some(i);
                let is_hovered = ctx.mouse.inside(&item_rect) && item_rect.y >= rect.y;

                let bg = if is_selected { item_selected }
                        else if is_hovered { item_hover }
                        else { item_bg };

                // Clip to list bounds
                let draw_y = item_rect.y.max(rect.y);
                let draw_h = item_h.min(rect.bottom() - draw_y);
                if draw_h > 0.0 {
                    draw_rectangle(item_rect.x + 2.0, draw_y, item_rect.w - 4.0, draw_h, bg);

                    if y >= rect.y {
                        draw_text(&asset.name, rect.x + 20.0, y + 17.0, 13.0, text_color);
                    }
                }

                // Handle click
                if is_hovered && ctx.mouse.left_pressed && item_rect.y >= rect.y {
                    clicked = Some((AssetCategory::Sample, i));
                }
            }
            y += item_h;
        }
    }

    // MY ASSETS section header
    let user_header_rect = Rect::new(rect.x, y, rect.w, section_h);
    if y + section_h > rect.y && y < rect.bottom() {
        draw_rectangle(user_header_rect.x, user_header_rect.y.max(rect.y),
                      user_header_rect.w, section_h.min(rect.bottom() - user_header_rect.y.max(rect.y)), section_bg);

        let arrow = if browser.user_collapsed { ">" } else { "v" };
        let cloud_indicator = if has_cloud { " [cloud]" } else { "" };
        draw_text(&format!("{} MY ASSETS ({}){}", arrow, browser.user_assets.len(), cloud_indicator),
                 rect.x + 8.0, y + 18.0, 14.0, text_color);

        // Toggle collapse on click
        if ctx.mouse.inside(&user_header_rect) && ctx.mouse.left_pressed && user_header_rect.y >= rect.y {
            browser.user_collapsed = !browser.user_collapsed;
        }
    }
    y += section_h;

    // MY ASSETS items
    if !browser.user_collapsed {
        if browser.is_loading_user_assets() {
            // Show loading indicator
            if y + item_h > rect.y && y < rect.bottom() {
                let time = get_time() as f32;
                let spinner_chars = ['|', '/', '-', '\\'];
                let spinner_idx = (time * 8.0) as usize % spinner_chars.len();
                let loading_text = format!("  {} Loading...", spinner_chars[spinner_idx]);
                draw_text(&loading_text, rect.x + 8.0, y + 17.0, 12.0, text_dim);
            }
        } else if browser.user_assets.is_empty() {
            // Show empty state message
            if y + item_h > rect.y && y < rect.bottom() {
                draw_text("  (no saved assets)", rect.x + 8.0, y + 17.0, 12.0, text_dim);
            }
        } else {
            for (i, asset) in browser.user_assets.iter().enumerate() {
                let item_rect = Rect::new(rect.x, y, rect.w, item_h);

                if y + item_h > rect.y && y < rect.bottom() {
                    let is_selected = browser.selected_category == Some(AssetCategory::User)
                        && browser.selected_index == Some(i);
                    let is_hovered = ctx.mouse.inside(&item_rect) && item_rect.y >= rect.y;

                    let bg = if is_selected { item_selected }
                            else if is_hovered { item_hover }
                            else { item_bg };

                    // Clip to list bounds
                    let draw_y = item_rect.y.max(rect.y);
                    let draw_h = item_h.min(rect.bottom() - draw_y);
                    if draw_h > 0.0 {
                        draw_rectangle(item_rect.x + 2.0, draw_y, item_rect.w - 4.0, draw_h, bg);

                        if y >= rect.y {
                            draw_text(&asset.name, rect.x + 20.0, y + 17.0, 13.0, text_color);

                            // Cloud icon for user assets when cloud storage is active
                            if has_cloud {
                                draw_text("*", rect.x + rect.w - 20.0, y + 17.0, 13.0, cloud_color);
                            }
                        }
                    }

                    // Handle click
                    if is_hovered && ctx.mouse.left_pressed && item_rect.y >= rect.y {
                        clicked = Some((AssetCategory::User, i));
                    }
                }
                y += item_h;
            }
        }
    }

    clicked
}

/// Draw the orbit preview of an asset (uses browser's internal framebuffer)
fn draw_orbit_preview_internal(
    ctx: &mut UiContext,
    browser: &mut AssetBrowser,
    rect: Rect,
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

    // Resize local preview framebuffer
    let preview_h = rect.h - 24.0;
    let target_w = (rect.w as usize).min(640);
    let target_h = (preview_h as usize).min(target_w * 3 / 4);
    let fb = &mut browser.preview_fb;
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
    storage: &crate::storage::Storage,
    icon_font: Option<&Font>,
) -> AssetBrowserAction {
    draw_asset_browser(ctx, browser, storage, icon_font)
}
