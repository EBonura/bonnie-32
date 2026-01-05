//! Tracker UI layout and rendering

use macroquad::prelude::*;
use crate::ui::{
    Rect, UiContext, Toolbar, icon, draw_knob, draw_mini_knob,
    // Theme colors
    BG_COLOR, HEADER_COLOR, TEXT_COLOR, TEXT_DIM,
    ROW_EVEN, ROW_ODD, ROW_BEAT, ROW_HIGHLIGHT,
    CURSOR_COLOR, PLAYBACK_ROW_COLOR,
    NOTE_COLOR, INST_COLOR, VOL_COLOR, FX_COLOR,
};
use super::state::{TrackerState, TrackerView};
use super::psx_reverb::ReverbType;
use super::actions::build_context;
use super::song_browser::{SongBrowserAction, next_available_song_name};

// Layout constants
const ROW_HEIGHT: f32 = 18.0;
const CHANNEL_WIDTH: f32 = 124.0; // Note + Vol + Fx + FxParam (no per-channel reverb)
const ROW_NUM_WIDTH: f32 = 30.0;
const NOTE_WIDTH: f32 = 36.0;
// Instrument column removed - instrument is now per-channel in the channel strip
const VOL_WIDTH: f32 = 28.0;
const FX_WIDTH: f32 = 16.0;
const FXPARAM_WIDTH: f32 = 28.0;

/// Status bar height
const STATUS_BAR_HEIGHT: f32 = 22.0;

/// Draw the tracker interface
pub fn draw_tracker(ctx: &mut UiContext, rect: Rect, state: &mut TrackerState, icon_font: Option<&Font>) {
    // Background
    draw_rectangle(rect.x, rect.y, rect.w, rect.h, BG_COLOR);

    // Split into header, main area, and status bar
    let header_height = 60.0;
    let header_rect = Rect::new(rect.x, rect.y, rect.w, header_height);
    let status_rect = Rect::new(rect.x, rect.y + rect.h - STATUS_BAR_HEIGHT, rect.w, STATUS_BAR_HEIGHT);
    let main_rect = Rect::new(rect.x, rect.y + header_height, rect.w, rect.h - header_height - STATUS_BAR_HEIGHT);

    // Draw header (transport, info)
    draw_header(ctx, header_rect, state, icon_font);

    // Draw main content based on view
    match state.view {
        TrackerView::Pattern => draw_pattern_view(ctx, main_rect, state),
        TrackerView::Arrangement => draw_arrangement_view(ctx, main_rect, state),
    }

    // Draw status bar at bottom
    draw_status_bar(status_rect, state);

    // Handle input (but not if browser is open)
    if !state.song_browser.open {
        handle_input(ctx, state);
    }
}

/// Draw song browser dialog and handle actions
/// Call this separately from draw_tracker so modal input blocking works correctly
pub fn draw_song_browser(ctx: &mut UiContext, state: &mut TrackerState, icon_font: Option<&Font>) -> SongBrowserAction {
    let screen_rect = Rect::new(0.0, 0.0, screen_width(), screen_height());
    let browser_action = state.song_browser.draw(ctx, screen_rect, icon_font);

    match browser_action {
        SongBrowserAction::SelectPreview(idx) => {
            // Stop any playing preview when selecting a new song
            if state.song_browser.preview_playing {
                state.stop_preview_playback();
                state.song_browser.preview_playing = false;
            }
            let path = state.song_browser.songs.get(idx).map(|s| s.path.clone());
            if let Some(path) = path {
                #[cfg(not(target_arch = "wasm32"))]
                {
                    if let Ok(song) = super::io::load_song(&path) {
                        state.song_browser.set_preview(song);
                    }
                }
                #[cfg(target_arch = "wasm32")]
                {
                    state.song_browser.pending_load_path = Some(path);
                }
            }
        }
        SongBrowserAction::OpenSong => {
            let path = state.song_browser.selected_index
                .and_then(|idx| state.song_browser.songs.get(idx))
                .map(|s| s.path.clone());
            if let Some(path) = path {
                #[cfg(not(target_arch = "wasm32"))]
                {
                    if let Err(e) = state.load_from_file(&path) {
                        state.set_status(&format!("Load failed: {}", e), 3.0);
                    }
                }
                #[cfg(target_arch = "wasm32")]
                {
                    // TODO: WASM async load
                    let _ = path;
                    state.set_status("Loading...", 1.0);
                }
            }
        }
        SongBrowserAction::NewSong => {
            state.new_song();
        }
        SongBrowserAction::TogglePreview => {
            if state.song_browser.preview_playing {
                // Stop preview
                state.stop_preview_playback();
                state.song_browser.preview_playing = false;
            } else {
                // Start preview - clone the preview song
                if let Some(ref preview_song) = state.song_browser.preview_song {
                    state.start_preview_playback(preview_song.clone());
                    state.song_browser.preview_playing = true;
                }
            }
        }
        SongBrowserAction::Cancel | SongBrowserAction::None => {}
    }

    // Stop preview playback when browser closes
    if !state.song_browser.open && state.song_browser.preview_playing {
        state.stop_preview_playback();
        state.song_browser.preview_playing = false;
    }

    browser_action
}

/// Draw the header with transport controls and song info
fn draw_header(ctx: &mut UiContext, rect: Rect, state: &mut TrackerState, icon_font: Option<&Font>) {
    draw_rectangle(rect.x, rect.y, rect.w, rect.h, HEADER_COLOR);

    // First row: toolbar with icons (36.0 height to match World Editor)
    let toolbar_rect = Rect::new(rect.x, rect.y, rect.w, 36.0);
    let mut toolbar = Toolbar::new(toolbar_rect);

    // File operations
    if toolbar.icon_button(ctx, icon::FILE_PLUS, icon_font, "New Song (Ctrl+N)") {
        state.new_song();
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        if toolbar.icon_button(ctx, icon::FOLDER_OPEN, icon_font, "Open (Ctrl+O)") {
            // Open file dialog to load .ron song file
            if let Some(path) = rfd::FileDialog::new()
                .add_filter("Song", &["ron"])
                .set_directory("assets/songs")
                .pick_file()
            {
                if let Err(e) = state.load_from_file(&path) {
                    state.set_status(&format!("Load failed: {}", e), 3.0);
                }
            }
        }
        // Save button - save to current file or auto-generate name
        if toolbar.icon_button(ctx, icon::SAVE, icon_font, "Save (Ctrl+S)") {
            if let Some(path) = state.current_file.clone() {
                if let Err(e) = state.save_to_file(&path) {
                    state.set_status(&format!("Save failed: {}", e), 3.0);
                }
            } else {
                // No current file - use auto-generated name
                let path = next_available_song_name();
                if let Some(parent) = path.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                if let Err(e) = state.save_to_file(&path) {
                    state.set_status(&format!("Save failed: {}", e), 3.0);
                }
            }
        }
        if toolbar.icon_button(ctx, icon::SAVE_AS, icon_font, "Save As") {
            if let Some(path) = rfd::FileDialog::new()
                .add_filter("Song", &["ron"])
                .set_directory("assets/songs")
                .set_file_name(&format!("{}.ron", state.song.name))
                .save_file()
            {
                if let Err(e) = state.save_to_file(&path) {
                    state.set_status(&format!("Save failed: {}", e), 3.0);
                }
            }
        }
    }

    #[cfg(target_arch = "wasm32")]
    {
        if toolbar.icon_button(ctx, icon::FOLDER_OPEN, icon_font, "Upload") {
            // TODO: Trigger file upload from JavaScript
            state.set_status("Upload not implemented yet", 2.0);
        }
        if toolbar.icon_button(ctx, icon::SAVE, icon_font, "Download") {
            // TODO: Trigger file download to JavaScript
            state.set_status("Download not implemented yet", 2.0);
        }
    }

    // Browse bundled songs (works on both native and WASM)
    if toolbar.icon_button(ctx, icon::BOOK_OPEN, icon_font, "Browse") {
        state.song_browser.open();
    }

    toolbar.separator();

    // View mode buttons (Pattern includes instruments panel on right side)
    let view_icons = [
        (TrackerView::Pattern, icon::GRID, "Pattern Editor"),
        (TrackerView::Arrangement, icon::NOTEBOOK_PEN, "Arrangement"),
    ];

    for (view, icon_char, tooltip) in view_icons {
        let is_active = state.view == view;
        if toolbar.icon_button_active(ctx, icon_char, icon_font, tooltip, is_active) {
            state.view = view;
        }
    }

    toolbar.separator();

    // Transport controls
    if toolbar.icon_button(ctx, icon::SKIP_BACK, icon_font, "Stop & Rewind") {
        state.stop_playback();
    }

    // Play from start
    if toolbar.icon_button(ctx, icon::PLAY, icon_font, "Play from Start") {
        state.play_from_start();
    }

    // Play/pause from cursor
    let play_icon = if state.playing { icon::PAUSE } else { icon::SKIP_FORWARD };
    let play_tooltip = if state.playing { "Pause" } else { "Play from Cursor" };
    if toolbar.icon_button_active(ctx, play_icon, icon_font, play_tooltip, state.playing) {
        state.toggle_playback();
    }

    toolbar.separator();

    // BPM controls (Shift+click for ±10, normal click for ±1)
    let bpm_step = if is_key_down(KeyCode::LeftShift) || is_key_down(KeyCode::RightShift) { 10 } else { 1 };
    toolbar.label(&format!("BPM:{:3}", state.song.bpm));
    if toolbar.icon_button(ctx, icon::MINUS, icon_font, "Decrease BPM (Shift+click for ±10)") {
        state.song.bpm = (state.song.bpm as i32 - bpm_step).clamp(40, 300) as u16;
    }
    if toolbar.icon_button(ctx, icon::PLUS, icon_font, "Increase BPM (Shift+click for ±10)") {
        state.song.bpm = (state.song.bpm as i32 + bpm_step).clamp(40, 300) as u16;
    }
    if toolbar.text_button(ctx, "Tap", "Tap Tempo - click repeatedly to set BPM") {
        if let Some(bpm) = state.tap_tempo() {
            state.song.bpm = bpm;
            state.set_status(&format!("BPM: {}", bpm), 1.0);
        }
    }

    toolbar.separator();

    // Master volume controls (Shift+click for ±10, normal click for ±5)
    let vol_step = if is_key_down(KeyCode::LeftShift) || is_key_down(KeyCode::RightShift) { 10 } else { 5 };
    let current_vol = (state.audio.master_volume() * 100.0) as i32;
    toolbar.label(&format!("Vol:{:3}%", current_vol));
    if toolbar.icon_button(ctx, icon::MINUS, icon_font, "Decrease Volume (Shift for ±10)") {
        let new_vol = (current_vol - vol_step).clamp(0, 200) as f32 / 100.0;
        state.audio.set_master_volume(new_vol);
    }
    if toolbar.icon_button(ctx, icon::PLUS, icon_font, "Increase Volume (Shift for ±10)") {
        let new_vol = (current_vol + vol_step).clamp(0, 200) as f32 / 100.0;
        state.audio.set_master_volume(new_vol);
    }

    toolbar.separator();

    // Octave controls
    toolbar.label(&format!("Oct:{}", state.octave));
    if toolbar.icon_button(ctx, icon::MINUS, icon_font, "Octave Down") {
        state.octave = state.octave.saturating_sub(1);
    }
    if toolbar.icon_button(ctx, icon::PLUS, icon_font, "Octave Up") {
        state.octave = (state.octave + 1).min(9);
    }

    toolbar.separator();

    // Channel count controls
    toolbar.label(&format!("Ch:{}", state.num_channels()));
    if toolbar.icon_button(ctx, icon::MINUS, icon_font, "Remove Channel") {
        state.remove_channel();
    }
    if toolbar.icon_button(ctx, icon::PLUS, icon_font, "Add Channel") {
        state.add_channel();
    }

    toolbar.separator();

    // Pattern length controls
    toolbar.label(&format!("Len:{:3}", state.pattern_length()));
    if toolbar.icon_button(ctx, icon::MINUS, icon_font, "Decrease Pattern Length (-16)") {
        state.decrease_pattern_length();
    }
    if toolbar.icon_button(ctx, icon::PLUS, icon_font, "Increase Pattern Length (+16)") {
        state.increase_pattern_length();
    }

    // Second row - position info and soundfont status
    let y2 = rect.y + 40.0;
    let pattern_num = state.song.arrangement.get(state.current_pattern_idx).copied().unwrap_or(0);
    draw_text(
        &format!("Pos: {:02}/{:02}  Pat: {:02}  Row: {:03}/{:03}  Ch: {}",
                 state.current_pattern_idx,
                 state.song.arrangement.len(),
                 pattern_num,
                 state.current_row,
                 state.current_pattern().map(|p| p.length).unwrap_or(64),
                 state.current_channel + 1),
        rect.x + 10.0, y2 + 14.0, 14.0, TEXT_COLOR
    );

    // Song name / file name with dirty indicator
    let song_display = if let Some(filename) = state.current_file_name() {
        if state.dirty {
            format!("*{}", filename)
        } else {
            filename
        }
    } else if state.dirty {
        "*Untitled".to_string()
    } else {
        "Untitled".to_string()
    };
    draw_text(&song_display, rect.x + 380.0, y2 + 14.0, 14.0, TEXT_COLOR);

    // Soundfont status
    let sf_status = state.audio.soundfont_name()
        .map(|n| format!("SF: {}", n))
        .unwrap_or_else(|| "No Soundfont".to_string());
    draw_text(&sf_status, rect.x + 540.0, y2 + 14.0, 14.0, if state.audio.is_loaded() { TEXT_DIM } else { Color::new(0.8, 0.3, 0.3, 1.0) });

    // Status message
    if let Some(status) = state.get_status() {
        draw_text(status, rect.x + 720.0, y2 + 14.0, 14.0, Color::new(1.0, 0.8, 0.3, 1.0));
    }
}

