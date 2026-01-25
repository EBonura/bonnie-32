//! Level Browser
//!
//! Modal dialog for browsing and previewing levels - both bundled samples
//! and user-created levels from storage.

use macroquad::prelude::*;
use crate::storage::{Storage, PendingLoad, PendingList};
use crate::ui::{Rect, UiContext, draw_icon_centered, ACCENT_COLOR};
use crate::world::Level;
use crate::rasterizer::{Framebuffer, Texture as RasterTexture, Camera, render_mesh, render_mesh_15, Color as RasterColor, Vec3, RasterSettings, Light, ShadingMode, Vertex};
use crate::modeler::checkerboard_clut;
use super::example_levels::{LevelInfo, LevelCategory, LevelStats, get_level_stats};
use super::TexturePack;

/// State for the level browser dialog
pub struct LevelBrowser {
    /// Whether the browser is open
    pub open: bool,
    /// Bundled sample levels (read-only)
    pub samples: Vec<LevelInfo>,
    /// User-created levels (editable)
    pub user_levels: Vec<LevelInfo>,
    /// Whether samples section is collapsed
    pub samples_collapsed: bool,
    /// Whether user levels section is collapsed
    pub user_collapsed: bool,
    /// Currently selected category
    pub selected_category: Option<LevelCategory>,
    /// Currently selected index within category
    pub selected_index: Option<usize>,
    /// Currently loaded preview level
    pub preview_level: Option<Level>,
    /// Stats for the preview level
    pub preview_stats: Option<LevelStats>,
    /// Orbit camera state for preview
    pub orbit_yaw: f32,
    pub orbit_pitch: f32,
    pub orbit_distance: f32,
    pub orbit_center: (f32, f32, f32),
    /// Mouse state for orbit control
    pub dragging: bool,
    pub last_mouse: (f32, f32),
    /// Scroll offset for the list
    pub scroll_offset: f32,
    /// Path pending async load (WASM)
    pub pending_load_path: Option<std::path::PathBuf>,
    /// Whether we need to async load the sample list (WASM)
    pub pending_load_list: bool,
    /// Pending async preview load (native cloud storage)
    pub pending_preview_load: Option<PendingLoad>,
    /// Pending async user level list (native cloud storage)
    pub pending_user_list: Option<PendingList>,
    /// Local framebuffer for preview rendering (avoids resizing main fb)
    preview_fb: Framebuffer,
}

