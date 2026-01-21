//! Song browser for loading and previewing saved songs
//!
//! Shows two sections:
//! - SAMPLES: Read-only bundled songs from assets/samples/songs/
//! - MY SONGS: User-created songs from assets/userdata/songs/

use std::path::PathBuf;
use crate::ui::{
    Rect, UiContext, draw_icon_centered,
    BG_COLOR, HEADER_COLOR, TEXT_COLOR, TEXT_DIM, ACCENT_COLOR,
};
use crate::storage::{PendingLoad, PendingList};
use macroquad::prelude::*;
use super::pattern::Song;

/// Close button icon (Lucide circle-x)
const CLOSE_ICON: char = '\u{e084}';

// Button colors matching other browsers
const BTN_BG: Color = Color::new(0.235, 0.235, 0.275, 1.0); // rgba(60, 60, 70, 255)

/// Directory constants
pub const SAMPLES_SONGS_DIR: &str = "assets/samples/songs";
pub const USER_SONGS_DIR: &str = "assets/userdata/songs";

/// Song source/category
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SongCategory {
    /// Bundled sample song (read-only)
    Sample,
    /// User-created song (editable)
    #[default]
    User,
}

/// Information about a song file
#[derive(Debug, Clone)]
pub struct SongInfo {
    /// Display name (filename without extension)
    pub name: String,
    /// Full path to the song file
    pub path: PathBuf,
    /// Source category (sample or user)
    pub category: SongCategory,
}

/// Action returned from the browser
#[derive(Debug, Clone, PartialEq)]
pub enum SongBrowserAction {
    /// No action
    None,
    /// User selected a song for preview (category, index)
    SelectPreview(SongCategory, usize),
    /// User clicked Open to load the selected song
    OpenSong,
    /// User clicked New to create a new song
    NewSong,
    /// User cancelled/closed the browser
    Cancel,
    /// Toggle preview playback
    TogglePreview,
    /// Delete selected user song
    DeleteSong,
    /// Refresh the song list
    Refresh,
}

/// Song browser dialog
pub struct SongBrowser {
    /// Is the browser open?
    pub open: bool,
    /// List of sample songs (read-only)
    pub samples: Vec<SongInfo>,
    /// List of user songs (editable)
    pub user_songs: Vec<SongInfo>,
    /// Currently selected category
    pub selected_category: Option<SongCategory>,
    /// Currently selected song index within category
    pub selected_index: Option<usize>,
    /// Preview of selected song (for stats display)
    pub preview_song: Option<Song>,
    /// Scroll offset for the song list
    pub scroll_offset: f32,
    /// Pending load path for WASM async
    pub pending_load_path: Option<PathBuf>,
    /// Pending list load for WASM async
    pub pending_load_list: bool,
    /// Is preview playing?
    pub preview_playing: bool,
    /// SAMPLES section collapsed
    pub samples_collapsed: bool,
    /// MY SONGS section collapsed
    pub user_collapsed: bool,
    /// Pending async preview load (native cloud storage)
    pub pending_preview_load: Option<PendingLoad>,
    /// Pending async user songs list (native cloud storage)
    pub pending_user_list: Option<PendingList>,
    /// Flag to trigger user songs refresh from main loop
    pub pending_refresh: bool,
}

impl Default for SongBrowser {
    fn default() -> Self {
        Self::new()
    }
}

impl SongBrowser {
    pub fn new() -> Self {
        Self {
            open: false,
            samples: Vec::new(),
            user_songs: Vec::new(),
            selected_category: None,
            selected_index: None,
            preview_song: None,
            scroll_offset: 0.0,
            pending_load_path: None,
            pending_load_list: false,
            preview_playing: false,
            samples_collapsed: false,
            user_collapsed: false,
            pending_preview_load: None,
            pending_user_list: None,
            pending_refresh: false,
        }
    }

    /// Close the browser
    pub fn close(&mut self) {
        self.open = false;
        self.preview_song = None;
        self.preview_playing = false;
        self.pending_preview_load = None;
    }