/// Height of the channel strip header (simplified: just channel name + instrument)
const CHANNEL_STRIP_HEIGHT: f32 = 28.0;

/// Draw the pattern editor view with split instrument panel
fn draw_pattern_view(ctx: &mut UiContext, rect: Rect, state: &mut TrackerState) {
    // Always use split panel - users can drag divider to resize
    // Left: instruments, Right: pattern grid
    let (instrument_rect, pattern_rect) = state.pattern_split.layout(rect);

    // Draw instruments panel on left
    draw_instruments_view(ctx, instrument_rect, state);

    // Draw pattern grid on right
    draw_pattern_grid(ctx, pattern_rect, state);

    // Handle split panel divider dragging (after drawing content so widgets can claim drags first)
    state.pattern_split.handle_input(ctx, rect);
}

/// The main pattern grid (channels, rows, notes)
fn draw_pattern_grid(ctx: &mut UiContext, rect: Rect, state: &mut TrackerState) {
    let num_channels = state.num_channels();

    // Calculate how many channels fit in the available width
    let available_width = rect.w - ROW_NUM_WIDTH;
    let visible_channels = ((available_width / CHANNEL_WIDTH) as usize).min(num_channels).max(1);

    // Calculate visible rows (accounting for channel strip header)
    state.visible_rows = ((rect.h - CHANNEL_STRIP_HEIGHT - ROW_HEIGHT) / ROW_HEIGHT) as usize;

    // Get pattern info without holding borrow
    let (pattern_length, rows_per_beat) = match state.current_pattern() {
        Some(p) => (p.length, state.song.rows_per_beat),
        None => return,
    };

    // === Simplified channel strip header (just "Ch1: Piano" labels) ===
    draw_rectangle(rect.x, rect.y, rect.w, CHANNEL_STRIP_HEIGHT, Color::new(0.12, 0.12, 0.14, 1.0));

    let mut x = rect.x + ROW_NUM_WIDTH;
    let mut channels_drawn = 0usize;
    for ch in 0..visible_channels {
        let ch_x = x;

        // Stop drawing channels if this one would overflow the rect
        if ch_x + CHANNEL_WIDTH > rect.x + rect.w {
            break;
        }
        channels_drawn += 1;

        let is_current = ch == state.current_channel;

        // Background for selected channel
        if is_current {
            draw_rectangle(ch_x, rect.y, CHANNEL_WIDTH - 1.0, CHANNEL_STRIP_HEIGHT, Color::new(0.18, 0.2, 0.24, 1.0));
        }

        // Get instrument name for this channel
        let inst = state.song.get_channel_instrument(ch);
        let presets = state.audio.get_preset_names();
        let inst_name = presets
            .iter()
            .find(|(_, p, _)| *p == inst)
            .map(|(_, _, n)| n.as_str())
            .unwrap_or("---");

        // Truncate instrument name to fit
        let max_name_len = 10;
        let display_name: String = if inst_name.len() > max_name_len {
            format!("{:.width$}", inst_name, width = max_name_len)
        } else {
            inst_name.to_string()
        };

        // Display "Ch1: Piano" centered in the channel strip
        let ch_color = if is_current { NOTE_COLOR } else { TEXT_COLOR };
        let label = format!("Ch{}: {}", ch + 1, display_name);
        let label_dims = measure_text(&label, None, 12, 1.0);
        let label_x = ch_x + (CHANNEL_WIDTH - label_dims.width) / 2.0;
        let label_y = rect.y + CHANNEL_STRIP_HEIGHT / 2.0 + 4.0;
        draw_text(&label, label_x, label_y, 12.0, ch_color);

        // Click anywhere in channel strip to select this channel
        let strip_rect = Rect::new(ch_x, rect.y, CHANNEL_WIDTH - 1.0, CHANNEL_STRIP_HEIGHT);
        if ctx.mouse.inside(&strip_rect) && ctx.mouse.left_pressed {
            state.current_channel = ch;
        }

        x += CHANNEL_WIDTH;

        // Channel separator
        draw_line(x - 1.0, rect.y, x - 1.0, rect.y + rect.h, 1.0, Color::new(0.25, 0.25, 0.3, 1.0));
    }

    // === Column headers (Note, Volume, Fx) ===
    let header_y = rect.y + CHANNEL_STRIP_HEIGHT;
    draw_rectangle(rect.x, header_y, rect.w, ROW_HEIGHT, HEADER_COLOR);

    x = rect.x + ROW_NUM_WIDTH;
    for ch in 0..channels_drawn {
        let ch_x = x;
        let header_rect = Rect::new(ch_x, header_y, CHANNEL_WIDTH, ROW_HEIGHT);

        // Highlight on hover
        if ctx.mouse.inside(&header_rect) {
            draw_rectangle(ch_x, header_y, CHANNEL_WIDTH, ROW_HEIGHT, Color::new(0.25, 0.25, 0.3, 1.0));

            // Click to select channel
            if ctx.mouse.left_pressed {
                state.current_channel = ch;
            }
        }

        // Column labels (Note, Volume, Fx - instrument is per-channel in strip)
        let is_current = ch == state.current_channel;
        let label_color = if is_current { NOTE_COLOR } else { TEXT_DIM };
        draw_text("Not", ch_x + 4.0, header_y + 14.0, 12.0, label_color);
        draw_text("Vl", ch_x + NOTE_WIDTH + 2.0, header_y + 14.0, 12.0, label_color);
        draw_text("Fx", ch_x + NOTE_WIDTH + VOL_WIDTH + 2.0, header_y + 14.0, 12.0, label_color);

        x += CHANNEL_WIDTH;
    }

    // Handle mouse clicks and scrolling on pattern grid
    let grid_y_start = rect.y + CHANNEL_STRIP_HEIGHT + ROW_HEIGHT;
    let grid_rect = Rect::new(rect.x, grid_y_start, rect.w, rect.h - CHANNEL_STRIP_HEIGHT - ROW_HEIGHT);

    // Mouse wheel scrolling
    if ctx.mouse.inside(&grid_rect) && ctx.mouse.scroll != 0.0 {
        let scroll_amount = if ctx.mouse.scroll > 0.0 { -4 } else { 4 }; // Scroll 4 rows at a time
        let new_scroll = (state.scroll_row as i32 + scroll_amount).max(0) as usize;
        state.scroll_row = new_scroll.min(pattern_length.saturating_sub(state.visible_rows));
    }

    if ctx.mouse.inside(&grid_rect) && ctx.mouse.left_pressed {
        let mouse_x = ctx.mouse.x;
        let mouse_y = ctx.mouse.y;

        // Check modifier keys for selection modes
        let shift_held = is_key_down(KeyCode::LeftShift) || is_key_down(KeyCode::RightShift);
        let ctrl_held = is_key_down(KeyCode::LeftControl) || is_key_down(KeyCode::RightControl)
            || is_key_down(KeyCode::LeftSuper) || is_key_down(KeyCode::RightSuper);

        // Calculate clicked row
        let clicked_screen_row = ((mouse_y - grid_y_start) / ROW_HEIGHT) as usize;
        let clicked_row = state.scroll_row + clicked_screen_row;

        if clicked_row < pattern_length {
            // Calculate clicked channel
            let rel_x = mouse_x - rect.x - ROW_NUM_WIDTH;
            if rel_x >= 0.0 {
                let clicked_channel = (rel_x / CHANNEL_WIDTH) as usize;
                if clicked_channel < num_channels {
                    if shift_held {
                        // Shift+click: range selection from current position to clicked position
                        if !state.has_selection() {
                            // Start selection from current cursor position
                            state.selection_start = Some((state.current_pattern_idx, state.current_row, state.current_channel));
                        }
                        // Extend selection to clicked position
                        state.selection_end = Some((state.current_pattern_idx, clicked_row, clicked_channel));
                        // Move cursor to clicked position
                        state.current_row = clicked_row;
                        state.current_channel = clicked_channel;
                    } else if ctrl_held {
                        // Ctrl+click: toggle cell in selection
                        // For simplicity, just move cursor and keep/extend selection
                        // A full multi-selection system would need a different data structure
                        if state.has_selection() {
                            // Extend selection to include clicked cell
                            if let Some((start_row, end_row, start_ch, end_ch)) = state.get_selection_bounds() {
                                let new_start_row = start_row.min(clicked_row);
                                let new_end_row = end_row.max(clicked_row);
                                let new_start_ch = start_ch.min(clicked_channel);
                                let new_end_ch = end_ch.max(clicked_channel);
                                state.selection_start = Some((state.current_pattern_idx, new_start_row, new_start_ch));
                                state.selection_end = Some((state.current_pattern_idx, new_end_row, new_end_ch));
                            }
                        } else {
                            // Start new selection at clicked cell
                            state.selection_start = Some((state.current_pattern_idx, clicked_row, clicked_channel));
                            state.selection_end = Some((state.current_pattern_idx, clicked_row, clicked_channel));
                        }
                        state.current_row = clicked_row;
                        state.current_channel = clicked_channel;
                    } else {
                        // Normal click: move cursor, clear selection
                        state.clear_selection();
                        state.current_row = clicked_row;
                        state.current_channel = clicked_channel;
                    }

                    // Calculate column within channel (always update)
                    // Columns: 0=Note, 1=Volume, 2=Effect, 3=Effect param
                    let col_x = rel_x - (clicked_channel as f32 * CHANNEL_WIDTH);
                    state.current_column = if col_x < NOTE_WIDTH {
                        0 // Note
                    } else if col_x < NOTE_WIDTH + VOL_WIDTH {
                        1 // Volume
                    } else if col_x < NOTE_WIDTH + VOL_WIDTH + FX_WIDTH {
                        2 // Effect
                    } else {
                        3 // Effect param
                    };
                }
            }
        }
    }

    // Now re-borrow pattern for drawing
    let pattern = match state.current_pattern() {
        Some(p) => p,
        None => return,
    };

    // Draw rows
    let start_row = state.scroll_row;
    let visible_rows_count = state.visible_rows;
    let end_row = (start_row + visible_rows_count).min(pattern.length);
    let channels_to_draw = channels_drawn.min(pattern.num_channels());

    for row_idx in start_row..end_row {
        let screen_row = row_idx - start_row;
        let y = rect.y + CHANNEL_STRIP_HEIGHT + ROW_HEIGHT + screen_row as f32 * ROW_HEIGHT;

        // Row background
        let row_bg = if state.playing && row_idx == state.playback_row && state.playback_pattern_idx == state.current_pattern_idx {
            PLAYBACK_ROW_COLOR
        } else if row_idx == state.current_row {
            ROW_HIGHLIGHT
        } else if row_idx % (rows_per_beat as usize * 4) == 0 {
            ROW_BEAT
        } else if row_idx % 2 == 0 {
            ROW_EVEN
        } else {
            ROW_ODD
        };
        draw_rectangle(rect.x, y, rect.w, ROW_HEIGHT, row_bg);

        // Row number
        let row_color = if row_idx % (rows_per_beat as usize) == 0 { TEXT_COLOR } else { TEXT_DIM };
        draw_text(&format!("{:02X}", row_idx), rect.x + 4.0, y + 14.0, 12.0, row_color);

        // Draw each channel
        let mut x = rect.x + ROW_NUM_WIDTH;
        for ch in 0..channels_to_draw {
            let note = &pattern.channels[ch][row_idx];

            // Selection highlight (draw before cursor so cursor overlays selection)
            if state.is_in_selection(row_idx, ch) {
                // Selection color: semi-transparent blue
                let selection_color = Color::new(0.2, 0.4, 0.7, 0.5);
                draw_rectangle(x, y, CHANNEL_WIDTH - 4.0, ROW_HEIGHT, selection_color);
            }

            // Cursor highlight for channel columns (0-3)
            // Columns: 0=Note, 1=Volume, 2=Effect, 3=Effect param
            if row_idx == state.current_row && ch == state.current_channel {
                let col_x = x + match state.current_column {
                    0 => 0.0,
                    1 => NOTE_WIDTH,
                    2 => NOTE_WIDTH + VOL_WIDTH,
                    _ => NOTE_WIDTH + VOL_WIDTH + FX_WIDTH,
                };
                let col_w = match state.current_column {
                    0 => NOTE_WIDTH,
                    1 => VOL_WIDTH,
                    2 => FX_WIDTH,
                    _ => FXPARAM_WIDTH,
                };
                draw_rectangle(col_x, y, col_w, ROW_HEIGHT, CURSOR_COLOR);
            }

            // Note
            let note_str = note.pitch_name().unwrap_or_else(|| "---".to_string());
            let note_color = if note.pitch.is_some() { NOTE_COLOR } else { TEXT_DIM };
            draw_text(&note_str, x + 2.0, y + 14.0, 12.0, note_color);

            // Volume (instrument column removed - instrument is per-channel)
            let vol_str = note.volume.map(|v| format!("{:3}", v)).unwrap_or_else(|| "---".to_string());
            let vol_color = if note.volume.is_some() { VOL_COLOR } else { TEXT_DIM };
            draw_text(&vol_str, x + NOTE_WIDTH + 2.0, y + 14.0, 12.0, vol_color);

            // Effect
            let fx_str = note.effect.map(|e| e.to_string()).unwrap_or_else(|| "-".to_string());
            let fx_color = if note.effect.is_some() { FX_COLOR } else { TEXT_DIM };
            draw_text(&fx_str, x + NOTE_WIDTH + VOL_WIDTH + 2.0, y + 14.0, 12.0, fx_color);

            // Effect param
            let fxp_str = note.effect_param.map(|p| format!("{:3}", p)).unwrap_or_else(|| "---".to_string());
            draw_text(&fxp_str, x + NOTE_WIDTH + VOL_WIDTH + FX_WIDTH + 2.0, y + 14.0, 12.0, fx_color);

            x += CHANNEL_WIDTH;
        }
    }
}

