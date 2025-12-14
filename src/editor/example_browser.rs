//! Example Level Browser
//!
//! Modal dialog for browsing and previewing bundled example levels.

use macroquad::prelude::*;
use crate::ui::{Rect, UiContext, draw_icon_centered, draw_scrollable_list, ACCENT_COLOR};
use crate::world::Level;
use crate::rasterizer::{Framebuffer, Texture as RasterTexture, Camera, render_mesh, Color as RasterColor, Vec3, RasterSettings};
use super::example_levels::{ExampleLevelInfo, LevelStats, get_level_stats};
use super::TexturePack;

/// State for the example browser dialog
pub struct ExampleBrowser {
    /// Whether the browser is open
    pub open: bool,
    /// List of available example levels
    pub examples: Vec<ExampleLevelInfo>,
    /// Currently selected index
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
}

impl Default for ExampleBrowser {
    fn default() -> Self {
        Self {
            open: false,
            examples: Vec::new(),
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
        }
    }
}

impl ExampleBrowser {
    /// Open the browser with the given list of examples
    pub fn open(&mut self, examples: Vec<ExampleLevelInfo>) {
        self.open = true;
        self.examples = examples;
        self.selected_index = None;
        self.preview_level = None;
        self.preview_stats = None;
        self.scroll_offset = 0.0;
    }