    /// Check if a preview is currently being loaded
    pub fn is_loading_preview(&self) -> bool {
        self.pending_preview_load.is_some() || self.pending_load_path.is_some()
    }

    /// Check if user songs are being loaded
    pub fn is_loading_user_songs(&self) -> bool {
        self.pending_user_list.is_some()
    }

    /// Open the browser and refresh the song list
    pub fn open(&mut self) {
        self.open = true;
        self.selected_category = None;
        self.selected_index = None;
        self.preview_song = None;
        self.preview_playing = false;
        self.scroll_offset = 0.0;

        #[cfg(not(target_arch = "wasm32"))]
        {
            // Samples are always local
            self.samples = discover_songs_from_dir(SAMPLES_SONGS_DIR, SongCategory::Sample);
            // User songs: set pending_refresh flag so main.rs handles cloud vs local
            // This ensures cloud users get their songs from cloud storage
            self.pending_refresh = true;
        }

        #[cfg(target_arch = "wasm32")]
        {
            self.pending_load_list = true;
        }
    }

    /// Set the preview song
    pub fn set_preview(&mut self, song: Song) {
        self.preview_song = Some(song);
    }

    /// Check if the selected song is a sample (read-only)
    pub fn is_sample_selected(&self) -> bool {
        self.selected_category == Some(SongCategory::Sample)
    }

    /// Check if the selected song is a user song (editable)
    pub fn is_user_selected(&self) -> bool {
        self.selected_category == Some(SongCategory::User)
    }

    /// Get the currently selected song info
    pub fn selected_song(&self) -> Option<&SongInfo> {
        match (self.selected_category, self.selected_index) {
            (Some(SongCategory::Sample), Some(i)) => self.samples.get(i),
            (Some(SongCategory::User), Some(i)) => self.user_songs.get(i),
            _ => None,
        }
    }