/// State for arrangement view interactions
static mut ARRANGEMENT_SELECTION: usize = 0;
static mut PATTERN_BANK_SELECTION: usize = 0;
static mut ARRANGEMENT_FOCUS: bool = true; // true = arrangement, false = pattern bank

fn draw_arrangement_view(ctx: &mut UiContext, rect: Rect, state: &mut TrackerState) {
    draw_rectangle(rect.x, rect.y, rect.w, rect.h, BG_COLOR);

    // Layout: Pattern Bank (left) | Arrangement (right)
    let bank_width = 200.0;
    let arrangement_width = rect.w - bank_width - 20.0;
    let list_top = rect.y + 40.0;
    let list_height = rect.h - 80.0;
    let row_h = 24.0;

    // === Pattern Bank (left side) ===
    draw_text("Pattern Bank", rect.x + 10.0, rect.y + 24.0, 16.0, TEXT_COLOR);

    let bank_rect = Rect::new(rect.x + 10.0, list_top, bank_width - 20.0, list_height);
    draw_rectangle(bank_rect.x, bank_rect.y, bank_rect.w, bank_rect.h, Color::new(0.08, 0.08, 0.1, 1.0));

    // Get selection state (using unsafe statics for simplicity)
    let (arr_sel, bank_sel, arr_focus) = unsafe {
        (ARRANGEMENT_SELECTION, PATTERN_BANK_SELECTION, ARRANGEMENT_FOCUS)
    };

    // Draw patterns in bank - collect data first to avoid borrow issues
    let visible_bank_rows = (list_height / row_h) as usize;
    let pattern_count = state.song.patterns.len();
    let mut bank_click_action: Option<usize> = None; // pattern to add via right-click

    for i in 0..pattern_count.min(visible_bank_rows) {
        let pattern = &state.song.patterns[i];
        let y = bank_rect.y + (i as f32 * row_h);
        let is_selected = !arr_focus && i == bank_sel;
        let is_in_arrangement = state.song.arrangement.contains(&i);

        let bg = if is_selected {
            CURSOR_COLOR
        } else if i % 2 == 0 {
            ROW_EVEN
        } else {
            ROW_ODD
        };
        draw_rectangle(bank_rect.x, y, bank_rect.w, row_h - 2.0, bg);

        // Pattern info: index, length, note count indicator
        let note_count: usize = pattern.channels.iter()
            .flat_map(|ch| ch.iter())
            .filter(|n| !n.is_empty())
            .count();
        // Use * for patterns with notes, - for empty
        let indicator = if note_count > 0 { "*" } else { "-" };

        let text_color = if is_selected { Color::new(0.0, 0.0, 0.0, 1.0) } else { TEXT_COLOR };
        draw_text(
            &format!("{} {:02} [{:3} rows]", indicator, i, pattern.length),
            bank_rect.x + 6.0, y + 16.0, 12.0, text_color
        );

        // Show if used in arrangement with ">" indicator
        if is_in_arrangement {
            draw_text(">", bank_rect.x + bank_rect.w - 16.0, y + 16.0, 12.0,
                if is_selected { Color::new(0.0, 0.0, 0.0, 1.0) } else { NOTE_COLOR });
        }

        // Click to select
        let item_rect = Rect::new(bank_rect.x, y, bank_rect.w, row_h - 2.0);
        if ctx.mouse.inside(&item_rect) {
            if ctx.mouse.left_pressed {
                unsafe {
                    PATTERN_BANK_SELECTION = i;
                    ARRANGEMENT_FOCUS = false;
                }
            }
            // Double-click to add to arrangement
            // (We'd need double-click detection, for now use right-click)
            if ctx.mouse.right_pressed {
                bank_click_action = Some(i);
            }
        }
    }

    // Handle deferred bank click action
    if let Some(pattern_idx) = bank_click_action {
        state.arrangement_insert(state.song.arrangement.len(), pattern_idx);
    }

    // === Arrangement (right side) ===
    let arr_x = rect.x + bank_width + 10.0;
    draw_text("Arrangement", arr_x, rect.y + 24.0, 16.0, TEXT_COLOR);

    let arr_rect = Rect::new(arr_x, list_top, arrangement_width - 20.0, list_height);
    draw_rectangle(arr_rect.x, arr_rect.y, arr_rect.w, arr_rect.h, Color::new(0.08, 0.08, 0.1, 1.0));

    // Draw arrangement entries
    let visible_arr_rows = (list_height / row_h) as usize;
    for (i, &pattern_idx) in state.song.arrangement.iter().enumerate() {
        if i >= visible_arr_rows { break; }

        let y = arr_rect.y + (i as f32 * row_h);
        let is_current = i == state.current_pattern_idx;
        let is_selected = arr_focus && i == arr_sel;

        let bg = if is_selected {
            CURSOR_COLOR
        } else if is_current {
            ROW_HIGHLIGHT
        } else if i % 2 == 0 {
            ROW_EVEN
        } else {
            ROW_ODD
        };
        draw_rectangle(arr_rect.x, y, arr_rect.w, row_h - 2.0, bg);

        // Show position number and pattern reference
        let text_color = if is_selected { Color::new(0.0, 0.0, 0.0, 1.0) }
            else if is_current { NOTE_COLOR } else { TEXT_COLOR };
        draw_text(
            &format!("{:02} > Pattern {:02}", i, pattern_idx),
            arr_rect.x + 6.0, y + 16.0, 12.0, text_color
        );

        // Playback indicator
        if is_current && state.playing {
            draw_text(">", arr_rect.x + arr_rect.w - 20.0, y + 16.0, 12.0, PLAYBACK_ROW_COLOR);
        }

        // Click to select
        let item_rect = Rect::new(arr_rect.x, y, arr_rect.w, row_h - 2.0);
        if ctx.mouse.inside(&item_rect) {
            if ctx.mouse.left_pressed {
                unsafe {
                    ARRANGEMENT_SELECTION = i;
                    ARRANGEMENT_FOCUS = true;
                }
            }
            // Double-click (or Enter) to jump to that position
            if ctx.mouse.right_pressed {
                state.current_pattern_idx = i;
                state.current_row = 0;
                state.view = TrackerView::Pattern;
            }
        }
    }

    // === Help text ===
    let help_y = rect.y + rect.h - 30.0;
    draw_text(
        "Tab: Switch focus | +: New pattern | Enter: Add to arrangement | Del: Remove | ↑↓: Move",
        rect.x + 10.0, help_y, 12.0, TEXT_DIM
    );

    // === Keyboard handling for arrangement view ===
    handle_arrangement_input(ctx, state);
}

