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
use super::audio::OutputSampleRate;
use super::actions::build_context;

// Layout constants
const ROW_HEIGHT: f32 = 18.0;
const CHANNEL_WIDTH: f32 = 116.0; // Note + Vol + Fx + FxParam (no per-channel reverb)
const ROW_NUM_WIDTH: f32 = 30.0;
const NOTE_WIDTH: f32 = 36.0;
// Instrument column removed - instrument is now per-channel in the channel strip
const VOL_WIDTH: f32 = 24.0;
const FX_WIDTH: f32 = 16.0;
const FXPARAM_WIDTH: f32 = 24.0;
const GLOBAL_REVERB_WIDTH: f32 = 24.0; // Single global reverb column (0-9)

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
        TrackerView::Instruments => draw_instruments_view(ctx, main_rect, state),
    }

    // Draw status bar at bottom
    draw_status_bar(status_rect, state);

    // Handle input
    handle_input(ctx, state);
}

/// Draw the header with transport controls and song info
fn draw_header(ctx: &mut UiContext, rect: Rect, state: &mut TrackerState, icon_font: Option<&Font>) {
    draw_rectangle(rect.x, rect.y, rect.w, rect.h, HEADER_COLOR);

    // First row: toolbar with icons (36.0 height to match World Editor)
    let toolbar_rect = Rect::new(rect.x, rect.y, rect.w, 36.0);
    let mut toolbar = Toolbar::new(toolbar_rect);

    // File operations (native only - no file dialogs on WASM)
    #[cfg(not(target_arch = "wasm32"))]
    {
        if toolbar.icon_button(ctx, icon::FILE_PLUS, icon_font, "New Song (Ctrl+N)") {
            state.new_song();
        }
        if toolbar.icon_button(ctx, icon::FOLDER_OPEN, icon_font, "Open Song (Ctrl+O)") {
            if let Some(path) = rfd::FileDialog::new()
                .add_filter("Bonnie Song", &["bsong"])
                .pick_file()
            {
                if let Err(e) = state.load_from_file(&path) {
                    state.set_status(&format!("Load failed: {}", e), 3.0);
                }
            }
        }
        // Save button - show dirty indicator
        let save_icon = if state.dirty { icon::SAVE } else { icon::SAVE };
        let save_tooltip = if state.current_file.is_some() { "Save (Ctrl+S)" } else { "Save As (Ctrl+S)" };
        if toolbar.icon_button(ctx, save_icon, icon_font, save_tooltip) {
            if let Some(path) = state.current_file.clone() {
                if let Err(e) = state.save_to_file(&path) {
                    state.set_status(&format!("Save failed: {}", e), 3.0);
                }
            } else {
                // No current file - prompt for location
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter("Bonnie Song", &["bsong"])
                    .set_file_name("song.bsong")
                    .save_file()
                {
                    if let Err(e) = state.save_to_file(&path) {
                        state.set_status(&format!("Save failed: {}", e), 3.0);
                    }
                }
            }
        }
        toolbar.separator();
    }

    // View mode buttons
    let view_icons = [
        (TrackerView::Pattern, icon::GRID, "Pattern Editor"),
        (TrackerView::Arrangement, icon::NOTEBOOK_PEN, "Arrangement"),
        (TrackerView::Instruments, icon::PIANO, "Instruments"),
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

    // BPM controls
    toolbar.label(&format!("BPM:{:3}", state.song.bpm));
    if toolbar.icon_button(ctx, icon::MINUS, icon_font, "Decrease BPM") {
        state.song.bpm = (state.song.bpm as i32 - 5).clamp(40, 300) as u16;
    }
    if toolbar.icon_button(ctx, icon::PLUS, icon_font, "Increase BPM") {
        state.song.bpm = (state.song.bpm as i32 + 5).clamp(40, 300) as u16;
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

    toolbar.separator();

    // Global sample rate (PS1 lo-fi effect - applies to all audio output)
    let current_rate = state.audio.output_sample_rate();
    toolbar.label(&format!("Rate:{}", current_rate.name()));
    if toolbar.icon_button(ctx, icon::MINUS, icon_font, "Lower Sample Rate (more lo-fi)") {
        let presets = OutputSampleRate::PRESETS;
        if let Some(idx) = presets.iter().position(|p| p.0 == current_rate.0) {
            if idx + 1 < presets.len() {
                state.audio.set_output_sample_rate(presets[idx + 1]);
            }
        }
    }
    if toolbar.icon_button(ctx, icon::PLUS, icon_font, "Higher Sample Rate (less lo-fi)") {
        let presets = OutputSampleRate::PRESETS;
        if let Some(idx) = presets.iter().position(|p| p.0 == current_rate.0) {
            if idx > 0 {
                state.audio.set_output_sample_rate(presets[idx - 1]);
            }
        }
    }

    // Note: Global reverb type control removed from toolbar
    // Reverb type is now set via the Rxx effect command in the pattern Fx column

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

/// Height of the channel strip header (instrument selector, knobs, reset button)
const CHANNEL_STRIP_HEIGHT: f32 = 120.0;

/// Draw the pattern editor view
fn draw_pattern_view(ctx: &mut UiContext, rect: Rect, state: &mut TrackerState) {
    let num_channels = state.num_channels();

    // Calculate visible rows (accounting for channel strip header)
    state.visible_rows = ((rect.h - CHANNEL_STRIP_HEIGHT - ROW_HEIGHT) / ROW_HEIGHT) as usize;

    // Get pattern info without holding borrow
    let (pattern_length, rows_per_beat) = match state.current_pattern() {
        Some(p) => (p.length, state.song.rows_per_beat),
        None => return,
    };

    // === Channel strip header (instrument selector, knobs, etc.) ===
    draw_rectangle(rect.x, rect.y, rect.w, CHANNEL_STRIP_HEIGHT, Color::new(0.12, 0.12, 0.14, 1.0));

    // Collect channel updates to apply after the loop (avoid borrow conflicts)
    let mut channel_updates: Vec<(usize, &str, u8)> = Vec::new();

    let mut x = rect.x + ROW_NUM_WIDTH;
    for ch in 0..num_channels {
        let ch_x = x;
        let is_current = ch == state.current_channel;

        // Background for selected channel
        if is_current {
            draw_rectangle(ch_x, rect.y, CHANNEL_WIDTH - 1.0, CHANNEL_STRIP_HEIGHT, Color::new(0.18, 0.2, 0.24, 1.0));
        }

        // === Row 1: Channel number (centered) ===
        let row1_y = rect.y + 2.0;
        let ch_color = if is_current { NOTE_COLOR } else { TEXT_COLOR };
        let ch_label = format!("Ch{}", ch + 1);
        let ch_dims = measure_text(&ch_label, None, 12, 1.0);
        draw_text(&ch_label, ch_x + (CHANNEL_WIDTH - ch_dims.width) / 2.0, row1_y + 12.0, 12.0, ch_color);

        // === Row 2: Instrument selector (full width): [-] [name] [+] ===
        let row2_y = rect.y + 18.0;
        let inst = state.song.get_channel_instrument(ch);
        let presets = state.audio.get_preset_names();
        let inst_name = presets
            .iter()
            .find(|(_, p, _)| *p == inst)
            .map(|(_, _, n)| n.as_str())
            .unwrap_or("---");

        // Truncate instrument name to fit
        let display_name: String = if inst_name.len() > 12 {
            format!("{:.12}", inst_name)
        } else {
            inst_name.to_string()
        };

        // [-] button
        let btn_size = 16.0;
        let btn_margin = 2.0;
        let inst_minus_rect = Rect::new(ch_x + btn_margin, row2_y, btn_size, btn_size);
        let inst_minus_hover = ctx.mouse.inside(&inst_minus_rect);
        draw_rectangle(inst_minus_rect.x, inst_minus_rect.y, inst_minus_rect.w, inst_minus_rect.h,
            if inst_minus_hover { Color::new(0.3, 0.3, 0.35, 1.0) } else { Color::new(0.2, 0.2, 0.25, 1.0) });
        draw_text("-", inst_minus_rect.x + 5.0, inst_minus_rect.y + 12.0, 12.0, TEXT_COLOR);
        if inst_minus_hover && is_mouse_button_pressed(MouseButton::Left) {
            let new_inst = inst.saturating_sub(1);
            channel_updates.push((ch, "inst", new_inst));
        }

        // Instrument name display (centered between buttons)
        let name_text = format!("{:02}:{}", inst, display_name);
        let name_dims = measure_text(&name_text, None, 11, 1.0);
        let name_area_start = ch_x + btn_margin + btn_size + 2.0;
        let name_area_end = ch_x + CHANNEL_WIDTH - btn_margin - btn_size - 2.0;
        let name_x = name_area_start + (name_area_end - name_area_start - name_dims.width) / 2.0;
        draw_text(&name_text, name_x, row2_y + 11.0, 11.0, INST_COLOR);

        // [+] button
        let inst_plus_rect = Rect::new(ch_x + CHANNEL_WIDTH - btn_margin - btn_size, row2_y, btn_size, btn_size);
        let inst_plus_hover = ctx.mouse.inside(&inst_plus_rect);
        draw_rectangle(inst_plus_rect.x, inst_plus_rect.y, inst_plus_rect.w, inst_plus_rect.h,
            if inst_plus_hover { Color::new(0.3, 0.3, 0.35, 1.0) } else { Color::new(0.2, 0.2, 0.25, 1.0) });
        draw_text("+", inst_plus_rect.x + 4.0, inst_plus_rect.y + 12.0, 12.0, TEXT_COLOR);
        if inst_plus_hover && is_mouse_button_pressed(MouseButton::Left) {
            let new_inst = (inst + 1).min(127);
            channel_updates.push((ch, "inst", new_inst));
        }

        // === Row 3: 3 Mini Knobs (Pan, Mod, Expr) ===
        let settings = state.song.get_channel_settings(ch);
        let knob_radius = 16.0;
        let knob_spacing = 38.0;
        let knobs_y = rect.y + 58.0;
        // Center the 3 knobs
        let knobs_total_width = knob_spacing * 2.0 + knob_radius * 2.0;
        let knobs_start_x = ch_x + (CHANNEL_WIDTH - knobs_total_width) / 2.0 + knob_radius;

        if let Some(new_val) = draw_mini_knob(ctx, knobs_start_x, knobs_y, knob_radius, settings.pan, "Pan", true) {
            channel_updates.push((ch, "pan", new_val));
        }
        if let Some(new_val) = draw_mini_knob(ctx, knobs_start_x + knob_spacing, knobs_y, knob_radius, settings.modulation, "Mod", false) {
            channel_updates.push((ch, "mod", new_val));
        }
        if let Some(new_val) = draw_mini_knob(ctx, knobs_start_x + knob_spacing * 2.0, knobs_y, knob_radius, settings.expression, "Expr", false) {
            channel_updates.push((ch, "expr", new_val));
        }

        // === Row 4: Reset button ===
        let reset_y = rect.y + CHANNEL_STRIP_HEIGHT - 20.0;
        let reset_rect = Rect::new(ch_x + 4.0, reset_y, CHANNEL_WIDTH - 10.0, 16.0);
        let reset_hover = ctx.mouse.inside(&reset_rect);
        draw_rectangle(reset_rect.x, reset_rect.y, reset_rect.w, reset_rect.h,
            if reset_hover { Color::new(0.4, 0.28, 0.28, 1.0) } else { Color::new(0.25, 0.2, 0.2, 1.0) });
        let reset_text = "Reset";
        let reset_dims = measure_text(reset_text, None, 11, 1.0);
        draw_text(reset_text, reset_rect.x + (reset_rect.w - reset_dims.width) / 2.0, reset_y + 12.0, 11.0, TEXT_DIM);
        if reset_hover && is_mouse_button_pressed(MouseButton::Left) {
            channel_updates.push((ch, "reset", 0));
        }

        x += CHANNEL_WIDTH;

        // Channel separator
        draw_line(x - 1.0, rect.y, x - 1.0, rect.y + rect.h, 1.0, Color::new(0.25, 0.25, 0.3, 1.0));
    }

    // Apply channel updates
    for (ch, param, value) in channel_updates {
        match param {
            "inst" => {
                state.song.set_channel_instrument(ch, value);
                state.audio.set_program(ch as i32, value as i32);
            }
            "pan" => state.set_channel_pan(ch, value),
            "mod" => state.set_channel_modulation(ch, value),
            "expr" => state.set_channel_expression(ch, value),
            "reset" => state.reset_channel_settings(ch),
            _ => {}
        }
    }

    // === Global reverb controls (above the Rv column) ===
    let reverb_strip_x = rect.x + ROW_NUM_WIDTH + (num_channels as f32 * CHANNEL_WIDTH);
    let reverb_strip_bg = Color::new(0.10, 0.12, 0.16, 1.0);
    draw_rectangle(reverb_strip_x, rect.y, 50.0, CHANNEL_STRIP_HEIGHT, reverb_strip_bg);

    // Global Wet knob (PS1 has single global reverb processor) - same size as channel knobs
    let wet_value = (state.audio.reverb_wet_level() * 127.0) as u8;
    let wet_knob_x = reverb_strip_x + 25.0; // Centered: (50 - 32) / 2 + 16
    let wet_knob_y = rect.y + 58.0; // Same Y as channel knobs (row 3)
    if let Some(new_val) = draw_mini_knob(ctx, wet_knob_x, wet_knob_y, 16.0, wet_value, "Wet", false) {
        state.audio.set_reverb_wet_level(new_val as f32 / 127.0);
    }

    // === Column headers (Note, Inst, Vol, etc.) ===
    let header_y = rect.y + CHANNEL_STRIP_HEIGHT;
    draw_rectangle(rect.x, header_y, rect.w, ROW_HEIGHT, HEADER_COLOR);

    x = rect.x + ROW_NUM_WIDTH;
    for ch in 0..num_channels {
        let ch_x = x;
        let header_rect = Rect::new(ch_x, header_y, CHANNEL_WIDTH, ROW_HEIGHT);

        // Highlight on hover
        if ctx.mouse.inside(&header_rect) {
            draw_rectangle(ch_x, header_y, CHANNEL_WIDTH, ROW_HEIGHT, Color::new(0.25, 0.25, 0.3, 1.0));

            // Click to select channel
            if is_mouse_button_pressed(MouseButton::Left) {
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

    // Global reverb column header (after all channels)
    let reverb_header_x = x;
    let is_reverb_selected = state.current_column == 4;
    let rv_label_color = if is_reverb_selected { NOTE_COLOR } else { TEXT_DIM };
    draw_text("Rv", reverb_header_x + 2.0, header_y + 14.0, 12.0, rv_label_color);

    // Handle mouse clicks and scrolling on pattern grid
    let grid_y_start = rect.y + CHANNEL_STRIP_HEIGHT + ROW_HEIGHT;
    let grid_rect = Rect::new(rect.x, grid_y_start, rect.w, rect.h - CHANNEL_STRIP_HEIGHT - ROW_HEIGHT);

    // Mouse wheel scrolling
    if ctx.mouse.inside(&grid_rect) {
        let scroll = mouse_wheel().1;
        if scroll != 0.0 {
            let scroll_amount = if scroll > 0.0 { -4 } else { 4 }; // Scroll 4 rows at a time
            let new_scroll = (state.scroll_row as i32 + scroll_amount).max(0) as usize;
            state.scroll_row = new_scroll.min(pattern_length.saturating_sub(state.visible_rows));
        }
    }

    if ctx.mouse.inside(&grid_rect) && is_mouse_button_pressed(MouseButton::Left) {
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

            // Check if clicked on global reverb column (after all channels)
            let reverb_col_x = ROW_NUM_WIDTH + (num_channels as f32 * CHANNEL_WIDTH);
            if rel_x >= reverb_col_x - ROW_NUM_WIDTH && rel_x < reverb_col_x + GLOBAL_REVERB_WIDTH - ROW_NUM_WIDTH {
                state.current_column = 4; // Global reverb column
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
    let visible_rows = state.visible_rows;
    let end_row = (start_row + visible_rows).min(pattern.length);
    let pattern_num_channels = pattern.num_channels();

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
        for ch in 0..pattern_num_channels {
            let note = &pattern.channels[ch][row_idx];

            // Selection highlight (draw before cursor so cursor overlays selection)
            if state.is_in_selection(row_idx, ch) {
                // Selection color: semi-transparent blue
                let selection_color = Color::new(0.2, 0.4, 0.7, 0.5);
                draw_rectangle(x, y, CHANNEL_WIDTH - 4.0, ROW_HEIGHT, selection_color);
            }

            // Cursor highlight for channel columns (0-3)
            // Columns: 0=Note, 1=Volume, 2=Effect, 3=Effect param
            if row_idx == state.current_row && ch == state.current_channel && state.current_column < 4 {
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
            let vol_str = note.volume.map(|v| format!("{:02X}", v)).unwrap_or_else(|| "--".to_string());
            let vol_color = if note.volume.is_some() { VOL_COLOR } else { TEXT_DIM };
            draw_text(&vol_str, x + NOTE_WIDTH + 2.0, y + 14.0, 12.0, vol_color);

            // Effect
            let fx_str = note.effect.map(|e| e.to_string()).unwrap_or_else(|| "-".to_string());
            let fx_color = if note.effect.is_some() { FX_COLOR } else { TEXT_DIM };
            draw_text(&fx_str, x + NOTE_WIDTH + VOL_WIDTH + 2.0, y + 14.0, 12.0, fx_color);

            // Effect param
            let fxp_str = note.effect_param.map(|p| format!("{:02X}", p)).unwrap_or_else(|| "--".to_string());
            draw_text(&fxp_str, x + NOTE_WIDTH + VOL_WIDTH + FX_WIDTH + 2.0, y + 14.0, 12.0, fx_color);

            x += CHANNEL_WIDTH;
        }

        // Global reverb column (single column after all channels)
        let reverb_x = x;
        let reverb = pattern.get_reverb(row_idx);

        // Cursor highlight for global reverb column (column 4)
        if row_idx == state.current_row && state.current_column == 4 {
            draw_rectangle(reverb_x, y, GLOBAL_REVERB_WIDTH, ROW_HEIGHT, CURSOR_COLOR);
        }

        // Reverb value (single digit 0-9)
        let rv_str = reverb.map(|r| format!("{}", r.min(9))).unwrap_or_else(|| "-".to_string());
        let rv_color = if reverb.is_some() { Color::new(0.6, 0.8, 1.0, 1.0) } else { TEXT_DIM };
        draw_text(&rv_str, reverb_x + 6.0, y + 14.0, 12.0, rv_color);
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
        let mouse = mouse_position();
        if mouse.0 >= bank_rect.x && mouse.0 <= bank_rect.x + bank_rect.w
            && mouse.1 >= y && mouse.1 <= y + row_h - 2.0
        {
            if is_mouse_button_pressed(MouseButton::Left) {
                unsafe {
                    PATTERN_BANK_SELECTION = i;
                    ARRANGEMENT_FOCUS = false;
                }
            }
            // Double-click to add to arrangement
            // (We'd need double-click detection, for now use right-click)
            if is_mouse_button_pressed(MouseButton::Right) {
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
        let mouse = mouse_position();
        if mouse.0 >= arr_rect.x && mouse.0 <= arr_rect.x + arr_rect.w
            && mouse.1 >= y && mouse.1 <= y + row_h - 2.0
        {
            if is_mouse_button_pressed(MouseButton::Left) {
                unsafe {
                    ARRANGEMENT_SELECTION = i;
                    ARRANGEMENT_FOCUS = true;
                }
            }
            // Double-click (or Enter) to jump to that position
            if is_mouse_button_pressed(MouseButton::Right) {
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
        // Pattern:  2  3  4     5  6  7     8  9  0
        //          Q  W  E  R  T  Y  U  I  O  P  [  ]
        17 => Some("Q"), 18 => Some("2"), 19 => Some("W"), 20 => Some("3"), 21 => Some("E"),
        22 => Some("4"), 23 => Some("R"), 24 => Some("T"), 25 => Some("5"), 26 => Some("Y"),
        27 => Some("6"), 28 => Some("U"), 29 => Some("I"), 30 => Some("8"), 31 => Some("O"),
        32 => Some("9"), 33 => Some("P"), 34 => Some("0"), 35 => Some("["), 36 => Some("]"),
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
        25 => Some(KeyCode::Key5), 26 => Some(KeyCode::Y), 27 => Some(KeyCode::Key6), 28 => Some(KeyCode::U),
        29 => Some(KeyCode::I), 30 => Some(KeyCode::Key8), 31 => Some(KeyCode::O), 32 => Some(KeyCode::Key9),
        33 => Some(KeyCode::P), 34 => Some(KeyCode::Key0), 35 => Some(KeyCode::LeftBracket),
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
    if ctx.mouse.inside(&list_content_rect) {
        let scroll = mouse_wheel().1;
        if scroll != 0.0 {
            let scroll_amount = if scroll > 0.0 { -3 } else { 3 }; // Scroll 3 items at a time
            let new_scroll = (state.instrument_scroll as i32 + scroll_amount).max(0) as usize;
            state.instrument_scroll = new_scroll.min(max_scroll);
        }
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
        if is_hovered && is_mouse_button_pressed(MouseButton::Left) {
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
        let is_mouse_pressed = is_hovered && is_mouse_button_down(MouseButton::Left);

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
        if is_hovered && is_mouse_button_pressed(MouseButton::Left) {
            state.audio.note_on(state.current_channel as i32, midi_note as i32, 100);
        }
        if is_hovered && is_mouse_button_released(MouseButton::Left) {
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
        let is_mouse_pressed = is_hovered && is_mouse_button_down(MouseButton::Left);

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
        if is_hovered && is_mouse_button_pressed(MouseButton::Left) {
            state.audio.note_on(state.current_channel as i32, midi_note as i32, 100);
        }
        if is_hovered && is_mouse_button_released(MouseButton::Left) {
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

    // === SPU SAMPLE RATE (with toggle) ===
    let sample_rate_y = info_y + 25.0;
    let spu_enabled = state.audio.is_spu_resampling_enabled();

    draw_text("SPU Sample Rate", piano_x, sample_rate_y, 16.0, TEXT_COLOR);

    let sr_btn_w = 52.0;
    let sr_btn_h = 22.0;
    let sr_spacing = 4.0;
    let btn_y = sample_rate_y + 10.0;

    // OFF button (first button)
    let off_btn_x = piano_x;
    let off_rect = Rect::new(off_btn_x, btn_y, sr_btn_w, sr_btn_h);
    let off_hovered = ctx.mouse.inside(&off_rect);
    let off_active = !spu_enabled;

    let off_bg = if off_active {
        Color::new(0.2, 0.4, 0.5, 1.0) // Cyan-ish for active
    } else if off_hovered {
        Color::new(0.25, 0.25, 0.3, 1.0)
    } else {
        Color::new(0.15, 0.15, 0.18, 1.0)
    };
    draw_rectangle(off_btn_x, btn_y, sr_btn_w, sr_btn_h, off_bg);
    let off_text_color = if off_active { WHITE } else { TEXT_COLOR };
    draw_text("OFF", off_btn_x + 16.0, btn_y + 15.0, 12.0, off_text_color);

    if off_hovered && is_mouse_button_pressed(MouseButton::Left) && spu_enabled {
        state.audio.set_spu_resampling_enabled(false);
        state.set_status("SPU resampling disabled", 1.0);
    }

    // Sample rate buttons (shifted by 1 to make room for OFF)
    let current_sample_rate = state.audio.output_sample_rate();

    for (i, rate) in OutputSampleRate::ALL.iter().enumerate() {
        let btn_x = piano_x + (i + 1) as f32 * (sr_btn_w + sr_spacing);

        let btn_rect = Rect::new(btn_x, btn_y, sr_btn_w, sr_btn_h);
        let is_active = spu_enabled && *rate == current_sample_rate;
        let is_hovered = ctx.mouse.inside(&btn_rect);

        let bg = if is_active {
            Color::new(0.2, 0.4, 0.5, 1.0) // Cyan-ish for active
        } else if is_hovered {
            Color::new(0.25, 0.25, 0.3, 1.0)
        } else {
            Color::new(0.15, 0.15, 0.18, 1.0)
        };

        draw_rectangle(btn_x, btn_y, sr_btn_w, sr_btn_h, bg);
        let text_color = if is_active { WHITE } else { TEXT_COLOR };
        draw_text(rate.name(), btn_x + 12.0, btn_y + 15.0, 12.0, text_color);

        if is_hovered && is_mouse_button_pressed(MouseButton::Left) {
            // Enable SPU and set sample rate
            if !spu_enabled {
                state.audio.set_spu_resampling_enabled(true);
            }
            state.audio.set_output_sample_rate(*rate);
            state.set_status(&format!("SPU sample rate: {}", rate.name()), 1.0);
        }
    }

    // === PS1 REVERB PRESETS ===
    let reverb_y = sample_rate_y + 45.0;
    draw_text("PS1 Reverb", piano_x, reverb_y, 16.0, TEXT_COLOR);

    let preset_btn_w = 80.0;
    let preset_btn_h = 22.0;
    let preset_spacing = 4.0;
    let presets_per_row = 5;

    let current_reverb = state.audio.reverb_type();

    for (i, reverb_type) in ReverbType::ALL.iter().enumerate() {
        let row = i / presets_per_row;
        let col = i % presets_per_row;
        let btn_x = piano_x + col as f32 * (preset_btn_w + preset_spacing);
        let btn_y = reverb_y + 10.0 + row as f32 * (preset_btn_h + preset_spacing);

        let btn_rect = Rect::new(btn_x, btn_y, preset_btn_w, preset_btn_h);
        let is_active = *reverb_type == current_reverb;
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
        draw_text(reverb_type.name(), btn_x + 5.0, btn_y + 15.0, 12.0, text_color);

        if is_hovered && is_mouse_button_pressed(MouseButton::Left) {
            state.audio.set_reverb_preset(*reverb_type);
            state.set_status(&format!("Reverb: {}", reverb_type.name()), 1.0);
        }
    }

    // Wet/Dry knob for reverb
    let wet_knob_x = piano_x + 5.0 * (preset_btn_w + preset_spacing) + 40.0;
    let wet_knob_y = reverb_y + 35.0;
    let wet_value = (state.audio.reverb_wet_level() * 127.0) as u8;

    let wet_result = draw_knob(
        ctx,
        wet_knob_x,
        wet_knob_y,
        28.0,
        wet_value,
        "Wet",
        false,
        false,
    );

    if let Some(new_val) = wet_result.value {
        state.audio.set_reverb_wet_level(new_val as f32 / 127.0);
    }

    // === CHANNEL EFFECT KNOBS ===
    let effects_y = reverb_y + 80.0;
    let ch = state.current_channel;

    draw_text("Channel Effects", piano_x, effects_y, 16.0, TEXT_COLOR);

    let knob_radius = 28.0;
    let knob_spacing = 70.0;
    let knob_y = effects_y + 50.0;

    // Knob definitions: (index, label, value, is_bipolar) - PS1 authentic only
    let knob_data = [
        (0, "Pan", state.preview_pan[ch], true),
        (1, "Mod", state.preview_modulation[ch], false),
        (2, "Expr", state.preview_expression[ch], false),
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

        // Enter to confirm
        if is_key_pressed(KeyCode::Enter) {
            if let Ok(val) = state.knob_edit_text.parse::<u8>() {
                let clamped = val.min(127);
                match editing_idx {
                    0 => state.set_preview_pan(clamped),
                    1 => state.set_preview_modulation(clamped),
                    2 => state.set_preview_expression(clamped),
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

        // Handle knob value change
        if let Some(new_val) = result.value {
            match idx {
                0 => state.set_preview_pan(new_val),
                1 => state.set_preview_modulation(new_val),
                2 => state.set_preview_expression(new_val),
                _ => {}
            }
        }

        // Handle editing start
        if result.editing {
            state.editing_knob = Some(*idx);
            state.knob_edit_text = format!("{}", value);
        }
    }

    // Reset button
    let reset_y = knob_y + knob_radius + 35.0;
    let reset_rect = Rect::new(piano_x, reset_y, 100.0, 20.0);
    let reset_hovered = ctx.mouse.inside(&reset_rect);

    draw_rectangle(reset_rect.x, reset_rect.y, reset_rect.w, reset_rect.h,
        if reset_hovered { Color::new(0.25, 0.25, 0.3, 1.0) } else { Color::new(0.18, 0.18, 0.22, 1.0) });
    draw_text("Reset All", reset_rect.x + 22.0, reset_rect.y + 14.0, 12.0, TEXT_COLOR);

    if reset_hovered && is_mouse_button_pressed(MouseButton::Left) {
        state.reset_preview_effects();
        state.set_status("Effects reset to defaults", 1.0);
    }

    // Help text
    let help_y = reset_y + 35.0;
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
            // Columns: 0=Note, 1=Volume, 2=Effect, 3=Effect param, 4=Global Reverb
            match state.current_column {
                0 => "Note: Z-/ Q-] piano keys | ` note-off | Del clear",
                1 => "Volume: 00-7F (hex) | Del clear",
                2 => "Effect: 0=Arp 1=SlideUp 2=SlideDown 3=Porta 4=Vib A=VolSlide C=Vol E=Expr M=Mod P=Pan",
                3 => "Effect Param: 00-FF (hex) | Del clear",
                _ => "Global Reverb (PS1 SPU): 0=Off 1=Room 2=StudioS 3=StudioM 4=StudioL 5=Hall 6=HalfEcho 7=SpaceEcho 8=Chaos 9=Delay",
            }
        }
        TrackerView::Arrangement => {
            "Tab: switch focus | Enter: edit pattern | +: new pattern | Del: remove | Shift+↑↓: reorder"
        }
        TrackerView::Instruments => {
            "Z-/ Q-]: preview notes | Click instrument to select | Drag knobs to adjust"
        }
    };

    draw_text(column_help, rect.x + 10.0, rect.y + 15.0, 14.0, TEXT_COLOR);

    // Right side: Global shortcuts
    #[cfg(not(target_arch = "wasm32"))]
    let shortcuts = "Ctrl+S: Save | Ctrl+O: Open | Ctrl+N: New | Space: Play/Pause";
    #[cfg(target_arch = "wasm32")]
    let shortcuts = "Space: Play/Pause | Esc: Stop | Numpad+/-: Octave";

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
    // Columns: 0=Note, 1=Volume, 2=Effect, 3=Effect param, 4=Global Reverb
    let column_type = match state.current_column {
        0 => "note",
        1 => "volume",
        2 => "effect",
        3 => "effect",
        4 => "reverb", // Global reverb column
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

    // File operations (native only - Ctrl+S/O/N)
    #[cfg(not(target_arch = "wasm32"))]
    {
        // Ctrl+N - New song
        if ctrl_held && is_key_pressed(KeyCode::N) {
            state.new_song();
        }
        // Ctrl+O - Open song
        if ctrl_held && is_key_pressed(KeyCode::O) {
            if let Some(path) = rfd::FileDialog::new()
                .add_filter("Bonnie Song", &["bsong"])
                .pick_file()
            {
                if let Err(e) = state.load_from_file(&path) {
                    state.set_status(&format!("Load failed: {}", e), 3.0);
                }
            }
        }
        // Ctrl+S - Save song
        if ctrl_held && is_key_pressed(KeyCode::S) {
            if let Some(path) = state.current_file.clone() {
                if let Err(e) = state.save_to_file(&path) {
                    state.set_status(&format!("Save failed: {}", e), 3.0);
                }
            } else {
                // No current file - prompt for location
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter("Bonnie Song", &["bsong"])
                    .set_file_name("song.bsong")
                    .save_file()
                {
                    if let Err(e) = state.save_to_file(&path) {
                        state.set_status(&format!("Save failed: {}", e), 3.0);
                    }
                }
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
            // Top row: Q 2 W 3 E 4 R T 5 Y 6 U I 8 O 9 P 0 [ ]
            KeyCode::Q, KeyCode::Key2, KeyCode::W, KeyCode::Key3, KeyCode::E,
            KeyCode::Key4, KeyCode::R, KeyCode::T, KeyCode::Key5, KeyCode::Y,
            KeyCode::Key6, KeyCode::U, KeyCode::I, KeyCode::Key8, KeyCode::O,
            KeyCode::Key9, KeyCode::P, KeyCode::Key0, KeyCode::LeftBracket,
            KeyCode::RightBracket,
        ];

        for key in note_keys {
            if is_key_pressed(key) {
                if let Some(pitch) = TrackerState::key_to_note(key, state.octave) {
                    state.enter_note(pitch);
                    state.clear_selection(); // Clear selection after filling
                }
            }
        }

        // Note off with backtick (apostrophe key) - period is now a piano key
        if is_key_pressed(KeyCode::Apostrophe) {
            state.enter_note_off();
            state.clear_selection();
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
    // Skip if Ctrl/Cmd is held
    if state.view == TrackerView::Pattern && state.edit_mode && state.current_column == 3 && !ctrl_held {
        // Hex digits 0-9, A-F for parameter entry
        let hex_keys = [
            (KeyCode::Key0, 0), (KeyCode::Key1, 1), (KeyCode::Key2, 2),
            (KeyCode::Key3, 3), (KeyCode::Key4, 4), (KeyCode::Key5, 5),
            (KeyCode::Key6, 6), (KeyCode::Key7, 7), (KeyCode::Key8, 8),
            (KeyCode::Key9, 9),
            (KeyCode::A, 10), (KeyCode::B, 11), (KeyCode::C, 12),
            (KeyCode::D, 13), (KeyCode::E, 14), (KeyCode::F, 15),
        ];

        for (key, nibble) in hex_keys {
            if is_key_pressed(key) {
                // Shift left and add new nibble (so you type XX as two keypresses)
                state.set_effect_param_high(state.current_pattern()
                    .and_then(|p| p.get(state.current_channel, state.current_row))
                    .and_then(|n| n.effect_param)
                    .map(|p| p & 0x0F)
                    .unwrap_or(0));
                state.set_effect_param_low(nibble);
            }
        }
    }

    // Reverb entry (in Pattern view, edit mode, reverb column = 4)
    // Skip if Ctrl/Cmd is held
    if state.view == TrackerView::Pattern && state.edit_mode && state.current_column == 4 && !ctrl_held {
        // Digits 0-9 for reverb preset
        let reverb_keys = [
            (KeyCode::Key0, 0), (KeyCode::Key1, 1), (KeyCode::Key2, 2),
            (KeyCode::Key3, 3), (KeyCode::Key4, 4), (KeyCode::Key5, 5),
            (KeyCode::Key6, 6), (KeyCode::Key7, 7), (KeyCode::Key8, 8),
            (KeyCode::Key9, 9),
        ];

        for (key, preset) in reverb_keys {
            if is_key_pressed(key) {
                state.set_reverb(preset);
            }
        }
    }

    // In Instruments view, allow keyboard to preview sounds without entering notes
    // Skip if Ctrl/Cmd is held
    if state.view == TrackerView::Instruments && !ctrl_held {
        // All piano keys: bottom row (Z to /) and top row (Q to ])
        let note_keys = [
            // Bottom row: Z S X D C V G B H N J M , L . ; /
            KeyCode::Z, KeyCode::S, KeyCode::X, KeyCode::D, KeyCode::C,
            KeyCode::V, KeyCode::G, KeyCode::B, KeyCode::H, KeyCode::N,
            KeyCode::J, KeyCode::M, KeyCode::Comma, KeyCode::L, KeyCode::Period,
            KeyCode::Semicolon, KeyCode::Slash,
            // Top row: Q 2 W 3 E 4 R T 5 Y 6 U I 8 O 9 P 0 [ ]
            KeyCode::Q, KeyCode::Key2, KeyCode::W, KeyCode::Key3, KeyCode::E,
            KeyCode::Key4, KeyCode::R, KeyCode::T, KeyCode::Key5, KeyCode::Y,
            KeyCode::Key6, KeyCode::U, KeyCode::I, KeyCode::Key8, KeyCode::O,
            KeyCode::Key9, KeyCode::P, KeyCode::Key0, KeyCode::LeftBracket,
            KeyCode::RightBracket,
        ];

        for key in note_keys {
            if is_key_pressed(key) {
                if let Some(pitch) = TrackerState::key_to_note(key, state.octave) {
                    // Just preview the sound, don't enter into pattern
                    state.audio.note_on(state.current_channel as i32, pitch as i32, 100);
                }
            }
            if is_key_released(key) {
                if let Some(pitch) = TrackerState::key_to_note(key, state.octave) {
                    state.audio.note_off(state.current_channel as i32, pitch as i32);
                }
            }
        }
    }
}