    /// Draw the browser and return any action
    pub fn draw(&mut self, ctx: &mut UiContext, screen_rect: Rect, icon_font: Option<&Font>, storage: &crate::storage::Storage) -> SongBrowserAction {
        if !self.open {
            return SongBrowserAction::None;
        }

        let mut action = SongBrowserAction::None;

        // Modal overlay
        draw_rectangle(0.0, 0.0, screen_rect.w, screen_rect.h, Color::new(0.0, 0.0, 0.0, 0.7));

        // Dialog box (centered, ~80% of screen up to 900x600)
        let dialog_w = (screen_rect.w * 0.8).min(900.0);
        let dialog_h = (screen_rect.h * 0.8).min(600.0);
        let dialog_x = (screen_rect.w - dialog_w) / 2.0;
        let dialog_y = (screen_rect.h - dialog_h) / 2.0;
        let dialog_rect = Rect::new(dialog_x, dialog_y, dialog_w, dialog_h);

        draw_rectangle(dialog_rect.x, dialog_rect.y, dialog_rect.w, dialog_rect.h, BG_COLOR);
        draw_rectangle_lines(dialog_rect.x, dialog_rect.y, dialog_rect.w, dialog_rect.h, 2.0, HEADER_COLOR);

        // Header
        let header_h = 40.0;
        draw_rectangle(dialog_rect.x, dialog_rect.y, dialog_rect.w, header_h, HEADER_COLOR);
        draw_text("Song Browser", dialog_rect.x + 12.0, dialog_rect.y + 26.0, 20.0, TEXT_COLOR);

        // Close button
        let close_btn = Rect::new(dialog_rect.x + dialog_rect.w - 36.0, dialog_rect.y + 4.0, 32.0, 32.0);
        if draw_close_button(ctx, close_btn, icon_font) {
            action = SongBrowserAction::Cancel;
        }

        // Content area
        let content_y = dialog_rect.y + header_h + 8.0;
        let content_h = dialog_rect.h - header_h - 60.0; // Leave room for footer

        // Left side: two-section song list
        let list_w = dialog_w * 0.45;
        let list_rect = Rect::new(dialog_rect.x + 8.0, content_y, list_w, content_h);

        // Draw two-section list and handle clicks
        let has_cloud = storage.has_cloud();
        let list_action = draw_two_section_song_list(ctx, list_rect, self, has_cloud);
        if let Some((category, idx)) = list_action.clicked {
            if self.selected_category != Some(category) || self.selected_index != Some(idx) {
                self.selected_category = Some(category);
                self.selected_index = Some(idx);
                action = SongBrowserAction::SelectPreview(category, idx);
            }
        }
        if list_action.double_clicked {
            action = SongBrowserAction::OpenSong;
        }

        // Right side: song info/preview
        let info_x = dialog_rect.x + list_w + 24.0;
        let info_w = dialog_w - list_w - 40.0;
        let info_rect = Rect::new(info_x, content_y, info_w, content_h);

        draw_rectangle(info_rect.x, info_rect.y, info_rect.w, info_rect.h, Color::new(0.1, 0.1, 0.12, 1.0));
        draw_rectangle_lines(info_rect.x, info_rect.y, info_rect.w, info_rect.h, 1.0, HEADER_COLOR);

        if let Some(song) = &self.preview_song {
            let mut y = info_rect.y + 20.0;
            let line_h = 22.0;

            draw_text(&format!("Name: {}", song.name), info_rect.x + 12.0, y, 16.0, TEXT_COLOR);
            y += line_h;

            // Show if sample (read-only)
            if self.is_sample_selected() {
                draw_text("(Sample - Read Only)", info_rect.x + 12.0, y, 14.0, Color::from_rgba(100, 180, 255, 255));
                y += line_h;
            }

            draw_text(&format!("BPM: {}", song.bpm), info_rect.x + 12.0, y, 16.0, TEXT_DIM);
            y += line_h;

            draw_text(&format!("Patterns: {}", song.patterns.len()), info_rect.x + 12.0, y, 16.0, TEXT_DIM);
            y += line_h;

            draw_text(&format!("Arrangement: {} entries", song.arrangement.len()), info_rect.x + 12.0, y, 16.0, TEXT_DIM);
            y += line_h;

            let channels = song.patterns.first().map(|p| p.num_channels()).unwrap_or(0);
            draw_text(&format!("Channels: {}", channels), info_rect.x + 12.0, y, 16.0, TEXT_DIM);
            y += line_h + 8.0;

            // Play/Stop button for preview
            let play_btn_w = 100.0;
            let play_btn = Rect::new(info_rect.x + 12.0, y, play_btn_w, 28.0);
            let play_text = if self.preview_playing { "Stop" } else { "Play" };
            let play_color = if self.preview_playing {
                Color::from_rgba(180, 60, 60, 255)
            } else {
                ACCENT_COLOR
            };
            if draw_text_button(ctx, play_btn, play_text, play_color) {
                action = SongBrowserAction::TogglePreview;
            }
        } else if self.samples.is_empty() && self.user_songs.is_empty() {
            draw_text("No songs found", info_rect.x + 12.0, info_rect.y + 30.0, 16.0, TEXT_DIM);
            draw_text("Click 'New' to create one", info_rect.x + 12.0, info_rect.y + 52.0, 14.0, TEXT_DIM);
        } else {
            draw_text("Select a song", info_rect.x + 12.0, info_rect.y + 30.0, 16.0, TEXT_DIM);
            draw_text("to preview", info_rect.x + 12.0, info_rect.y + 52.0, 14.0, TEXT_DIM);
        }

        // Footer buttons
        let footer_y = dialog_rect.y + dialog_rect.h - 44.0;
        let btn_w = 80.0;
        let btn_h = 32.0;
        let btn_spacing = 12.0;

        // New button (left side)
        let new_btn = Rect::new(dialog_rect.x + 12.0, footer_y, btn_w, btn_h);
        if draw_text_button(ctx, new_btn, "New", BTN_BG) {
            action = SongBrowserAction::NewSong;
        }

        // Delete button (only for user songs)
        let delete_btn = Rect::new(dialog_rect.x + 12.0 + btn_w + btn_spacing, footer_y, btn_w, btn_h);
        let delete_enabled = self.is_user_selected() && self.preview_song.is_some();
        if draw_text_button_enabled(ctx, delete_btn, "Delete", Color::from_rgba(120, 50, 50, 255), delete_enabled) {
            action = SongBrowserAction::DeleteSong;
        }

        // Refresh button
        let refresh_btn = Rect::new(dialog_rect.x + 12.0 + (btn_w + btn_spacing) * 2.0, footer_y, btn_w, btn_h);
        if draw_text_button(ctx, refresh_btn, "Refresh", BTN_BG) {
            action = SongBrowserAction::Refresh;
        }

        // Cancel button (right side)
        let cancel_btn = Rect::new(dialog_rect.x + dialog_rect.w - btn_w - 12.0, footer_y, btn_w, btn_h);
        if draw_text_button(ctx, cancel_btn, "Cancel", BTN_BG) {
            action = SongBrowserAction::Cancel;
        }

        // Open button
        let open_btn = Rect::new(cancel_btn.x - btn_w - btn_spacing, footer_y, btn_w, btn_h);
        let open_enabled = self.selected_index.is_some();
        if draw_text_button_enabled(ctx, open_btn, "Open", ACCENT_COLOR, open_enabled) {
            action = SongBrowserAction::OpenSong;
        }

        // Handle escape key
        if is_key_pressed(KeyCode::Escape) {
            action = SongBrowserAction::Cancel;
        }

        // Handle enter key
        if is_key_pressed(KeyCode::Enter) && self.selected_index.is_some() {
            action = SongBrowserAction::OpenSong;
        }

        // Close on certain actions
        match action {
            SongBrowserAction::OpenSong | SongBrowserAction::NewSong | SongBrowserAction::Cancel | SongBrowserAction::DeleteSong => {
                self.open = false;
            }
            _ => {}
        }

        action
    }
}