/// Handle keyboard input for arrangement view
fn handle_arrangement_input(_ctx: &mut UiContext, state: &mut TrackerState) {
    let (arr_sel, bank_sel, arr_focus) = unsafe {
        (ARRANGEMENT_SELECTION, PATTERN_BANK_SELECTION, ARRANGEMENT_FOCUS)
    };

    // Tab switches focus between bank and arrangement
    if is_key_pressed(KeyCode::Tab) {
        unsafe { ARRANGEMENT_FOCUS = !ARRANGEMENT_FOCUS; }
    }

    // Navigation
    if is_key_pressed(KeyCode::Up) {
        if arr_focus {
            unsafe { ARRANGEMENT_SELECTION = arr_sel.saturating_sub(1); }
        } else {
            unsafe { PATTERN_BANK_SELECTION = bank_sel.saturating_sub(1); }
        }
    }
    if is_key_pressed(KeyCode::Down) {
        if arr_focus {
            unsafe {
                if arr_sel + 1 < state.song.arrangement.len() {
                    ARRANGEMENT_SELECTION = arr_sel + 1;
                }
            }
        } else {
            unsafe {
                if bank_sel + 1 < state.song.patterns.len() {
                    PATTERN_BANK_SELECTION = bank_sel + 1;
                }
            }
        }
    }

    // Pattern bank actions
    if !arr_focus {
        // + or Insert: Create new pattern
        if is_key_pressed(KeyCode::Equal) || is_key_pressed(KeyCode::KpAdd) || is_key_pressed(KeyCode::Insert) {
            let new_idx = state.create_pattern();
            state.set_status(&format!("Created pattern {:02}", new_idx), 1.5);
            unsafe { PATTERN_BANK_SELECTION = new_idx; }
        }

        // Enter: Add selected pattern to arrangement
        if is_key_pressed(KeyCode::Enter) {
            state.arrangement_insert(state.song.arrangement.len(), bank_sel);
            state.set_status(&format!("Added pattern {:02} to arrangement", bank_sel), 1.5);
        }

        // D: Duplicate pattern
        if is_key_pressed(KeyCode::D) {
            if let Some(new_idx) = state.duplicate_pattern(bank_sel) {
                state.set_status(&format!("Duplicated to pattern {:02}", new_idx), 1.5);
                unsafe { PATTERN_BANK_SELECTION = new_idx; }
            }
        }

        // Delete: Delete pattern (if not the last one)
        if is_key_pressed(KeyCode::Delete) || is_key_pressed(KeyCode::Backspace) {
            if state.delete_pattern(bank_sel) {
                state.set_status("Pattern deleted", 1.5);
                unsafe {
                    if PATTERN_BANK_SELECTION >= state.song.patterns.len() {
                        PATTERN_BANK_SELECTION = state.song.patterns.len().saturating_sub(1);
                    }
                }
            } else {
                state.set_status("Cannot delete last pattern", 1.5);
            }
        }
    }

    // Arrangement actions
    if arr_focus && arr_sel < state.song.arrangement.len() {
        // Enter: Jump to pattern and switch to pattern view
        if is_key_pressed(KeyCode::Enter) {
            state.current_pattern_idx = arr_sel;
            state.current_row = 0;
            state.view = TrackerView::Pattern;
        }

        // Delete/Backspace: Remove from arrangement
        if is_key_pressed(KeyCode::Delete) || is_key_pressed(KeyCode::Backspace) {
            if state.arrangement_remove(arr_sel) {
                state.set_status("Removed from arrangement", 1.5);
                unsafe {
                    if ARRANGEMENT_SELECTION >= state.song.arrangement.len() {
                        ARRANGEMENT_SELECTION = state.song.arrangement.len().saturating_sub(1);
                    }
                }
            }
        }

        // Shift+Up/Down: Move arrangement entry
        let shift = is_key_down(KeyCode::LeftShift) || is_key_down(KeyCode::RightShift);
        if shift && is_key_pressed(KeyCode::Up) {
            if state.arrangement_move_up(arr_sel) {
                unsafe { ARRANGEMENT_SELECTION = arr_sel - 1; }
            }
        }
        if shift && is_key_pressed(KeyCode::Down) {
            if state.arrangement_move_down(arr_sel) {
                unsafe { ARRANGEMENT_SELECTION = arr_sel + 1; }
            }
        }

        // +/-: Change which pattern is at this position
        if is_key_pressed(KeyCode::Equal) || is_key_pressed(KeyCode::KpAdd) {
            let current_pat = state.song.arrangement[arr_sel];
            let new_pat = (current_pat + 1) % state.song.patterns.len();
            state.arrangement_set_pattern(arr_sel, new_pat);
        }
        if is_key_pressed(KeyCode::Minus) || is_key_pressed(KeyCode::KpSubtract) {
            let current_pat = state.song.arrangement[arr_sel];
            let new_pat = if current_pat == 0 { state.song.patterns.len() - 1 } else { current_pat - 1 };
            state.arrangement_set_pattern(arr_sel, new_pat);
        }

        // Insert: Insert the currently selected bank pattern at this position
        if is_key_pressed(KeyCode::Insert) {
            state.arrangement_insert(arr_sel, bank_sel);
            state.set_status(&format!("Inserted pattern {:02}", bank_sel), 1.5);
        }
    }
}