impl Default for LevelBrowser {
    fn default() -> Self {
        Self {
            open: false,
            samples: Vec::new(),
            user_levels: Vec::new(),
            samples_collapsed: false,
            user_collapsed: false,
            selected_category: None,
            selected_index: None,
            preview_level: None,
            preview_stats: None,
            orbit_yaw: 0.5,
            orbit_pitch: 0.4,
            orbit_distance: 4000.0,
            orbit_center: (0.0, 0.0, 0.0),
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

impl LevelBrowser {
    /// Open the browser with sample and user levels
    pub fn open_with_levels(&mut self, samples: Vec<LevelInfo>, user_levels: Vec<LevelInfo>) {
        self.open = true;
        self.samples = samples;
        self.user_levels = user_levels;
        self.selected_category = None;
        self.selected_index = None;
        self.preview_level = None;
        self.preview_stats = None;
        self.scroll_offset = 0.0;
    }

    /// Open the browser with just sample levels (legacy compatibility)
    pub fn open(&mut self, samples: Vec<LevelInfo>) {
        self.open_with_levels(samples, Vec::new());
    }

    /// Close the browser
    pub fn close(&mut self) {
        self.open = false;
        self.preview_level = None;
    }

    /// Get the currently selected level info
    pub fn selected_level(&self) -> Option<&LevelInfo> {
        match (self.selected_category, self.selected_index) {
            (Some(LevelCategory::Sample), Some(i)) => self.samples.get(i),
            (Some(LevelCategory::User), Some(i)) => self.user_levels.get(i),
            _ => None,
        }
    }

    /// Check if the selected level is a sample (read-only)
    pub fn is_sample_selected(&self) -> bool {
        self.selected_category == Some(LevelCategory::Sample)
    }

    /// Check if the selected level is a user level (editable)
    pub fn is_user_selected(&self) -> bool {
        self.selected_category == Some(LevelCategory::User)
    }

    /// Check if a preview is currently being loaded
    pub fn is_loading_preview(&self) -> bool {
        self.pending_preview_load.is_some() || self.pending_load_path.is_some()
    }

    /// Check if user levels are being loaded
    pub fn is_loading_user_levels(&self) -> bool {
        self.pending_user_list.is_some()
    }

    /// Set the preview level (called after async load)
    pub fn set_preview(&mut self, level: Level) {
        use crate::world::SECTOR_SIZE;

        // Calculate bounding box of all rooms to find center
        let mut min_x = f32::MAX;
        let mut max_x = f32::MIN;
        let mut min_y = f32::MAX;
        let mut max_y = f32::MIN;
        let mut min_z = f32::MAX;
        let mut max_z = f32::MIN;

        for room in &level.rooms {
            let room_min_x = room.position.x;
            let room_max_x = room.position.x + (room.width as f32) * SECTOR_SIZE;
            let room_min_z = room.position.z;
            let room_max_z = room.position.z + (room.depth as f32) * SECTOR_SIZE;

            min_x = min_x.min(room_min_x);
            max_x = max_x.max(room_max_x);
            min_z = min_z.min(room_min_z);
            max_z = max_z.max(room_max_z);

            // Check floor/ceiling heights in sectors
            for row in &room.sectors {
                for sector_opt in row {
                    if let Some(sector) = sector_opt {
                        if let Some(floor) = &sector.floor {
                            for h in &floor.heights {
                                min_y = min_y.min(*h);
                                max_y = max_y.max(*h);
                            }
                        }
                        if let Some(ceiling) = &sector.ceiling {
                            for h in &ceiling.heights {
                                min_y = min_y.min(*h);
                                max_y = max_y.max(*h);
                            }
                        }
                    }
                }
            }
        }

        // Default Y range if no geometry found
        if min_y == f32::MAX {
            min_y = 0.0;
            max_y = 0.0;
        }

        // Calculate center
        let center_x = (min_x + max_x) / 2.0;
        let center_y = (min_y + max_y) / 2.0;
        let center_z = (min_z + max_z) / 2.0;
        self.orbit_center = (center_x, center_y, center_z);

        // Set distance based on level size (diagonal of bounding box)
        let size_x = max_x - min_x;
        let size_y = max_y - min_y;
        let size_z = max_z - min_z;
        let diagonal = (size_x * size_x + size_y * size_y + size_z * size_z).sqrt();
        self.orbit_distance = diagonal.max(2000.0) * 1.2;

        self.preview_stats = Some(get_level_stats(&level));
        self.preview_level = Some(level);

        // Reset orbit angle - start looking at level from an angle
        self.orbit_yaw = 0.8;
        self.orbit_pitch = 0.4;
    }

    /// Get the currently selected example info (legacy compatibility)
    pub fn selected_example(&self) -> Option<&LevelInfo> {
        self.selected_level()
    }
}

/// Result from drawing the level browser
#[derive(Debug, Clone, PartialEq)]
pub enum BrowserAction {
    None,
    /// User selected a level to preview (need to load it async)
    SelectPreview(LevelCategory, usize),
    /// User wants to open the selected level
    OpenLevel,
    /// User wants to create a copy of the sample as a user level
    OpenCopy,
    /// User wants to delete the selected user level
    DeleteLevel,
    /// User wants to start with a new empty level
    NewLevel,
    /// User wants to refresh the level list
    Refresh,
    /// User cancelled
    Cancel,
}

/// Draw the level browser modal dialog
pub fn draw_level_browser(
    ctx: &mut UiContext,
    browser: &mut LevelBrowser,
    storage: &Storage,
    icon_font: Option<&Font>,
    texture_packs: &[TexturePack],
    asset_library: &crate::asset::AssetLibrary,
) -> BrowserAction {
    if !browser.open {
        return BrowserAction::None;
    }

    let mut action = BrowserAction::None;

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
    draw_text("Level Browser", dialog_x + 16.0, dialog_y + 26.0, 20.0, WHITE);

    // Close button
    let close_rect = Rect::new(dialog_x + dialog_w - 36.0, dialog_y + 4.0, 32.0, 32.0);
    if draw_close_button(ctx, close_rect, icon_font) {
        action = BrowserAction::Cancel;
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
            action = BrowserAction::SelectPreview(category, idx);
        }
    }

    // Preview panel (right)
    let preview_x = dialog_x + list_w + 16.0;
    let preview_w = dialog_w - list_w - 24.0;
    let preview_rect = Rect::new(preview_x, content_y, preview_w, content_h);

    draw_rectangle(preview_rect.x, preview_rect.y, preview_rect.w, preview_rect.h, Color::from_rgba(20, 20, 25, 255));

    // Draw preview content
    let has_preview = browser.preview_level.is_some();
    let has_selection = browser.selected_category.is_some();

    if has_preview {
        // Render 3D preview with orbit camera (uses browser's local framebuffer)
        draw_orbit_preview(ctx, browser, preview_rect, texture_packs, asset_library);

        // Draw stats at bottom of preview
        if let Some(stats) = &browser.preview_stats {
            let stats_y = preview_rect.bottom() - 24.0;
            draw_rectangle(preview_rect.x, stats_y, preview_rect.w, 24.0, Color::from_rgba(30, 30, 35, 200));
            let stats_text = format!(
                "Rooms: {}  Sectors: {}  Floors: {}  Walls: {}",
                stats.room_count, stats.sector_count, stats.floor_count, stats.wall_count
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
        draw_text("Select a level to preview", preview_rect.x + 20.0, preview_rect.y + 40.0, 16.0, Color::from_rgba(100, 100, 100, 255));
    }

    // Footer with buttons
    let footer_y = dialog_y + dialog_h - 44.0;
    draw_rectangle(dialog_x, footer_y, dialog_w, 44.0, Color::from_rgba(40, 40, 48, 255));

    // New button (left side) - start with empty level
    let new_rect = Rect::new(dialog_x + 10.0, footer_y + 8.0, 70.0, 28.0);
    if draw_text_button(ctx, new_rect, "New", Color::from_rgba(60, 60, 70, 255)) {
        action = BrowserAction::NewLevel;
    }

    // Delete button (only for user levels)
    let delete_rect = Rect::new(dialog_x + 90.0, footer_y + 8.0, 70.0, 28.0);
    let delete_enabled = browser.is_user_selected() && browser.preview_level.is_some();
    if draw_text_button_enabled(ctx, delete_rect, "Delete", Color::from_rgba(120, 50, 50, 255), delete_enabled) {
        action = BrowserAction::DeleteLevel;
    }

    // Refresh button - reload level lists from storage
    let refresh_rect = Rect::new(dialog_x + 170.0, footer_y + 8.0, 70.0, 28.0);
    if draw_text_button(ctx, refresh_rect, "Refresh", Color::from_rgba(60, 60, 70, 255)) {
        action = BrowserAction::Refresh;
    }

    // Cancel button
    let cancel_rect = Rect::new(dialog_x + dialog_w - 270.0, footer_y + 8.0, 70.0, 28.0);
    if draw_text_button(ctx, cancel_rect, "Cancel", Color::from_rgba(60, 60, 70, 255)) {
        action = BrowserAction::Cancel;
    }

    // Open Copy button (only for samples - copies sample to user level)
    let copy_rect = Rect::new(dialog_x + dialog_w - 190.0, footer_y + 8.0, 90.0, 28.0);
    let copy_enabled = browser.is_sample_selected() && browser.preview_level.is_some() && storage.can_write();
    if draw_text_button_enabled(ctx, copy_rect, "Open Copy", Color::from_rgba(60, 80, 60, 255), copy_enabled) {
        action = BrowserAction::OpenCopy;
    }

    // Open button (enabled if something is selected and loaded)
    let open_rect = Rect::new(dialog_x + dialog_w - 90.0, footer_y + 8.0, 80.0, 28.0);
    let open_enabled = browser.preview_level.is_some();
    if draw_text_button_enabled(ctx, open_rect, "Open", ACCENT_COLOR, open_enabled) {
        action = BrowserAction::OpenLevel;
    }

    // Handle Escape to close
    if is_key_pressed(KeyCode::Escape) {
        action = BrowserAction::Cancel;
    }

    action
}

/// Legacy alias for backward compatibility
pub fn draw_example_browser(
    ctx: &mut UiContext,
    browser: &mut LevelBrowser,
    icon_font: Option<&Font>,
    texture_packs: &[TexturePack],
    asset_library: &crate::asset::AssetLibrary,
) -> BrowserAction {
    // Create a temporary storage for legacy calls (local-only)
    let storage = Storage::new();
    draw_level_browser(ctx, browser, &storage, icon_font, texture_packs, asset_library)
}

/// Draw the two-section list (Samples + My Levels)
fn draw_two_section_list(
    ctx: &mut UiContext,
    rect: Rect,
    browser: &mut LevelBrowser,
    item_h: f32,
    section_h: f32,
    has_cloud: bool,
) -> Option<(LevelCategory, usize)> {
    let mut clicked: Option<(LevelCategory, usize)> = None;
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
    let user_content_h = if browser.user_collapsed { 0.0 } else { browser.user_levels.len() as f32 * item_h };
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
        draw_text(&format!("{} SAMPLE LEVELS ({})", arrow, browser.samples.len()),
                 rect.x + 8.0, y + 18.0, 14.0, text_color);

        // Toggle collapse on click
        if ctx.mouse.inside(&samples_header_rect) && ctx.mouse.left_pressed {
            browser.samples_collapsed = !browser.samples_collapsed;
        }
    }
    y += section_h;

    // SAMPLES items
    if !browser.samples_collapsed {
        for (i, level) in browser.samples.iter().enumerate() {
            let item_rect = Rect::new(rect.x, y, rect.w, item_h);

            if y + item_h > rect.y && y < rect.bottom() {
                let is_selected = browser.selected_category == Some(LevelCategory::Sample)
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
                        draw_text(&level.name, rect.x + 20.0, y + 17.0, 13.0, text_color);
                    }
                }

                // Handle click
                if is_hovered && ctx.mouse.left_pressed && item_rect.y >= rect.y {
                    clicked = Some((LevelCategory::Sample, i));
                }
            }
            y += item_h;
        }
    }

    // MY LEVELS section header
    let user_header_rect = Rect::new(rect.x, y, rect.w, section_h);
    if y + section_h > rect.y && y < rect.bottom() {
        draw_rectangle(user_header_rect.x, user_header_rect.y.max(rect.y),
                      user_header_rect.w, section_h.min(rect.bottom() - user_header_rect.y.max(rect.y)), section_bg);

        let arrow = if browser.user_collapsed { ">" } else { "v" };
        let cloud_indicator = if has_cloud { " [cloud]" } else { "" };
        draw_text(&format!("{} MY LEVELS ({}){}", arrow, browser.user_levels.len(), cloud_indicator),
                 rect.x + 8.0, y + 18.0, 14.0, text_color);

        // Toggle collapse on click
        if ctx.mouse.inside(&user_header_rect) && ctx.mouse.left_pressed && user_header_rect.y >= rect.y {
            browser.user_collapsed = !browser.user_collapsed;
        }
    }
    y += section_h;

    // MY LEVELS items
    if !browser.user_collapsed {
        if browser.is_loading_user_levels() {
            // Show loading indicator
            if y + item_h > rect.y && y < rect.bottom() {
                let time = get_time() as f32;
                let spinner_chars = ['|', '/', '-', '\\'];
                let spinner_idx = (time * 8.0) as usize % spinner_chars.len();
                let loading_text = format!("  {} Loading...", spinner_chars[spinner_idx]);
                draw_text(&loading_text, rect.x + 8.0, y + 17.0, 12.0, text_dim);
            }
        } else if browser.user_levels.is_empty() {
            // Show empty state message
            if y + item_h > rect.y && y < rect.bottom() {
                draw_text("  (no saved levels)", rect.x + 8.0, y + 17.0, 12.0, text_dim);
            }
        } else {
            for (i, level) in browser.user_levels.iter().enumerate() {
                let item_rect = Rect::new(rect.x, y, rect.w, item_h);

                if y + item_h > rect.y && y < rect.bottom() {
                    let is_selected = browser.selected_category == Some(LevelCategory::User)
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
                            draw_text(&level.name, rect.x + 20.0, y + 17.0, 13.0, text_color);

                            // Cloud icon for user levels when cloud storage is active
                            if has_cloud {
                                draw_text("*", rect.x + rect.w - 20.0, y + 17.0, 13.0, cloud_color);
                            }
                        }
                    }

                    // Handle click
                    if is_hovered && ctx.mouse.left_pressed && item_rect.y >= rect.y {
                        clicked = Some((LevelCategory::User, i));
                    }
                }
                y += item_h;
            }
        }
    }

    clicked
}