/// Result from drawing the two-section list
struct SongListResult {
    clicked: Option<(SongCategory, usize)>,
    double_clicked: bool,
}

/// Draw the two-section song list (SAMPLES + MY SONGS)
fn draw_two_section_song_list(
    ctx: &mut UiContext,
    rect: Rect,
    browser: &mut SongBrowser,
    has_cloud: bool,
) -> SongListResult {
    let mut result = SongListResult {
        clicked: None,
        double_clicked: false,
    };

    let item_h = 26.0;
    let section_h = 28.0;

    let section_bg = Color::from_rgba(40, 40, 50, 255);
    let item_bg = Color::from_rgba(30, 30, 38, 255);
    let item_hover = Color::from_rgba(50, 50, 60, 255);
    let item_selected = Color::from_rgba(60, 80, 120, 255);
    let text_color = Color::from_rgba(200, 200, 200, 255);
    let text_dim = Color::from_rgba(140, 140, 140, 255);

    // Draw list background
    draw_rectangle(rect.x, rect.y, rect.w, rect.h, Color::from_rgba(25, 25, 30, 255));

    // Calculate total content height for scroll
    let samples_content_h = if browser.samples_collapsed { 0.0 } else { browser.samples.len() as f32 * item_h };
    let user_content_h = if browser.user_collapsed { 0.0 } else { browser.user_songs.len() as f32 * item_h };
    let total_h = section_h * 2.0 + samples_content_h + user_content_h;

    // Handle scroll within list bounds
    if ctx.mouse.inside(&rect) && ctx.mouse.scroll != 0.0 {
        browser.scroll_offset = (browser.scroll_offset - ctx.mouse.scroll * 30.0)
            .clamp(0.0, (total_h - rect.h).max(0.0));
    }

    let mut y = rect.y - browser.scroll_offset;

    // SAMPLES section header
    let samples_header_rect = Rect::new(rect.x, y, rect.w, section_h);
    if y + section_h > rect.y && y < rect.bottom() {
        let draw_y = y.max(rect.y);
        let draw_h = section_h.min(rect.bottom() - draw_y);
        draw_rectangle(rect.x, draw_y, rect.w, draw_h, section_bg);

        if y >= rect.y {
            let arrow = if browser.samples_collapsed { ">" } else { "v" };
            draw_text(
                &format!("{} SAMPLE SONGS ({})", arrow, browser.samples.len()),
                rect.x + 8.0,
                y + 18.0,
                14.0,
                text_color,
            );
        }

        // Toggle collapse on click
        if ctx.mouse.inside(&samples_header_rect) && ctx.mouse.left_pressed && samples_header_rect.y >= rect.y {
            browser.samples_collapsed = !browser.samples_collapsed;
        }
    }
    y += section_h;

    // SAMPLES items
    if !browser.samples_collapsed {
        if browser.samples.is_empty() {
            if y + item_h > rect.y && y < rect.bottom() {
                draw_text("  (no sample songs)", rect.x + 8.0, y + 17.0, 12.0, text_dim);
            }
            y += item_h;
        } else {
            for (i, song) in browser.samples.iter().enumerate() {
                let item_rect = Rect::new(rect.x, y, rect.w, item_h);

                if y + item_h > rect.y && y < rect.bottom() {
                    let is_selected = browser.selected_category == Some(SongCategory::Sample)
                        && browser.selected_index == Some(i);
                    let is_hovered = ctx.mouse.inside(&item_rect) && item_rect.y >= rect.y;

                    let bg = if is_selected {
                        item_selected
                    } else if is_hovered {
                        item_hover
                    } else {
                        item_bg
                    };

                    // Clip to list bounds
                    let draw_y = item_rect.y.max(rect.y);
                    let draw_h = item_h.min(rect.bottom() - draw_y);
                    if draw_h > 0.0 {
                        draw_rectangle(item_rect.x + 2.0, draw_y, item_rect.w - 4.0, draw_h, bg);

                        if y >= rect.y {
                            draw_text(&song.name, rect.x + 20.0, y + 17.0, 13.0, text_color);
                        }
                    }

                    // Handle click
                    if is_hovered && ctx.mouse.left_pressed && item_rect.y >= rect.y {
                        result.clicked = Some((SongCategory::Sample, i));
                    }
                    if is_hovered && ctx.mouse.double_clicked && item_rect.y >= rect.y {
                        result.double_clicked = true;
                    }
                }
                y += item_h;
            }
        }
    }

    // MY SONGS section header
    let user_header_rect = Rect::new(rect.x, y, rect.w, section_h);
    if y + section_h > rect.y && y < rect.bottom() {
        let draw_y = y.max(rect.y);
        let draw_h = section_h.min(rect.bottom() - draw_y);
        draw_rectangle(rect.x, draw_y, rect.w, draw_h, section_bg);

        if y >= rect.y {
            let arrow = if browser.user_collapsed { ">" } else { "v" };
            let cloud_indicator = if has_cloud { " [cloud]" } else { "" };
            draw_text(
                &format!("{} MY SONGS ({}){}", arrow, browser.user_songs.len(), cloud_indicator),
                rect.x + 8.0,
                y + 18.0,
                14.0,
                text_color,
            );
        }

        // Toggle collapse on click
        if ctx.mouse.inside(&user_header_rect) && ctx.mouse.left_pressed && user_header_rect.y >= rect.y {
            browser.user_collapsed = !browser.user_collapsed;
        }
    }
    y += section_h;

    // MY SONGS items
    if !browser.user_collapsed {
        if browser.is_loading_user_songs() {
            // Show loading indicator
            if y + item_h > rect.y && y < rect.bottom() {
                let time = get_time() as f32;
                let spinner_chars = ['|', '/', '-', '\\'];
                let spinner_idx = (time * 8.0) as usize % spinner_chars.len();
                let loading_text = format!("  {} Loading...", spinner_chars[spinner_idx]);
                draw_text(&loading_text, rect.x + 8.0, y + 17.0, 12.0, text_dim);
            }
            // y += item_h; // Not needed since this is the last section
        } else if browser.user_songs.is_empty() {
            if y + item_h > rect.y && y < rect.bottom() {
                draw_text("  (no saved songs)", rect.x + 8.0, y + 17.0, 12.0, text_dim);
            }
            // y += item_h; // Not needed since this is the last section
        } else {
            for (i, song) in browser.user_songs.iter().enumerate() {
                let item_rect = Rect::new(rect.x, y, rect.w, item_h);

                if y + item_h > rect.y && y < rect.bottom() {
                    let is_selected = browser.selected_category == Some(SongCategory::User)
                        && browser.selected_index == Some(i);
                    let is_hovered = ctx.mouse.inside(&item_rect) && item_rect.y >= rect.y;

                    let bg = if is_selected {
                        item_selected
                    } else if is_hovered {
                        item_hover
                    } else {
                        item_bg
                    };

                    // Clip to list bounds
                    let draw_y = item_rect.y.max(rect.y);
                    let draw_h = item_h.min(rect.bottom() - draw_y);
                    if draw_h > 0.0 {
                        draw_rectangle(item_rect.x + 2.0, draw_y, item_rect.w - 4.0, draw_h, bg);

                        if y >= rect.y {
                            draw_text(&song.name, rect.x + 20.0, y + 17.0, 13.0, text_color);
                        }
                    }

                    // Handle click
                    if is_hovered && ctx.mouse.left_pressed && item_rect.y >= rect.y {
                        result.clicked = Some((SongCategory::User, i));
                    }
                    if is_hovered && ctx.mouse.double_clicked && item_rect.y >= rect.y {
                        result.double_clicked = true;
                    }
                }
                y += item_h;
            }
        }
    }

    result
}