/// Piano key layout for drawing
const PIANO_WHITE_KEYS: [(u8, &str); 7] = [
    (0, "C"), (2, "D"), (4, "E"), (5, "F"), (7, "G"), (9, "A"), (11, "B")
];
const PIANO_BLACK_KEYS: [(u8, &str, f32); 5] = [
    (1, "C#", 0.7), (3, "D#", 1.7), (6, "F#", 3.7), (8, "G#", 4.7), (10, "A#", 5.7)
];

/// Keyboard mapping for piano: maps semitone offset to keyboard key name
/// Continuous layout: Bottom row Z-/ (0-16), Top row Q-] (17-36)
fn get_key_label(offset: u8) -> Option<&'static str> {
    match offset {
        // Bottom row: Z to / (semitones 0-16: C to E)
        0 => Some("Z"), 1 => Some("S"), 2 => Some("X"), 3 => Some("D"), 4 => Some("C"),
        5 => Some("V"), 6 => Some("G"), 7 => Some("B"), 8 => Some("H"), 9 => Some("N"),
        10 => Some("J"), 11 => Some("M"), 12 => Some(","), 13 => Some("L"), 14 => Some("."),
        15 => Some(";"), 16 => Some("/"),
        // Top row: Q to ] (semitones 17-36: F to C)
        // Pattern:  2  3  4     6  7     9  0  -
        //          Q  W  E  R  T  Y  U  I  O  P  [  ]
        17 => Some("Q"), 18 => Some("2"), 19 => Some("W"), 20 => Some("3"), 21 => Some("E"),
        22 => Some("4"), 23 => Some("R"), 24 => Some("T"), 25 => Some("6"), 26 => Some("Y"),
        27 => Some("7"), 28 => Some("U"), 29 => Some("I"), 30 => Some("9"), 31 => Some("O"),
        32 => Some("0"), 33 => Some("P"), 34 => Some("-"), 35 => Some("["), 36 => Some("]"),
        _ => None,
    }
}

/// Check if the keyboard key for a given semitone offset is currently pressed
/// Continuous layout: Bottom row (0-16), Top row (17-36)
fn is_note_key_down(offset: u8) -> bool {
    let key = match offset {
        // Bottom row: Z to / (semitones 0-16)
        0 => Some(KeyCode::Z), 1 => Some(KeyCode::S), 2 => Some(KeyCode::X), 3 => Some(KeyCode::D),
        4 => Some(KeyCode::C), 5 => Some(KeyCode::V), 6 => Some(KeyCode::G), 7 => Some(KeyCode::B),
        8 => Some(KeyCode::H), 9 => Some(KeyCode::N), 10 => Some(KeyCode::J), 11 => Some(KeyCode::M),
        12 => Some(KeyCode::Comma), 13 => Some(KeyCode::L), 14 => Some(KeyCode::Period),
        15 => Some(KeyCode::Semicolon), 16 => Some(KeyCode::Slash),
        // Top row: Q to ] (semitones 17-36)
        17 => Some(KeyCode::Q), 18 => Some(KeyCode::Key2), 19 => Some(KeyCode::W), 20 => Some(KeyCode::Key3),
        21 => Some(KeyCode::E), 22 => Some(KeyCode::Key4), 23 => Some(KeyCode::R), 24 => Some(KeyCode::T),
        25 => Some(KeyCode::Key6), 26 => Some(KeyCode::Y), 27 => Some(KeyCode::Key7), 28 => Some(KeyCode::U),
        29 => Some(KeyCode::I), 30 => Some(KeyCode::Key9), 31 => Some(KeyCode::O), 32 => Some(KeyCode::Key0),
        33 => Some(KeyCode::P), 34 => Some(KeyCode::Minus), 35 => Some(KeyCode::LeftBracket),
        36 => Some(KeyCode::RightBracket),
        _ => None,
    };

    key.map_or(false, is_key_down)
}

