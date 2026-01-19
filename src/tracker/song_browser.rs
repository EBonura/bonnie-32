//! Song browser for loading and previewing saved songs

use std::path::PathBuf;
use crate::ui::{
    Rect, UiContext, draw_scrollable_list, draw_icon_centered,
    BG_COLOR, HEADER_COLOR, TEXT_COLOR, TEXT_DIM, ACCENT_COLOR,
};
use macroquad::prelude::*;
use super::pattern::Song;

/// Close button icon (Lucide circle-x)
const CLOSE_ICON: char = '\u{e084}';

// Button colors matching other browsers
const BTN_BG: Color = Color::new(0.235, 0.235, 0.275, 1.0); // rgba(60, 60, 70, 255)

/// Information about a song file
#[derive(Debug, Clone)]
pub struct SongInfo {
    /// Display name (filename without extension)
    pub name: String,
    /// Full path to the song file
    pub path: PathBuf,
}

/// Action returned from the browser
#[derive(Debug, Clone, PartialEq)]
pub enum SongBrowserAction {
    /// No action
    None,
    /// User selected a song for preview
    SelectPreview(usize),
    /// User clicked Open to load the selected song
    OpenSong,
    /// User clicked New to create a new song
    NewSong,
    /// User cancelled/closed the browser
    Cancel,
    /// Toggle preview playback
    TogglePreview,
}

/// Song browser dialog
pub struct SongBrowser {
    /// Is the browser open?
    pub open: bool,
    /// List of discovered songs
    pub songs: Vec<SongInfo>,
    /// Currently selected song index
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
            songs: Vec::new(),
            selected_index: None,
            preview_song: None,
            scroll_offset: 0.0,
            pending_load_path: None,
            pending_load_list: false,
            preview_playing: false,
        }
    }

    /// Close the browser
    pub fn close(&mut self) {
        self.open = false;
        self.preview_song = None;
        self.preview_playing = false;
    }

    /// Open the browser and refresh the song list
    pub fn open(&mut self) {
        self.open = true;
        self.selected_index = None;
        self.preview_song = None;
        self.preview_playing = false;
        self.scroll_offset = 0.0;

        #[cfg(not(target_arch = "wasm32"))]
        {
            self.songs = discover_songs();
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

    /// Draw the browser and return any action
    pub fn draw(&mut self, ctx: &mut UiContext, screen_rect: Rect, icon_font: Option<&Font>) -> SongBrowserAction {
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
        draw_text("Open Song", dialog_rect.x + 12.0, dialog_rect.y + 26.0, 20.0, TEXT_COLOR);

        // Close button
        let close_btn = Rect::new(dialog_rect.x + dialog_rect.w - 36.0, dialog_rect.y + 4.0, 32.0, 32.0);
        if draw_close_button(ctx, close_btn, icon_font) {
            action = SongBrowserAction::Cancel;
        }

        // Content area
        let content_y = dialog_rect.y + header_h + 8.0;
        let content_h = dialog_rect.h - header_h - 60.0; // Leave room for footer

        // Left side: song list
        let list_w = dialog_w * 0.45;
        let list_rect = Rect::new(dialog_rect.x + 8.0, content_y, list_w, content_h);

        let items: Vec<String> = self.songs.iter().map(|s| s.name.clone()).collect();
        let item_h = 24.0;

        let list_result = draw_scrollable_list(
            ctx,
            list_rect,
            &items,
            self.selected_index,
            &mut self.scroll_offset,
            item_h,
            None,
        );

        if let Some(clicked_idx) = list_result.clicked {
            if self.selected_index != Some(clicked_idx) {
                self.selected_index = Some(clicked_idx);
                action = SongBrowserAction::SelectPreview(clicked_idx);
            }
        }

        if list_result.double_clicked.is_some() {
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
        } else if self.songs.is_empty() {
            draw_text("No songs found", info_rect.x + 12.0, info_rect.y + 30.0, 16.0, TEXT_DIM);
            draw_text("in assets/userdata/songs/", info_rect.x + 12.0, info_rect.y + 52.0, 14.0, TEXT_DIM);
        } else {
            draw_text("Select a song", info_rect.x + 12.0, info_rect.y + 30.0, 16.0, TEXT_DIM);
            draw_text("to preview", info_rect.x + 12.0, info_rect.y + 52.0, 14.0, TEXT_DIM);
        }

        // Footer buttons
        let footer_y = dialog_rect.y + dialog_rect.h - 44.0;
        let btn_w = 80.0;
        let btn_h = 32.0;
        let btn_spacing = 12.0;

        // New button
        let new_btn = Rect::new(dialog_rect.x + 12.0, footer_y, btn_w, btn_h);
        if draw_text_button(ctx, new_btn, "New", BTN_BG) {
            action = SongBrowserAction::NewSong;
        }

        // Cancel button
        let cancel_btn = Rect::new(dialog_rect.x + dialog_rect.w - btn_w - 12.0, footer_y, btn_w, btn_h);
        if draw_text_button(ctx, cancel_btn, "Cancel", BTN_BG) {
            action = SongBrowserAction::Cancel;
        }

        // Open button (only enabled if selection)
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
            SongBrowserAction::OpenSong | SongBrowserAction::NewSong | SongBrowserAction::Cancel => {
                self.open = false;
            }
            _ => {}
        }

        action
    }
}

/// Discover songs in the assets/userdata/songs/ directory
#[cfg(not(target_arch = "wasm32"))]
pub fn discover_songs() -> Vec<SongInfo> {
    let songs_dir = std::path::Path::new("assets/userdata/songs");
    let mut songs = Vec::new();

    if let Ok(entries) = std::fs::read_dir(songs_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().map(|e| e == "ron").unwrap_or(false) {
                if let Some(stem) = path.file_stem() {
                    songs.push(SongInfo {
                        name: stem.to_string_lossy().to_string(),
                        path,
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
pub fn next_available_song_name() -> PathBuf {
    let songs_dir = PathBuf::from("assets/userdata/songs");
    let _ = std::fs::create_dir_all(&songs_dir);

    let mut highest = 0;

    #[cfg(not(target_arch = "wasm32"))]
    if let Ok(entries) = std::fs::read_dir(&songs_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                if let Some(num_str) = stem.strip_prefix("song_") {
                    if let Ok(num) = num_str.parse::<u32>() {
                        highest = highest.max(num);
                    }
                }
            }
        }
    }

    let next_num = highest + 1;
    songs_dir.join(format!("song_{:03}.ron", next_num))
}

/// Load song list from manifest asynchronously (for WASM)
pub async fn load_song_list() -> Vec<SongInfo> {
    use macroquad::prelude::*;

    let manifest = match load_string("assets/userdata/songs/manifest.txt").await {
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
        let path = PathBuf::from(format!("assets/userdata/songs/{}", line));
        songs.push(SongInfo { name, path });
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