    /// Close the browser
    pub fn close(&mut self) {
        self.open = false;
        self.preview_level = None;
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

    /// Get the currently selected example info
    pub fn selected_example(&self) -> Option<&ExampleLevelInfo> {
        self.selected_index.and_then(|i| self.examples.get(i))
    }
}

/// Result from drawing the example browser
#[derive(Debug, Clone, PartialEq)]
pub enum BrowserAction {
    None,
    /// User selected a level to preview (need to load it async)
    SelectPreview(usize),
    /// User wants to open the selected level
    OpenLevel,
    /// User wants to start with a new empty level
    NewLevel,
    /// User cancelled
    Cancel,
}

/// Draw the example browser modal dialog
pub fn draw_example_browser(
    ctx: &mut UiContext,
    browser: &mut ExampleBrowser,
    icon_font: Option<&Font>,
    texture_packs: &[TexturePack],
    fb: &mut Framebuffer,
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
    let dialog_rect = Rect::new(dialog_x, dialog_y, dialog_w, dialog_h);

    // Draw dialog background
    draw_rectangle(dialog_x, dialog_y, dialog_w, dialog_h, Color::from_rgba(35, 35, 40, 255));
    draw_rectangle_lines(dialog_x, dialog_y, dialog_w, dialog_h, 2.0, Color::from_rgba(60, 60, 70, 255));

    // Header
    let header_h = 40.0;
    draw_rectangle(dialog_x, dialog_y, dialog_w, header_h, Color::from_rgba(45, 45, 55, 255));
    draw_text("Browse Levels", dialog_x + 16.0, dialog_y + 26.0, 20.0, WHITE);

    // Close button
    let close_rect = Rect::new(dialog_x + dialog_w - 36.0, dialog_y + 4.0, 32.0, 32.0);
    if draw_close_button(ctx, close_rect, icon_font) {
        action = BrowserAction::Cancel;
    }

    // Content area
    let content_y = dialog_y + header_h + 8.0;
    let content_h = dialog_h - header_h - 60.0; // Leave room for footer
    let list_w = 200.0;

    // List panel (left) - use reusable scrollable list widget
    let list_rect = Rect::new(dialog_x + 8.0, content_y, list_w, content_h);
    let item_h = 28.0;

    // Convert examples to string labels
    let items: Vec<String> = browser.examples.iter().map(|e| e.name.clone()).collect();

    let list_result = draw_scrollable_list(
        ctx,
        list_rect,
        &items,
        browser.selected_index,
        &mut browser.scroll_offset,
        item_h,
        None, // Use default colors
    );

    // Handle list click
    if let Some(clicked_idx) = list_result.clicked {
        if browser.selected_index != Some(clicked_idx) {
            browser.selected_index = Some(clicked_idx);
            action = BrowserAction::SelectPreview(clicked_idx);
        }
    }

    // Preview panel (right)
    let preview_x = dialog_x + list_w + 16.0;
    let preview_w = dialog_w - list_w - 24.0;
    let preview_rect = Rect::new(preview_x, content_y, preview_w, content_h);

    draw_rectangle(preview_rect.x, preview_rect.y, preview_rect.w, preview_rect.h, Color::from_rgba(20, 20, 25, 255));

    // Draw preview content
    let has_preview = browser.preview_level.is_some();
    let has_selection = browser.selected_index.is_some();

    if has_preview {
        // Render 3D preview with orbit camera
        draw_orbit_preview(ctx, browser, preview_rect, texture_packs, fb);

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
    } else if has_selection {
        // Loading indicator
        draw_text("Loading preview...", preview_rect.x + 20.0, preview_rect.y + 40.0, 16.0, Color::from_rgba(150, 150, 150, 255));
    } else {
        // No selection
        draw_text("Select a level to preview", preview_rect.x + 20.0, preview_rect.y + 40.0, 16.0, Color::from_rgba(100, 100, 100, 255));
    }

    // Footer with buttons
    let footer_y = dialog_y + dialog_h - 44.0;
    draw_rectangle(dialog_x, footer_y, dialog_w, 44.0, Color::from_rgba(40, 40, 48, 255));

    // New button (left side) - start with empty level
    let new_rect = Rect::new(dialog_x + 10.0, footer_y + 8.0, 80.0, 28.0);
    if draw_text_button(ctx, new_rect, "New", Color::from_rgba(60, 60, 70, 255)) {
        action = BrowserAction::NewLevel;
    }

    // Cancel button
    let cancel_rect = Rect::new(dialog_x + dialog_w - 180.0, footer_y + 8.0, 80.0, 28.0);
    if draw_text_button(ctx, cancel_rect, "Cancel", Color::from_rgba(60, 60, 70, 255)) {
        action = BrowserAction::Cancel;
    }

    // Open button (only enabled if something is selected)
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

/// Draw the orbit preview of a level
fn draw_orbit_preview(
    ctx: &mut UiContext,
    browser: &mut ExampleBrowser,
    rect: Rect,
    texture_packs: &[TexturePack],
    fb: &mut Framebuffer,
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
        let scroll = mouse_wheel().1;
        if scroll != 0.0 {
            browser.orbit_distance = (browser.orbit_distance - scroll * 100.0).clamp(500.0, 20000.0);
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

    // Resize framebuffer to fit preview area while maintaining aspect
    let preview_h = rect.h - 24.0; // Leave room for stats bar
    let target_w = (rect.w as usize).min(WIDTH * 2);
    let target_h = (preview_h as usize).min(target_w * 3 / 4); // Maintain roughly 4:3
    fb.resize(target_w, target_h);
    fb.clear(RasterColor::new(15, 15, 20));

    // Render settings
    let settings = RasterSettings::default();

    // Build flattened textures array and texture map (same as main viewport)
    let textures: Vec<RasterTexture> = texture_packs
        .iter()
        .flat_map(|pack| &pack.textures)
        .cloned()
        .collect();

    let mut texture_map: std::collections::HashMap<(String, String), usize> = std::collections::HashMap::new();
    let mut texture_idx = 0;
    for pack in texture_packs {
        for tex in &pack.textures {
            texture_map.insert((pack.name.clone(), tex.name.clone()), texture_idx);
            texture_idx += 1;
        }
    }

    let resolve_texture = |tex_ref: &crate::world::TextureRef| -> Option<usize> {
        if !tex_ref.is_valid() {
            return Some(0); // Fallback to first texture
        }
        texture_map.get(&(tex_ref.pack.clone(), tex_ref.name.clone())).copied()
    };

    // Render each room using the same method as the main viewport
    for room in &level.rooms {
        let (vertices, faces) = room.to_render_data_with_textures(&resolve_texture);
        if !vertices.is_empty() {
            render_mesh(fb, &vertices, &faces, &textures, &camera, &settings);
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

    // Draw X icon
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