/// Draw the instruments view with piano keyboard
fn draw_instruments_view(ctx: &mut UiContext, rect: Rect, state: &mut TrackerState) {
    draw_rectangle(rect.x, rect.y, rect.w, rect.h, BG_COLOR);

    // Split into left (instrument list) and right (piano + info)
    let list_width = 280.0;
    let list_rect = Rect::new(rect.x, rect.y, list_width, rect.h);

    // === LEFT: Instrument List ===
    draw_rectangle(list_rect.x, list_rect.y, list_rect.w, list_rect.h, Color::new(0.09, 0.09, 0.11, 1.0));
    draw_text("Instruments (GM)", list_rect.x + 10.0, list_rect.y + 20.0, 16.0, TEXT_COLOR);

    // Scrollable instrument list
    let presets = state.audio.get_preset_names();
    let item_height = 18.0;
    let list_start_y = list_rect.y + 35.0;
    let list_height = list_rect.h - 45.0;
    let visible_items = (list_height / item_height) as usize;
    let max_scroll = presets.len().saturating_sub(visible_items);

    // Handle mouse wheel scrolling over the instrument list
    let list_content_rect = Rect::new(list_rect.x, list_start_y, list_rect.w, list_height);
    if ctx.mouse.inside(&list_content_rect) && ctx.mouse.scroll != 0.0 {
        let scroll_amount = if ctx.mouse.scroll > 0.0 { -3 } else { 3 }; // Scroll 3 items at a time
        let new_scroll = (state.instrument_scroll as i32 + scroll_amount).max(0) as usize;
        state.instrument_scroll = new_scroll.min(max_scroll);
    }

    let current_inst = state.current_instrument();
    let scroll_offset = state.instrument_scroll.min(max_scroll);

    for (i, (_, program, name)) in presets.iter().enumerate().skip(scroll_offset).take(visible_items) {
        let y = list_start_y + (i - scroll_offset) as f32 * item_height;
        let item_rect = Rect::new(list_rect.x + 5.0, y, list_rect.w - 10.0, item_height);

        let is_current = *program == current_inst;
        let is_hovered = ctx.mouse.inside(&item_rect);

        // Background
        let bg = if is_current {
            Color::new(0.25, 0.3, 0.35, 1.0)
        } else if is_hovered {
            Color::new(0.18, 0.18, 0.22, 1.0)
        } else if i % 2 == 0 {
            Color::new(0.11, 0.11, 0.13, 1.0)
        } else {
            Color::new(0.09, 0.09, 0.11, 1.0)
        };
        draw_rectangle(item_rect.x, item_rect.y, item_rect.w, item_rect.h, bg);

        // Click to select (sets the current channel's instrument)
        if is_hovered && ctx.mouse.left_pressed {
            state.set_current_instrument(*program);
        }

        // Text
        let color = if is_current { NOTE_COLOR } else { TEXT_COLOR };
        draw_text(&format!("{:03}: {}", program, name), item_rect.x + 5.0, y + 13.0, 12.0, color);
    }

    // Draw scrollbar if needed
    if presets.len() > visible_items {
        let scrollbar_x = list_rect.x + list_rect.w - 8.0;
        let scrollbar_h = list_height * (visible_items as f32 / presets.len() as f32);
        let scrollbar_y = list_start_y + (scroll_offset as f32 / max_scroll as f32) * (list_height - scrollbar_h);

        // Track
        draw_rectangle(scrollbar_x, list_start_y, 6.0, list_height, Color::new(0.15, 0.15, 0.18, 1.0));
        // Thumb
        draw_rectangle(scrollbar_x, scrollbar_y, 6.0, scrollbar_h, Color::new(0.35, 0.35, 0.4, 1.0));
    }

    // === RIGHT: Piano Keyboard ===
    // Extended piano showing 3+ octaves to match the full keyboard layout (semitones 0-36)
    let piano_x = rect.x + list_width + 20.0;
    let piano_y = rect.y + 30.0;
    let white_key_w = 24.0;  // Narrower to fit more keys
    let white_key_h = 100.0;
    let black_key_w = 16.0;
    let black_key_h = 60.0;

    draw_text(&format!("Piano - Octave {}", state.octave), piano_x, piano_y - 10.0, 16.0, TEXT_COLOR);

    // Define all white keys we need to display (semitones 0-36, ~3 octaves: C to C)
    // semitone offset, note name
    let all_white_keys: [(u8, &str); 22] = [
        (0, "C"), (2, "D"), (4, "E"), (5, "F"), (7, "G"), (9, "A"), (11, "B"),  // Oct 0: C-B
        (12, "C"), (14, "D"), (16, "E"), (17, "F"), (19, "G"), (21, "A"), (23, "B"),  // Oct 1: C-B
        (24, "C"), (26, "D"), (28, "E"), (29, "F"), (31, "G"), (33, "A"), (35, "B"),  // Oct 2: C-B
        (36, "C"),  // Oct 3: just the final C
    ];

    // Define all black keys with their x positions relative to white key index
    // semitone offset, x position (between which white keys)
    let all_black_keys: [(u8, f32); 15] = [
        (1, 0.7), (3, 1.7), (6, 3.7), (8, 4.7), (10, 5.7),  // Oct 0
        (13, 7.7), (15, 8.7), (18, 10.7), (20, 11.7), (22, 12.7),  // Oct 1
        (25, 14.7), (27, 15.7), (30, 17.7), (32, 18.7), (34, 19.7),  // Oct 2
    ];

    // Draw white keys first
    for (i, (semitone, note_name)) in all_white_keys.iter().enumerate() {
        let key_x = piano_x + i as f32 * white_key_w;
        let key_rect = Rect::new(key_x, piano_y, white_key_w - 2.0, white_key_h);

        let midi_note = state.octave * 12 + *semitone;
        let is_hovered = ctx.mouse.inside(&key_rect);
        let is_key_pressed = is_note_key_down(*semitone);
        let is_mouse_pressed = is_hovered && ctx.mouse.left_down;

        // Background - cyan highlight when key pressed (keyboard or mouse), gray when hovered
        let bg = if is_key_pressed || is_mouse_pressed {
            Color::new(0.0, 0.75, 0.9, 1.0) // Cyan highlight
        } else if is_hovered {
            Color::new(0.85, 0.85, 0.9, 1.0)
        } else {
            Color::new(0.95, 0.95, 0.95, 1.0)
        };
        draw_rectangle(key_x, piano_y, white_key_w - 2.0, white_key_h, Color::new(0.3, 0.3, 0.3, 1.0));
        draw_rectangle(key_x + 1.0, piano_y + 1.0, white_key_w - 4.0, white_key_h - 2.0, bg);

        // Click to play
        if is_hovered && ctx.mouse.left_pressed {
            state.audio.note_on(state.current_channel as i32, midi_note as i32, 100);
        }
        if is_hovered && ctx.mouse.left_released {
            state.audio.note_off(state.current_channel as i32, midi_note as i32);
        }

        // Note name at bottom (only show for C notes to reduce clutter)
        if note_name == &"C" {
            let octave_num = state.octave + (*semitone / 12);
            let text_color = if is_key_pressed { WHITE } else { Color::new(0.3, 0.3, 0.3, 1.0) };
            draw_text(&format!("C{}", octave_num), key_x + 2.0, piano_y + white_key_h - 20.0, 10.0, text_color);
        }

        // Keyboard shortcut label (single label per key - continuous layout)
        if let Some(label) = get_key_label(*semitone) {
            let label_color = if is_key_pressed { WHITE } else { Color::new(0.5, 0.5, 0.5, 1.0) };
            draw_text(label, key_x + 6.0, piano_y + white_key_h - 5.0, 10.0, label_color);
        }
    }

    // Draw black keys on top
    for (semitone, x_pos) in all_black_keys.iter() {
        let key_x = piano_x + *x_pos * white_key_w;
        let key_rect = Rect::new(key_x, piano_y, black_key_w, black_key_h);

        let midi_note = state.octave * 12 + *semitone;
        let is_hovered = ctx.mouse.inside(&key_rect);
        let is_key_pressed = is_note_key_down(*semitone);
        let is_mouse_pressed = is_hovered && ctx.mouse.left_down;

        // Background - cyan highlight when key pressed (keyboard or mouse)
        let bg = if is_key_pressed || is_mouse_pressed {
            Color::new(0.0, 0.6, 0.75, 1.0) // Darker cyan for black keys
        } else if is_hovered {
            Color::new(0.35, 0.35, 0.4, 1.0)
        } else {
            Color::new(0.15, 0.15, 0.18, 1.0)
        };
        draw_rectangle(key_x, piano_y, black_key_w, black_key_h, bg);

        // Click to play
        if is_hovered && ctx.mouse.left_pressed {
            state.audio.note_on(state.current_channel as i32, midi_note as i32, 100);
        }
        if is_hovered && ctx.mouse.left_released {
            state.audio.note_off(state.current_channel as i32, midi_note as i32);
        }

        // Keyboard shortcut label (single label per key - continuous layout)
        if let Some(label) = get_key_label(*semitone) {
            let label_color = if is_key_pressed { WHITE } else { Color::new(0.6, 0.6, 0.6, 1.0) };
            draw_text(label, key_x + 3.0, piano_y + black_key_h - 5.0, 9.0, label_color);
        }
    }

    // Current instrument info below piano
    let info_y = piano_y + white_key_h + 30.0;
    let current_inst = state.current_instrument();
    let current_name = presets.iter()
        .find(|(_, p, _)| *p == current_inst)
        .map(|(_, _, n)| n.as_str())
        .unwrap_or("Unknown");

    draw_text(&format!("Current: {:03} - {}", current_inst, current_name),
              piano_x, info_y, 16.0, INST_COLOR);

    // === CHANNEL EFFECT KNOBS (with per-channel sample rate, reverb) ===
    let effects_y = info_y + 25.0;
    let ch = state.current_channel;

    // Show which channel we're editing
    draw_text(&format!("Channel {} Effects", ch + 1), piano_x, effects_y, 16.0, TEXT_COLOR);

    // Per-channel sample rate buttons
    let sr_y = effects_y + 20.0;
    let sr_btn_w = 52.0;
    let sr_btn_h = 20.0;
    let sr_spacing = 2.0;

    // Get current channel's sample rate setting (0=OFF, 1=44k, 2=22k, 3=11k, 4=5k)
    let channel_settings = state.song.get_channel_settings(ch);
    let current_sr = channel_settings.sample_rate;

    // Sample rate button labels: OFF, 44k, 22k, 11k, 5k
    let sr_labels = ["OFF", "44kHz", "22kHz", "11kHz", "5kHz"];
    for (i, label) in sr_labels.iter().enumerate() {
        let btn_x = piano_x + i as f32 * (sr_btn_w + sr_spacing);
        let btn_rect = Rect::new(btn_x, sr_y, sr_btn_w, sr_btn_h);
        let is_active = current_sr == i as u8;
        let is_hovered = ctx.mouse.inside(&btn_rect);

        let bg = if is_active {
            Color::new(0.2, 0.4, 0.5, 1.0) // Cyan for active
        } else if is_hovered {
            Color::new(0.25, 0.25, 0.3, 1.0)
        } else {
            Color::new(0.15, 0.15, 0.18, 1.0)
        };

        draw_rectangle(btn_x, sr_y, sr_btn_w, sr_btn_h, bg);
        let text_color = if is_active { WHITE } else { TEXT_COLOR };
        draw_text(label, btn_x + 6.0, sr_y + 14.0, 11.0, text_color);

        if is_hovered && ctx.mouse.left_pressed {
            state.set_channel_sample_rate(ch, i as u8);
            state.set_status(&format!("Ch{} Sample Rate: {}", ch + 1, label), 1.0);
        }
    }

    // Per-channel reverb preset (row of buttons)
    let reverb_y = sr_y + sr_btn_h + 10.0;
    let preset_btn_w = 68.0;
    let preset_btn_h = 20.0;
    let preset_spacing = 2.0;
    let presets_per_row = 5;

    // Get current channel's reverb settings (reuse channel_settings from above)
    let current_reverb_idx = channel_settings.reverb_type;
    let current_wet = channel_settings.wet;

    for (i, reverb_type) in ReverbType::ALL.iter().enumerate() {
        let row = i / presets_per_row;
        let col = i % presets_per_row;
        let btn_x = piano_x + col as f32 * (preset_btn_w + preset_spacing);
        let btn_y = reverb_y + row as f32 * (preset_btn_h + preset_spacing);

        let btn_rect = Rect::new(btn_x, btn_y, preset_btn_w, preset_btn_h);
        let is_active = reverb_type.to_index() == current_reverb_idx;
        let is_hovered = ctx.mouse.inside(&btn_rect);

        let bg = if is_active {
            Color::new(0.2, 0.5, 0.3, 1.0) // Green for active
        } else if is_hovered {
            Color::new(0.25, 0.25, 0.3, 1.0)
        } else {
            Color::new(0.15, 0.15, 0.18, 1.0)
        };

        draw_rectangle(btn_x, btn_y, preset_btn_w, preset_btn_h, bg);
        let text_color = if is_active { WHITE } else { TEXT_COLOR };
        draw_text(reverb_type.name(), btn_x + 4.0, btn_y + 14.0, 11.0, text_color);

        if is_hovered && ctx.mouse.left_pressed {
            state.set_channel_reverb_type(ch, reverb_type.to_index());
            state.set_status(&format!("Ch{} Reverb: {}", ch + 1, reverb_type.name()), 1.0);
        }
    }

    // Wet knob (next to reverb buttons)
    let wet_knob_x = piano_x + presets_per_row as f32 * (preset_btn_w + preset_spacing) + 25.0;
    let wet_knob_y = reverb_y + preset_btn_h / 2.0 + 10.0;
    if let Some(new_val) = draw_mini_knob(ctx, wet_knob_x, wet_knob_y, 14.0, current_wet, "Wet", false) {
        state.set_channel_wet(ch, new_val);
    }

    // Pan/Mod/Expr knobs below the reverb buttons
    let knob_radius = 28.0;
    let knob_spacing = 70.0;
    let knob_y = reverb_y + 2.0 * (preset_btn_h + preset_spacing) + 40.0; // Below the 2 rows of reverb buttons

    // Read persistent channel settings (saved in song file)
    let settings = state.song.get_channel_settings(ch);

    // Knob definitions: (index, label, value, is_bipolar) - using persistent channel_settings
    let knob_data = [
        (0, "Pan", settings.pan, true),
        (1, "Mod", settings.modulation, false),
        (2, "Expr", settings.expression, false),
    ];

    // Handle text input for knob editing
    if let Some(editing_idx) = state.editing_knob {
        // Handle keyboard input for editing
        for key in 0..10 {
            let keycode = match key {
                0 => KeyCode::Key0,
                1 => KeyCode::Key1,
                2 => KeyCode::Key2,
                3 => KeyCode::Key3,
                4 => KeyCode::Key4,
                5 => KeyCode::Key5,
                6 => KeyCode::Key6,
                7 => KeyCode::Key7,
                8 => KeyCode::Key8,
                9 => KeyCode::Key9,
                _ => continue,
            };
            if is_key_pressed(keycode) && state.knob_edit_text.len() < 3 {
                state.knob_edit_text.push(char::from_digit(key as u32, 10).unwrap());
            }
        }

        // Backspace
        if is_key_pressed(KeyCode::Backspace) {
            state.knob_edit_text.pop();
        }

        // Enter to confirm - use persistent channel settings
        if is_key_pressed(KeyCode::Enter) {
            if let Ok(val) = state.knob_edit_text.parse::<u8>() {
                let clamped = val.min(127);
                match editing_idx {
                    0 => state.set_channel_pan(ch, clamped),
                    1 => state.set_channel_modulation(ch, clamped),
                    2 => state.set_channel_expression(ch, clamped),
                    _ => {}
                }
            }
            state.editing_knob = None;
            state.knob_edit_text.clear();
        }

        // Escape to cancel
        if is_key_pressed(KeyCode::Escape) {
            state.editing_knob = None;
            state.knob_edit_text.clear();
        }
    }

    // Draw knobs
    for (i, (idx, label, value, is_bipolar)) in knob_data.iter().enumerate() {
        let knob_x = piano_x + 35.0 + i as f32 * knob_spacing;
        let is_editing = state.editing_knob == Some(*idx);

        let result = draw_knob(
            ctx,
            knob_x,
            knob_y,
            knob_radius,
            *value,
            label,
            *is_bipolar,
            is_editing,
        );

        // Handle knob value change - use persistent channel settings
        if let Some(new_val) = result.value {
            match idx {
                0 => state.set_channel_pan(ch, new_val),
                1 => state.set_channel_modulation(ch, new_val),
                2 => state.set_channel_expression(ch, new_val),
                _ => {}
            }
        }

        // Handle editing start
        if result.editing {
            state.editing_knob = Some(*idx);
            state.knob_edit_text = format!("{}", value);
        }
    }

    // Reset button - reset persistent channel settings
    let reset_y = knob_y + knob_radius + 35.0;
    let reset_rect = Rect::new(piano_x, reset_y, 100.0, 20.0);
    let reset_hovered = ctx.mouse.inside(&reset_rect);

    draw_rectangle(reset_rect.x, reset_rect.y, reset_rect.w, reset_rect.h,
        if reset_hovered { Color::new(0.25, 0.25, 0.3, 1.0) } else { Color::new(0.18, 0.18, 0.22, 1.0) });
    draw_text("Reset", reset_rect.x + 30.0, reset_rect.y + 14.0, 12.0, TEXT_COLOR);

    if reset_hovered && ctx.mouse.left_pressed {
        state.reset_channel_settings(ch);
        state.set_status(&format!("Channel {} reset to defaults", ch + 1), 1.0);
    }

    // === EFFECT BUTTONS (insert at cursor position) ===
    let effects_btn_y = reset_y + 30.0;
    draw_text("Insert Effect", piano_x, effects_btn_y, 14.0, TEXT_COLOR);

    // Effect button definitions: (effect_char, label, tooltip)
    let effect_btns: [(char, &str); 10] = [
        ('0', "Arp"),
        ('1', "SlideUp"),
        ('2', "SlideDn"),
        ('3', "Porta"),
        ('4', "Vib"),
        ('A', "VolSlide"),
        ('C', "Vol"),
        ('E', "Expr"),
        ('M', "Mod"),
        ('P', "Pan"),
    ];

    let fx_btn_w = 60.0;
    let fx_btn_h = 20.0;
    let fx_btn_spacing = 2.0;
    let fx_btns_per_row = 5;
    let fx_btn_start_y = effects_btn_y + 15.0;

    for (i, (effect_char, label)) in effect_btns.iter().enumerate() {
        let row = i / fx_btns_per_row;
        let col = i % fx_btns_per_row;
        let btn_x = piano_x + col as f32 * (fx_btn_w + fx_btn_spacing);
        let btn_y = fx_btn_start_y + row as f32 * (fx_btn_h + fx_btn_spacing);

        let btn_rect = Rect::new(btn_x, btn_y, fx_btn_w, fx_btn_h);
        let is_hovered = ctx.mouse.inside(&btn_rect);

        let bg = if is_hovered {
            Color::new(0.3, 0.4, 0.5, 1.0) // Blue-ish for hover
        } else {
            Color::new(0.18, 0.18, 0.22, 1.0)
        };

        draw_rectangle(btn_x, btn_y, fx_btn_w, fx_btn_h, bg);
        let text_color = if is_hovered { WHITE } else { TEXT_COLOR };
        draw_text(label, btn_x + 4.0, btn_y + 14.0, 11.0, text_color);

        if is_hovered && ctx.mouse.left_pressed {
            // Insert effect at cursor position with the current effect amount
            let effect_amount = state.song.get_channel_settings(ch).effect_amount;
            state.set_effect(*effect_char, effect_amount);
            state.set_status(&format!("Inserted {} ({})", label, effect_amount), 1.0);
        }
    }

    // Effect amount knob (controls the parameter value inserted)
    let fx_amount_x = piano_x + fx_btns_per_row as f32 * (fx_btn_w + fx_btn_spacing) + 25.0;
    let fx_amount_y = fx_btn_start_y + fx_btn_h / 2.0 + 2.0;
    let current_fx_amount = state.song.get_channel_settings(ch).effect_amount;
    if let Some(new_val) = draw_mini_knob(ctx, fx_amount_x, fx_amount_y, 14.0, current_fx_amount, "Amt", false) {
        state.set_channel_effect_amount(ch, new_val);
    }

    // Help text
    let help_y = fx_btn_start_y + 2.0 * (fx_btn_h + fx_btn_spacing) + 15.0;
    draw_text("Click keys to preview | Keyboard: Z-/ (lower) Q-] (upper)",
              piano_x, help_y, 12.0, TEXT_DIM);
    draw_text("Numpad +/- = octave | Drag knobs to adjust effects",
              piano_x, help_y + 17.0, 12.0, TEXT_DIM);
    draw_text("Click value to type | Use list or channel +/- for instrument",
              piano_x, help_y + 34.0, 12.0, TEXT_DIM);
}