/// Discover songs from both samples and user directories
///
/// Returns (samples, user_songs) tuple
#[cfg(not(target_arch = "wasm32"))]
pub fn discover_songs() -> (Vec<SongInfo>, Vec<SongInfo>) {
    let samples = discover_songs_from_dir(SAMPLES_SONGS_DIR, SongCategory::Sample);
    let user_songs = discover_songs_from_dir(USER_SONGS_DIR, SongCategory::User);
    (samples, user_songs)
}

/// Discover songs from a specific directory
#[cfg(not(target_arch = "wasm32"))]
pub fn discover_songs_from_dir(dir: &str, category: SongCategory) -> Vec<SongInfo> {
    let songs_dir = std::path::Path::new(dir);
    let mut songs = Vec::new();

    if let Ok(entries) = std::fs::read_dir(songs_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().map(|e| e == "ron").unwrap_or(false) {
                if let Some(stem) = path.file_stem() {
                    songs.push(SongInfo {
                        name: stem.to_string_lossy().to_string(),
                        path,
                        category,
                    });
                }
            }
        }
    }

    // Sort by name
    songs.sort_by(|a, b| a.name.cmp(&b.name));
    songs
}

/// Generate the next available song filename (song_001.ron, song_002.ron, etc.)
///
/// Takes the loaded song lists to check for existing names. This works correctly
/// with cloud storage where songs may not exist locally.
pub fn next_available_song_name(samples: &[SongInfo], user_songs: &[SongInfo]) -> PathBuf {
    let songs_dir = PathBuf::from(USER_SONGS_DIR);
    let _ = std::fs::create_dir_all(&songs_dir);

    let mut highest = 0;

    // Check names from the provided song lists (works with cloud storage)
    for song in samples.iter().chain(user_songs.iter()) {
        if let Some(num_str) = song.name.strip_prefix("song_") {
            if let Ok(num) = num_str.parse::<u32>() {
                highest = highest.max(num);
            }
        }
    }

    let next_num = highest + 1;
    songs_dir.join(format!("song_{:03}.ron", next_num))
}