/// Draw the orbit preview of a level (uses browser's local framebuffer)
fn draw_orbit_preview(
    ctx: &mut UiContext,
    browser: &mut LevelBrowser,
    rect: Rect,
    texture_packs: &[TexturePack],
    asset_library: &crate::asset::AssetLibrary,
) {
    use crate::rasterizer::WIDTH;

    // Get the level from browser (we know it exists from the caller check)
    let level = match &browser.preview_level {
        Some(l) => l,
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

        // Scroll to zoom
        if ctx.mouse.scroll != 0.0 {
            browser.orbit_distance = (browser.orbit_distance - ctx.mouse.scroll * 100.0).clamp(500.0, 20000.0);
        }
    } else {
        browser.dragging = false;
    }

    // Calculate camera position from orbit using spherical coordinates
    // yaw = horizontal angle around Y axis, pitch = vertical angle from horizontal
    let (cx, cy, cz) = browser.orbit_center;

    // Spherical to Cartesian (offset from center):
    // We place the camera at a distance from the center, then look back at it
    let cos_pitch = browser.orbit_pitch.cos();
    let sin_pitch = browser.orbit_pitch.sin();
    let cos_yaw = browser.orbit_yaw.cos();
    let sin_yaw = browser.orbit_yaw.sin();

    // Camera position: offset from center in spherical coordinates
    let offset_x = browser.orbit_distance * cos_pitch * sin_yaw;
    let offset_y = browser.orbit_distance * sin_pitch;
    let offset_z = browser.orbit_distance * cos_pitch * cos_yaw;

    let cam_x = cx + offset_x;
    let cam_y = cy + offset_y;
    let cam_z = cz + offset_z;

    // Create camera
    let mut camera = Camera::new();
    camera.position = Vec3::new(cam_x, cam_y, cam_z);

    // Direction FROM camera TO center (what we want to look at)
    let dir_x = cx - cam_x;  // = -offset_x
    let dir_y = cy - cam_y;  // = -offset_y
    let dir_z = cz - cam_z;  // = -offset_z

    // Calculate rotation angles from direction vector
    // The camera's basis_z formula is:
    //   x = cos(rotation_x) * sin(rotation_y)
    //   y = -sin(rotation_x)
    //   z = cos(rotation_x) * cos(rotation_y)
    //
    // From direction (dir_x, dir_y, dir_z), normalize first
    let len = (dir_x * dir_x + dir_y * dir_y + dir_z * dir_z).sqrt();
    let nx = dir_x / len;
    let ny = dir_y / len;
    let nz = dir_z / len;

    // rotation_x (pitch): from y = -sin(rotation_x), so rotation_x = -asin(y)
    // Note: we negate because the camera convention has -sin for y
    camera.rotation_x = (-ny).asin();

    // rotation_y (yaw): from x/z = sin(rotation_y)/cos(rotation_y) = tan(rotation_y)
    // So rotation_y = atan2(x, z) but we need to account for cos(rotation_x)
    // At rotation_x=0: x = sin(rotation_y), z = cos(rotation_y)
    // So rotation_y = atan2(x, z)
    camera.rotation_y = nx.atan2(nz);

    camera.update_basis();

    // Resize local preview framebuffer to fit preview area while maintaining aspect
    let preview_h = rect.h - 24.0; // Leave room for stats bar
    let target_w = (rect.w as usize).min(WIDTH * 2);
    let target_h = (preview_h as usize).min(target_w * 3 / 4); // Maintain roughly 4:3
    let fb = &mut browser.preview_fb;
    fb.resize(target_w, target_h);
    fb.clear(RasterColor::new(15, 15, 20));

    // Build lighting from level
    let mut lights = Vec::new();
    let mut total_ambient = 0.0;
    let mut room_count = 0;
    for room in &level.rooms {
        total_ambient += room.ambient;
        room_count += 1;
    }
    // Collect lights from room objects (any asset with Light component)
    for room in &level.rooms {
        for obj in room.objects.iter().filter(|o| {
            o.enabled && asset_library.get_by_id(o.asset_id)
                .map(|a| a.has_light())
                .unwrap_or(false)
        }) {
            let world_pos = obj.world_position(room);
            // Use default light settings
            let light = Light::point(world_pos, 5000.0, 1.0);
            lights.push(light);
        }
    }
    let ambient = if room_count > 0 { total_ambient / room_count as f32 } else { 0.5 };

    // Render settings with Gouraud shading and room lights
    let settings = RasterSettings {
        shading: ShadingMode::Gouraud,
        lights,
        ambient,
        ..RasterSettings::default()
    };

    // Build flattened textures array and texture map (same as main viewport)
    let textures: Vec<RasterTexture> = texture_packs
        .iter()
        .flat_map(|pack| &pack.textures)
        .cloned()
        .collect();

    // Maps (pack, name) -> (texture_idx, texture_width)
    let mut texture_map: std::collections::HashMap<(String, String), (usize, u32)> = std::collections::HashMap::new();
    let mut texture_idx = 0;
    for pack in texture_packs {
        for tex in &pack.textures {
            texture_map.insert((pack.name.clone(), tex.name.clone()), (texture_idx, 64)); // Pack textures are 64x64
            texture_idx += 1;
        }
    }

    // Texture resolver - returns (texture_id, texture_width)
    let resolve_texture = |tex_ref: &crate::world::TextureRef| -> Option<(usize, u32)> {
        if !tex_ref.is_valid() {
            return Some((0, 64)); // Fallback to first texture with default 64x64 size
        }
        texture_map.get(&(tex_ref.pack.clone(), tex_ref.name.clone())).copied()
    };

    // Convert textures to RGB555 if enabled
    let use_rgb555 = settings.use_rgb555;
    let textures_15: Vec<_> = if use_rgb555 {
        textures.iter().map(|t| t.to_15()).collect()
    } else {
        Vec::new()
    };

    // Render each room using the same method as the main viewport
    for room in &level.rooms {
        let (vertices, faces) = room.to_render_data_with_textures(&resolve_texture);
        if !vertices.is_empty() {
            if use_rgb555 {
                render_mesh_15(fb, &vertices, &faces, &textures_15, None, &camera, &settings, None);
            } else {
                render_mesh(fb, &vertices, &faces, &textures, &camera, &settings);
            }
        }
    }

    // Render asset meshes placed in each room
    let fallback_clut = checkerboard_clut();
    for room in &level.rooms {
        for obj in &room.objects {
            if !obj.enabled {
                continue;
            }

            // Get asset from library
            let asset = match asset_library.get_by_id(obj.asset_id) {
                Some(a) => a,
                None => continue,
            };

            // Get mesh parts from asset
            let mesh_parts = match asset.mesh() {
                Some(parts) => parts,
                None => continue,
            };

            // Calculate world transform
            let world_pos = obj.world_position(room);
            let facing = obj.facing;
            let cos_f = facing.cos();
            let sin_f = facing.sin();

            // Per-room render settings
            let asset_settings = RasterSettings {
                lights: settings.lights.clone(),
                ambient,
                ..settings.clone()
            };

            // Render each visible mesh part
            for part in mesh_parts.iter().filter(|p| p.visible) {
                let (local_vertices, faces) = part.mesh.to_render_data_textured();
                if local_vertices.is_empty() {
                    continue;
                }

                // Transform vertices: rotate around Y by facing, then translate
                let transformed_vertices: Vec<Vertex> = local_vertices.iter().map(|v| {
                    let rx = v.pos.x * cos_f - v.pos.z * sin_f;
                    let rz = v.pos.x * sin_f + v.pos.z * cos_f;
                    Vertex {
                        pos: Vec3::new(rx + world_pos.x, v.pos.y + world_pos.y, rz + world_pos.z),
                        uv: v.uv,
                        normal: Vec3::new(
                            v.normal.x * cos_f - v.normal.z * sin_f,
                            v.normal.y,
                            v.normal.x * sin_f + v.normal.z * cos_f,
                        ),
                        color: v.color,
                        bone_index: v.bone_index,
                    }
                }).collect();

                // Use part atlas with fallback clut for preview
                if use_rgb555 {
                    let tex15 = part.atlas.to_texture15(fallback_clut, "preview_asset");
                    let part_textures = [tex15];
                    render_mesh_15(fb, &transformed_vertices, &faces, &part_textures, None, &camera, &asset_settings, None);
                } else {
                    let tex = part.atlas.to_raster_texture(fallback_clut, "preview_asset");
                    let part_textures = [tex];
                    render_mesh(fb, &transformed_vertices, &faces, &part_textures, &camera, &asset_settings);
                }
            }
        }
    }

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