/// Draw the status bar at the bottom with context-sensitive help
fn draw_status_bar(rect: Rect, state: &TrackerState) {
    draw_rectangle(rect.x, rect.y, rect.w, rect.h, Color::new(0.16, 0.16, 0.18, 1.0));

    // Left side: Column-specific help based on current column and view
    let column_help = match state.view {
        TrackerView::Pattern => {
            // Columns: 0=Note, 1=Volume, 2=Effect, 3=Effect param
            match state.current_column {
                0 => "Note: Z-/ Q-] piano keys | ` note-off | Del clear",
                1 => "Volume: 0-127 | Del clear",
                2 => "Effect: 0=Arp 1=SlideUp 2=SlideDown 3=Porta 4=Vib A=VolSlide C=Vol E=Expr M=Mod P=Pan",
                _ => "Effect Param: 0-127 | Del clear",
            }
        }
        TrackerView::Arrangement => {
            "Tab: switch focus | Enter: edit pattern | +: new pattern | Del: remove | Shift+↑↓: reorder"
        }
    };

    draw_text(column_help, rect.x + 10.0, rect.y + 15.0, 14.0, TEXT_COLOR);

    // Right side: Global shortcuts
    let shortcuts = "Ctrl+S: Save | Ctrl+O: Open | Ctrl+N: New | Space: Play/Pause";

    let shortcuts_width = shortcuts.len() as f32 * 6.5; // Approximate width
    draw_text(
        shortcuts,
        rect.x + rect.w - shortcuts_width - 10.0,
        rect.y + 15.0,
        12.0,
        TEXT_DIM,
    );
}