/// Load song lists from manifests asynchronously (for WASM)
///
/// Returns (samples, user_songs) tuple
pub async fn load_song_list() -> (Vec<SongInfo>, Vec<SongInfo>) {
    let samples = load_song_list_from_dir(SAMPLES_SONGS_DIR, SongCategory::Sample).await;
    let user_songs = load_song_list_from_dir(USER_SONGS_DIR, SongCategory::User).await;
    (samples, user_songs)
}

/// Load song list from a specific directory's manifest (for WASM)
async fn load_song_list_from_dir(dir: &str, category: SongCategory) -> Vec<SongInfo> {
    use macroquad::prelude::*;

    let manifest_path = format!("{}/manifest.txt", dir);
    let manifest = match load_string(&manifest_path).await {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };

    let mut songs = Vec::new();
    for line in manifest.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || !line.ends_with(".ron") {
            continue;
        }
        let name = line.strip_suffix(".ron").unwrap_or(line).to_string();
        let path = PathBuf::from(format!("{}/{}", dir, line));
        songs.push(SongInfo { name, path, category });
    }
    songs
}

/// Load song from path asynchronously (for WASM)
/// Supports both compressed (brotli) and uncompressed RON files
pub async fn load_song_async(path: &PathBuf) -> Option<super::pattern::Song> {
    use macroquad::prelude::*;
    use std::io::Cursor;

    let path_str = path.to_string_lossy().replace('\\', "/");

    // Load as binary to support both compressed and uncompressed
    let bytes = match load_file(&path_str).await {
        Ok(b) => b,
        Err(_) => return None,
    };

    // Detect format: RON files start with '(' or whitespace, brotli is binary
    let is_plain_ron = bytes.first().map(|&b| b == b'(' || b == b' ' || b == b'\n' || b == b'\r' || b == b'\t').unwrap_or(false);

    let contents = if is_plain_ron {
        // Plain RON text
        match String::from_utf8(bytes) {
            Ok(s) => s,
            Err(_) => return None,
        }
    } else {
        // Brotli compressed - decompress first
        let mut decompressed = Vec::new();
        match brotli::BrotliDecompress(&mut Cursor::new(&bytes), &mut decompressed) {
            Ok(_) => match String::from_utf8(decompressed) {
                Ok(s) => s,
                Err(_) => return None,
            },
            Err(_) => return None,
        }
    };

    super::io::load_song_from_str(&contents).ok()
}