/// Handle keyboard and mouse input
fn handle_input(_ctx: &mut UiContext, state: &mut TrackerState) {
    // Build action context
    // Columns: 0=Note, 1=Volume, 2=Effect, 3=Effect param
    let column_type = match state.current_column {
        0 => "note",
        1 => "volume",
        2 | 3 => "effect",
        _ => "other",
    };
    let actx = build_context(
        state.playing,
        state.current_pattern().is_some(),
        column_type,
        state.editing_knob.is_some(),
        state.has_selection(),
        state.clipboard.is_some(),
    );

    // Check modifier keys
    let shift_held = is_key_down(KeyCode::LeftShift) || is_key_down(KeyCode::RightShift);
    let ctrl_held = is_key_down(KeyCode::LeftControl) || is_key_down(KeyCode::RightControl)
        || is_key_down(KeyCode::LeftSuper) || is_key_down(KeyCode::RightSuper); // Cmd on macOS

    // File operations (Ctrl+S/O/N)
    // Ctrl+N - New song
    if ctrl_held && is_key_pressed(KeyCode::N) {
        state.new_song();
    }
    // Ctrl+O - Open song browser
    if ctrl_held && is_key_pressed(KeyCode::O) {
        state.song_browser.open();
    }
    // Ctrl+S - Save song (auto-name if new)
    if ctrl_held && is_key_pressed(KeyCode::S) {
        if let Some(path) = state.current_file.clone() {
            if let Err(e) = state.save_to_file(&path) {
                state.set_status(&format!("Save failed: {}", e), 3.0);
            }
        } else {
            // No current file - use auto-generated name
            #[cfg(not(target_arch = "wasm32"))]
            {
                let path = next_available_song_name();
                if let Some(parent) = path.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                if let Err(e) = state.save_to_file(&path) {
                    state.set_status(&format!("Save failed: {}", e), 3.0);
                }
            }
            #[cfg(target_arch = "wasm32")]
            {
                state.set_status("Save not available on web", 2.0);
            }
        }
    }

    // Copy/Paste/Cut actions (handle before navigation to prevent conflicts)
    if state.actions.triggered("edit.copy", &actx) {
        state.copy_selection();
    }
    if state.actions.triggered("edit.cut", &actx) {
        state.cut_selection();
    }
    if state.actions.triggered("edit.paste", &actx) {
        state.paste();
    }
    if state.actions.triggered("edit.select_all", &actx) {
        // Select entire pattern
        if let Some(pattern) = state.current_pattern() {
            let num_ch = pattern.num_channels();
            let len = pattern.length;
            state.selection_start = Some((state.current_pattern_idx, 0, 0));
            state.selection_end = Some((state.current_pattern_idx, len - 1, num_ch - 1));
            state.set_status("Selected entire pattern", 1.0);
        }
    }

    // Navigation with selection support (Shift+Arrow extends selection)
    // Use direct key checks since action system doesn't support modifier combinations
    if is_key_pressed(KeyCode::Up) && !ctrl_held {
        if shift_held {
            if !state.has_selection() {
                state.start_selection();
            }
            state.cursor_up();
            state.update_selection();
        } else {
            state.clear_selection();
            state.cursor_up();
        }
    }
    if is_key_pressed(KeyCode::Down) && !ctrl_held {
        if shift_held {
            if !state.has_selection() {
                state.start_selection();
            }
            state.cursor_down();
            state.update_selection();
        } else {
            state.clear_selection();
            state.cursor_down();
        }
    }
    if is_key_pressed(KeyCode::Left) && !ctrl_held {
        if shift_held {
            if !state.has_selection() {
                state.start_selection();
            }
            state.cursor_left();
            state.update_selection();
        } else {
            state.clear_selection();
            state.cursor_left();
        }
    }
    if is_key_pressed(KeyCode::Right) && !ctrl_held {
        if shift_held {
            if !state.has_selection() {
                state.start_selection();
            }
            state.cursor_right();
            state.update_selection();
        } else {
            state.clear_selection();
            state.cursor_right();
        }
    }
    if state.actions.triggered("nav.next_channel", &actx) {
        state.clear_selection();
        state.next_channel();
    }
    if state.actions.triggered("nav.prev_channel", &actx) {
        state.clear_selection();
        state.prev_channel();
    }

    // Page up/down
    if is_key_pressed(KeyCode::PageUp) {
        if shift_held {
            if !state.has_selection() {
                state.start_selection();
            }
            for _ in 0..16 {
                state.cursor_up();
            }
            state.update_selection();
        } else {
            state.clear_selection();
            for _ in 0..16 {
                state.cursor_up();
            }
        }
    }
    if is_key_pressed(KeyCode::PageDown) {
        if shift_held {
            if !state.has_selection() {
                state.start_selection();
            }
            for _ in 0..16 {
                state.cursor_down();
            }
            state.update_selection();
        } else {
            state.clear_selection();
            for _ in 0..16 {
                state.cursor_down();
            }
        }
    }

    // Home/End
    if is_key_pressed(KeyCode::Home) {
        state.clear_selection();
        state.current_row = 0;
        state.scroll_row = 0;
    }
    if is_key_pressed(KeyCode::End) {
        state.clear_selection();
        if let Some(pattern) = state.current_pattern() {
            state.current_row = pattern.length - 1;
        }
    }

    // Playback
    if state.actions.triggered("playback.toggle", &actx) {
        state.toggle_playback();
    }
    if state.actions.triggered("playback.stop", &actx) {
        state.stop_playback();
    }

    // Octave (numpad only - regular +/- are piano keys now)
    if state.actions.triggered("octave.up", &actx) {
        state.octave = (state.octave + 1).min(9);
        state.set_status(&format!("Octave: {}", state.octave), 1.0);
    }
    if state.actions.triggered("octave.down", &actx) {
        state.octave = state.octave.saturating_sub(1);
        state.set_status(&format!("Octave: {}", state.octave), 1.0);
    }

    // Instrument selection removed - [ and ] are now piano keys
    // Use the instrument list in Instruments view or channel strip +/- buttons instead

    // Delete - handles selection if present
    if is_key_pressed(KeyCode::Delete) || is_key_pressed(KeyCode::Backspace) {
        state.delete_selection(); // This handles both single note and selection
    }

    // Note entry (only in Pattern view, when in edit mode and in note column)
    // Skip if Ctrl/Cmd is held (for copy/paste shortcuts)
    if state.view == TrackerView::Pattern && state.edit_mode && state.current_column == 0 && !ctrl_held {
        // All piano keys: bottom row (Z to /) and top row (Q to ])
        // Note: Period is a piano key now, so we use Apostrophe for note-off
        let note_keys = [
            // Bottom row: Z S X D C V G B H N J M , L . ; /
            KeyCode::Z, KeyCode::S, KeyCode::X, KeyCode::D, KeyCode::C,
            KeyCode::V, KeyCode::G, KeyCode::B, KeyCode::H, KeyCode::N,
            KeyCode::J, KeyCode::M, KeyCode::Comma, KeyCode::L, KeyCode::Period,
            KeyCode::Semicolon, KeyCode::Slash,
            // Top row: Q 2 W 3 E 4 R T 6 Y 7 U I 9 O 0 P - [ ]
            KeyCode::Q, KeyCode::Key2, KeyCode::W, KeyCode::Key3, KeyCode::E,
            KeyCode::Key4, KeyCode::R, KeyCode::T, KeyCode::Key6, KeyCode::Y,
            KeyCode::Key7, KeyCode::U, KeyCode::I, KeyCode::Key9, KeyCode::O,
            KeyCode::Key0, KeyCode::P, KeyCode::Minus, KeyCode::LeftBracket,
            KeyCode::RightBracket,
        ];

        for key in note_keys {
            if is_key_pressed(key) {
                if let Some(pitch) = TrackerState::key_to_note(key, state.octave) {
                    state.enter_note(pitch);
                    state.clear_selection(); // Clear selection after filling
                }
            }
            // Stop note preview when key is released
            if is_key_released(key) {
                if let Some(pitch) = TrackerState::key_to_note(key, state.octave) {
                    state.audio.note_off(state.current_channel as i32, pitch as i32);
                }
            }
        }

        // Note off with backtick (apostrophe key) - period is now a piano key
        if is_key_pressed(KeyCode::Apostrophe) {
            state.enter_note_off();
            state.clear_selection();
        }
    }

    // Volume entry (in Pattern view, edit mode, volume column = 1)
    // Type 3 digits for 0-127 (resets on each keypress, last 3 digits kept)
    // Skip if Ctrl/Cmd is held
    if state.view == TrackerView::Pattern && state.edit_mode && state.current_column == 1 && !ctrl_held {
        let digit_keys = [
            (KeyCode::Key0, 0), (KeyCode::Key1, 1), (KeyCode::Key2, 2),
            (KeyCode::Key3, 3), (KeyCode::Key4, 4), (KeyCode::Key5, 5),
            (KeyCode::Key6, 6), (KeyCode::Key7, 7), (KeyCode::Key8, 8),
            (KeyCode::Key9, 9),
        ];

        for (key, digit) in digit_keys {
            if is_key_pressed(key) {
                // Get current volume or 0
                let current = state.current_pattern()
                    .and_then(|p| p.get(state.current_channel, state.current_row))
                    .and_then(|n| n.volume)
                    .unwrap_or(0) as u16;
                // Shift left and add new digit, keep last 3 digits, clamp to 127
                let new_vol = ((current * 10 + digit as u16) % 1000).min(127) as u8;
                state.set_volume(new_vol);
            }
        }
    }

    // Effect entry (in Pattern view, edit mode, effect column = 2)
    // Skip if Ctrl/Cmd is held
    if state.view == TrackerView::Pattern && state.edit_mode && state.current_column == 2 && !ctrl_held {
        // Effect letters: 0-9, A-F for standard effects, + our new ones (C, E, H, M, P, R)
        let effect_keys = [
            (KeyCode::Key0, '0'), (KeyCode::Key1, '1'), (KeyCode::Key2, '2'),
            (KeyCode::Key3, '3'), (KeyCode::Key4, '4'), (KeyCode::Key5, '5'),
            (KeyCode::Key6, '6'), (KeyCode::Key7, '7'), (KeyCode::Key8, '8'),
            (KeyCode::Key9, '9'),
            (KeyCode::A, 'A'), (KeyCode::B, 'B'), (KeyCode::C, 'C'),
            (KeyCode::D, 'D'), (KeyCode::E, 'E'), (KeyCode::F, 'F'),
            (KeyCode::H, 'H'), (KeyCode::M, 'M'), (KeyCode::P, 'P'), (KeyCode::R, 'R'),
        ];

        for (key, ch) in effect_keys {
            if is_key_pressed(key) {
                state.set_effect_char(ch);
                state.set_status(&format!("Effect: {}", ch), 1.0);
            }
        }
    }

    // Effect parameter entry (in Pattern view, edit mode, fx_param column = 3)
    // Type digits for 0-255 (shift left and add, keep last 3 digits)
    // Skip if Ctrl/Cmd is held
    if state.view == TrackerView::Pattern && state.edit_mode && state.current_column == 3 && !ctrl_held {
        let digit_keys = [
            (KeyCode::Key0, 0), (KeyCode::Key1, 1), (KeyCode::Key2, 2),
            (KeyCode::Key3, 3), (KeyCode::Key4, 4), (KeyCode::Key5, 5),
            (KeyCode::Key6, 6), (KeyCode::Key7, 7), (KeyCode::Key8, 8),
            (KeyCode::Key9, 9),
        ];

        for (key, digit) in digit_keys {
            if is_key_pressed(key) {
                // Get current param or 0
                let current = state.current_pattern()
                    .and_then(|p| p.get(state.current_channel, state.current_row))
                    .and_then(|n| n.effect_param)
                    .unwrap_or(0) as u16;
                // Shift left and add new digit, keep last 3 digits, clamp to 127
                let new_param = ((current * 10 + digit as u16) % 1000).min(127) as u8;
                state.set_effect_param(new_param);
            }
        }
    }

}