/// Draw a close button (X) with icon font
fn draw_close_button(ctx: &mut UiContext, rect: Rect, icon_font: Option<&Font>) -> bool {
    let hovered = ctx.mouse.inside(&rect);
    let clicked = hovered && ctx.mouse.left_pressed;

    if hovered {
        draw_rectangle(rect.x, rect.y, rect.w, rect.h, Color::from_rgba(80, 40, 40, 255));
    }

    // Draw X icon using Lucide font
    draw_icon_centered(icon_font, CLOSE_ICON, &rect, 16.0, WHITE);

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
        Color::new(0.196, 0.196, 0.216, 1.0)
    } else if hovered {
        Color::new(bg_color.r * 1.2, bg_color.g * 1.2, bg_color.b * 1.2, bg_color.a)
    } else {
        bg_color
    };

    draw_rectangle(rect.x, rect.y, rect.w, rect.h, color);

    let text_color = if enabled { WHITE } else { Color::new(0.4, 0.4, 0.4, 1.0) };
    let dims = measure_text(text, None, 14, 1.0);
    let tx = rect.x + (rect.w - dims.width) / 2.0;
    let ty = rect.y + (rect.h + dims.height) / 2.0 - 2.0;
    draw_text(text, tx, ty, 14.0, text_color);

    clicked
}
