//! Tracker UI layout and rendering

use macroquad::prelude::*;
use crate::ui::{Rect, UiContext, Toolbar, icon, ACCENT_COLOR, draw_knob};
use super::state::{TrackerState, TrackerView};

// Colors
const BG_COLOR: Color = Color::new(0.11, 0.11, 0.13, 1.0);
const HEADER_COLOR: Color = Color::new(0.15, 0.15, 0.18, 1.0);
const ROW_EVEN: Color = Color::new(0.13, 0.13, 0.15, 1.0);
const ROW_ODD: Color = Color::new(0.11, 0.11, 0.13, 1.0);
const ROW_BEAT: Color = Color::new(0.16, 0.14, 0.12, 1.0);
const ROW_HIGHLIGHT: Color = Color::new(0.2, 0.25, 0.3, 1.0);
const CURSOR_COLOR: Color = Color::new(0.3, 0.5, 0.8, 0.8);
const PLAYBACK_ROW_COLOR: Color = Color::new(0.4, 0.2, 0.2, 0.6);
const TEXT_COLOR: Color = Color::new(0.8, 0.8, 0.85, 1.0);
const TEXT_DIM: Color = Color::new(0.4, 0.4, 0.45, 1.0);
const NOTE_COLOR: Color = Color::new(0.9, 0.85, 0.5, 1.0);
const INST_COLOR: Color = Color::new(0.5, 0.8, 0.5, 1.0);
const VOL_COLOR: Color = Color::new(0.5, 0.7, 0.9, 1.0);
const FX_COLOR: Color = Color::new(0.9, 0.5, 0.7, 1.0);

// Layout constants
const ROW_HEIGHT: f32 = 18.0;
const CHANNEL_WIDTH: f32 = 140.0;
const ROW_NUM_WIDTH: f32 = 30.0;
const NOTE_WIDTH: f32 = 36.0;
const INST_WIDTH: f32 = 24.0;
const VOL_WIDTH: f32 = 24.0;
const FX_WIDTH: f32 = 16.0;
const FXPARAM_WIDTH: f32 = 24.0;

/// Draw the tracker interface
pub fn draw_tracker(ctx: &mut UiContext, rect: Rect, state: &mut TrackerState, icon_font: Option<&Font>) {
    // Background
    draw_rectangle(rect.x, rect.y, rect.w, rect.h, BG_COLOR);

    // Split into header and main area
    let header_height = 60.0;
    let header_rect = Rect::new(rect.x, rect.y, rect.w, header_height);
    let main_rect = Rect::new(rect.x, rect.y + header_height, rect.w, rect.h - header_height);

    // Draw header (transport, info)
    draw_header(ctx, header_rect, state, icon_font);

    // Draw main content based on view
    match state.view {
        TrackerView::Pattern => draw_pattern_view(ctx, main_rect, state),
        TrackerView::Arrangement => draw_arrangement_view(ctx, main_rect, state),
        TrackerView::Instruments => draw_instruments_view(ctx, main_rect, state),
    }

    // Handle input
    handle_input(ctx, state);
}

/// Draw the header with transport controls and song info
fn draw_header(ctx: &mut UiContext, rect: Rect, state: &mut TrackerState, icon_font: Option<&Font>) {
    draw_rectangle(rect.x, rect.y, rect.w, rect.h, HEADER_COLOR);

    // First row: toolbar with icons (36.0 height to match World Editor)
    let toolbar_rect = Rect::new(rect.x, rect.y, rect.w, 36.0);
    let mut toolbar = Toolbar::new(toolbar_rect);

    // View mode buttons
    let view_icons = [
        (TrackerView::Pattern, icon::GRID, "Pattern Editor"),
        (TrackerView::Arrangement, icon::LIST_MUSIC, "Arrangement"),
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

    // Step controls
    toolbar.label(&format!("Step:{}", state.edit_step));
    if toolbar.icon_button(ctx, icon::MINUS, icon_font, "Decrease Step") {
        state.edit_step = state.edit_step.saturating_sub(1);
    }
    if toolbar.icon_button(ctx, icon::PLUS, icon_font, "Increase Step") {
        state.edit_step = (state.edit_step + 1).min(16);
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
        rect.x + 10.0, y2 + 14.0, 12.0, TEXT_COLOR
    );

    // Soundfont status
    let sf_status = state.audio.soundfont_name()
        .map(|n| format!("SF: {}", n))
        .unwrap_or_else(|| "No Soundfont".to_string());
    draw_text(&sf_status, rect.x + 350.0, y2 + 14.0, 12.0, if state.audio.is_loaded() { TEXT_DIM } else { Color::new(0.8, 0.3, 0.3, 1.0) });

    // Status message
    if let Some(status) = state.get_status() {
        draw_text(status, rect.x + 550.0, y2 + 14.0, 12.0, Color::new(1.0, 0.8, 0.3, 1.0));
    }
}

/// Height of the channel strip header (instrument selector, etc.)
const CHANNEL_STRIP_HEIGHT: f32 = 36.0;

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

    // === Channel strip header (instrument selector) ===
    draw_rectangle(rect.x, rect.y, rect.w, CHANNEL_STRIP_HEIGHT, Color::new(0.12, 0.12, 0.14, 1.0));

    let mut x = rect.x + ROW_NUM_WIDTH;
    for ch in 0..num_channels {
        let ch_x = x;
        let is_current = ch == state.current_channel;
        let strip_rect = Rect::new(ch_x, rect.y, CHANNEL_WIDTH - 1.0, CHANNEL_STRIP_HEIGHT);

        // Background for selected channel
        if is_current {
            draw_rectangle(ch_x, rect.y, CHANNEL_WIDTH - 1.0, CHANNEL_STRIP_HEIGHT, Color::new(0.18, 0.2, 0.24, 1.0));
        }

        // Click to select channel
        if ctx.mouse.inside(&strip_rect) && is_mouse_button_pressed(MouseButton::Left) {
            state.current_channel = ch;
        }

        // Channel number
        let ch_color = if is_current { NOTE_COLOR } else { TEXT_COLOR };
        draw_text(&format!("Ch {}", ch + 1), ch_x + 4.0, rect.y + 12.0, 11.0, ch_color);

        // Instrument selector: [-] [instrument name] [+]
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
        let minus_rect = Rect::new(ch_x + 2.0, rect.y + 16.0, 16.0, 16.0);
        let minus_hover = ctx.mouse.inside(&minus_rect);
        draw_rectangle(minus_rect.x, minus_rect.y, minus_rect.w, minus_rect.h,
            if minus_hover { Color::new(0.3, 0.3, 0.35, 1.0) } else { Color::new(0.2, 0.2, 0.25, 1.0) });
        draw_text("-", minus_rect.x + 5.0, minus_rect.y + 12.0, 12.0, TEXT_COLOR);
        if minus_hover && is_mouse_button_pressed(MouseButton::Left) {
            let new_inst = inst.saturating_sub(1);
            state.song.set_channel_instrument(ch, new_inst);
            if ch == state.current_channel {
                state.audio.set_program(ch as i32, new_inst as i32);
            }
        }

        // Instrument name (clickable to open instrument picker)
        let name_x = ch_x + 20.0;
        draw_text(&format!("{:03}:{}", inst, display_name), name_x, rect.y + 28.0, 10.0, INST_COLOR);

        // [+] button
        let plus_rect = Rect::new(ch_x + CHANNEL_WIDTH - 20.0, rect.y + 16.0, 16.0, 16.0);
        let plus_hover = ctx.mouse.inside(&plus_rect);
        draw_rectangle(plus_rect.x, plus_rect.y, plus_rect.w, plus_rect.h,
            if plus_hover { Color::new(0.3, 0.3, 0.35, 1.0) } else { Color::new(0.2, 0.2, 0.25, 1.0) });
        draw_text("+", plus_rect.x + 4.0, plus_rect.y + 12.0, 12.0, TEXT_COLOR);
        if plus_hover && is_mouse_button_pressed(MouseButton::Left) {
            let new_inst = (inst + 1).min(127);
            state.song.set_channel_instrument(ch, new_inst);
            if ch == state.current_channel {
                state.audio.set_program(ch as i32, new_inst as i32);
            }
        }

        x += CHANNEL_WIDTH;

        // Channel separator
        draw_line(x - 1.0, rect.y, x - 1.0, rect.y + rect.h, 1.0, Color::new(0.25, 0.25, 0.3, 1.0));
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

        // Column labels
        let is_current = ch == state.current_channel;
        let label_color = if is_current { NOTE_COLOR } else { TEXT_DIM };
        draw_text("Not", ch_x + 4.0, header_y + 13.0, 10.0, label_color);
        draw_text("In", ch_x + NOTE_WIDTH + 2.0, header_y + 13.0, 10.0, label_color);
        draw_text("Vl", ch_x + NOTE_WIDTH + INST_WIDTH + 2.0, header_y + 13.0, 10.0, label_color);
        draw_text("Fx", ch_x + NOTE_WIDTH + INST_WIDTH + VOL_WIDTH + 2.0, header_y + 13.0, 10.0, label_color);

        x += CHANNEL_WIDTH;
    }

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

        // Calculate clicked row
        let clicked_screen_row = ((mouse_y - grid_y_start) / ROW_HEIGHT) as usize;
        let clicked_row = state.scroll_row + clicked_screen_row;

        if clicked_row < pattern_length {
            state.current_row = clicked_row;

            // Calculate clicked channel and column
            let rel_x = mouse_x - rect.x - ROW_NUM_WIDTH;
            if rel_x >= 0.0 {
                let clicked_channel = (rel_x / CHANNEL_WIDTH) as usize;
                if clicked_channel < num_channels {
                    state.current_channel = clicked_channel;

                    // Calculate column within channel
                    let col_x = rel_x - (clicked_channel as f32 * CHANNEL_WIDTH);
                    state.current_column = if col_x < NOTE_WIDTH {
                        0 // Note
                    } else if col_x < NOTE_WIDTH + INST_WIDTH {
                        1 // Instrument
                    } else if col_x < NOTE_WIDTH + INST_WIDTH + VOL_WIDTH {
                        2 // Volume
                    } else if col_x < NOTE_WIDTH + INST_WIDTH + VOL_WIDTH + FX_WIDTH {
                        3 // Effect
                    } else {
                        4 // Effect param
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

            // Cursor highlight
            if row_idx == state.current_row && ch == state.current_channel {
                let col_x = x + match state.current_column {
                    0 => 0.0,
                    1 => NOTE_WIDTH,
                    2 => NOTE_WIDTH + INST_WIDTH,
                    3 => NOTE_WIDTH + INST_WIDTH + VOL_WIDTH,
                    _ => NOTE_WIDTH + INST_WIDTH + VOL_WIDTH + FX_WIDTH,
                };
                let col_w = match state.current_column {
                    0 => NOTE_WIDTH,
                    1 => INST_WIDTH,
                    2 => VOL_WIDTH,
                    3 => FX_WIDTH,
                    _ => FXPARAM_WIDTH,
                };
                draw_rectangle(col_x, y, col_w, ROW_HEIGHT, CURSOR_COLOR);
            }

            // Note
            let note_str = note.pitch_name().unwrap_or_else(|| "---".to_string());
            let note_color = if note.pitch.is_some() { NOTE_COLOR } else { TEXT_DIM };
            draw_text(&note_str, x + 2.0, y + 14.0, 12.0, note_color);

            // Instrument
            let inst_str = note.instrument.map(|i| format!("{:02X}", i)).unwrap_or_else(|| "--".to_string());
            let inst_color = if note.instrument.is_some() { INST_COLOR } else { TEXT_DIM };
            draw_text(&inst_str, x + NOTE_WIDTH + 2.0, y + 14.0, 12.0, inst_color);

            // Volume
            let vol_str = note.volume.map(|v| format!("{:02X}", v)).unwrap_or_else(|| "--".to_string());
            let vol_color = if note.volume.is_some() { VOL_COLOR } else { TEXT_DIM };
            draw_text(&vol_str, x + NOTE_WIDTH + INST_WIDTH + 2.0, y + 14.0, 12.0, vol_color);

            // Effect
            let fx_str = note.effect.map(|e| e.to_string()).unwrap_or_else(|| "-".to_string());
            let fx_color = if note.effect.is_some() { FX_COLOR } else { TEXT_DIM };
            draw_text(&fx_str, x + NOTE_WIDTH + INST_WIDTH + VOL_WIDTH + 2.0, y + 14.0, 12.0, fx_color);

            // Effect param
            let fxp_str = note.effect_param.map(|p| format!("{:02X}", p)).unwrap_or_else(|| "--".to_string());
            draw_text(&fxp_str, x + NOTE_WIDTH + INST_WIDTH + VOL_WIDTH + FX_WIDTH + 2.0, y + 14.0, 12.0, fx_color);

            x += CHANNEL_WIDTH;
        }
    }
}

/// Draw the arrangement view (placeholder)
fn draw_arrangement_view(_ctx: &mut UiContext, rect: Rect, state: &TrackerState) {
    draw_rectangle(rect.x, rect.y, rect.w, rect.h, BG_COLOR);

    // Header
    draw_text("Song Arrangement", rect.x + 10.0, rect.y + 24.0, 16.0, TEXT_COLOR);

    // Draw arrangement as list
    let mut y = rect.y + 50.0;
    for (i, &pattern_idx) in state.song.arrangement.iter().enumerate() {
        let is_current = i == state.current_pattern_idx;
        let bg = if is_current { ROW_HIGHLIGHT } else if i % 2 == 0 { ROW_EVEN } else { ROW_ODD };
        draw_rectangle(rect.x + 10.0, y, 200.0, 24.0, bg);
        draw_text(
            &format!("{:02}: Pattern {:02}", i, pattern_idx),
            rect.x + 20.0, y + 16.0, 14.0,
            if is_current { NOTE_COLOR } else { TEXT_COLOR }
        );
        y += 26.0;
    }

    draw_text("(Press + to add pattern, - to remove)", rect.x + 10.0, rect.y + rect.h - 30.0, 12.0, TEXT_DIM);
}

/// Piano key layout for drawing
const PIANO_WHITE_KEYS: [(u8, &str); 7] = [
    (0, "C"), (2, "D"), (4, "E"), (5, "F"), (7, "G"), (9, "A"), (11, "B")
];
const PIANO_BLACK_KEYS: [(u8, &str, f32); 5] = [
    (1, "C#", 0.7), (3, "D#", 1.7), (6, "F#", 3.7), (8, "G#", 4.7), (10, "A#", 5.7)
];

/// Keyboard mapping for piano: maps key offset (0-23) to keyboard key name
fn get_key_label(offset: u8) -> Option<&'static str> {
    match offset {
        0 => Some("Z"), 1 => Some("S"), 2 => Some("X"), 3 => Some("D"), 4 => Some("C"),
        5 => Some("V"), 6 => Some("G"), 7 => Some("B"), 8 => Some("H"), 9 => Some("N"),
        10 => Some("J"), 11 => Some("M"),
        12 => Some("Q"), 13 => Some("2"), 14 => Some("W"), 15 => Some("3"), 16 => Some("E"),
        17 => Some("R"), 18 => Some("5"), 19 => Some("T"), 20 => Some("6"), 21 => Some("Y"),
        22 => Some("7"), 23 => Some("U"),
        _ => None,
    }
}

/// Check if the keyboard key for a given note offset is currently pressed
fn is_note_key_down(offset: u8) -> bool {
    let key = match offset {
        0 => KeyCode::Z, 1 => KeyCode::S, 2 => KeyCode::X, 3 => KeyCode::D, 4 => KeyCode::C,
        5 => KeyCode::V, 6 => KeyCode::G, 7 => KeyCode::B, 8 => KeyCode::H, 9 => KeyCode::N,
        10 => KeyCode::J, 11 => KeyCode::M,
        12 => KeyCode::Q, 13 => KeyCode::Key2, 14 => KeyCode::W, 15 => KeyCode::Key3, 16 => KeyCode::E,
        17 => KeyCode::R, 18 => KeyCode::Key5, 19 => KeyCode::T, 20 => KeyCode::Key6, 21 => KeyCode::Y,
        22 => KeyCode::Key7, 23 => KeyCode::U,
        _ => return false,
    };
    is_key_down(key)
}

/// Draw the instruments view with piano keyboard
fn draw_instruments_view(ctx: &mut UiContext, rect: Rect, state: &mut TrackerState) {
    draw_rectangle(rect.x, rect.y, rect.w, rect.h, BG_COLOR);

    // Split into left (instrument list) and right (piano + info)
    let list_width = 280.0;
    let list_rect = Rect::new(rect.x, rect.y, list_width, rect.h);

    // === LEFT: Instrument List ===
    draw_rectangle(list_rect.x, list_rect.y, list_rect.w, list_rect.h, Color::new(0.09, 0.09, 0.11, 1.0));
    draw_text("Instruments (GM)", list_rect.x + 10.0, list_rect.y + 20.0, 14.0, TEXT_COLOR);

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
    let piano_x = rect.x + list_width + 20.0;
    let piano_y = rect.y + 30.0;
    let white_key_w = 36.0;
    let white_key_h = 120.0;
    let black_key_w = 24.0;
    let black_key_h = 75.0;

    draw_text(&format!("Piano - Octave {} & {}", state.octave, state.octave + 1), piano_x, piano_y - 10.0, 14.0, TEXT_COLOR);

    // Draw two octaves of keys
    for octave_offset in 0..2 {
        let octave_x = piano_x + octave_offset as f32 * (7.0 * white_key_w);

        // White keys first (so black keys draw on top)
        for (i, (semitone, note_name)) in PIANO_WHITE_KEYS.iter().enumerate() {
            let key_x = octave_x + i as f32 * white_key_w;
            let key_rect = Rect::new(key_x, piano_y, white_key_w - 2.0, white_key_h);

            let note_offset = octave_offset * 12 + *semitone;
            let midi_note = state.octave * 12 + note_offset;
            let is_hovered = ctx.mouse.inside(&key_rect);
            let is_key_pressed = is_note_key_down(note_offset);

            // Background - cyan highlight when key pressed, gray when hovered
            let bg = if is_key_pressed {
                Color::new(0.0, 0.75, 0.9, 1.0) // Cyan highlight
            } else if is_hovered {
                Color::new(0.85, 0.85, 0.9, 1.0)
            } else {
                Color::new(0.95, 0.95, 0.95, 1.0)
            };
            draw_rectangle(key_x, piano_y, white_key_w - 2.0, white_key_h, bg);
            draw_rectangle(key_x, piano_y, white_key_w - 2.0, white_key_h, Color::new(0.3, 0.3, 0.3, 1.0));
            draw_rectangle(key_x + 1.0, piano_y + 1.0, white_key_w - 4.0, white_key_h - 2.0, bg);

            // Click to play
            if is_hovered && is_mouse_button_pressed(MouseButton::Left) {
                state.audio.note_on(state.current_channel as i32, midi_note as i32, 100);
            }
            if is_hovered && is_mouse_button_released(MouseButton::Left) {
                state.audio.note_off(state.current_channel as i32, midi_note as i32);
            }

            // Note name at bottom
            let text_color = if is_key_pressed { WHITE } else { Color::new(0.3, 0.3, 0.3, 1.0) };
            draw_text(note_name, key_x + 12.0, piano_y + white_key_h - 25.0, 14.0, text_color);

            // Keyboard shortcut label
            if let Some(key_label) = get_key_label(note_offset) {
                let label_color = if is_key_pressed { WHITE } else { Color::new(0.5, 0.5, 0.5, 1.0) };
                draw_text(key_label, key_x + 13.0, piano_y + white_key_h - 8.0, 12.0, label_color);
            }
        }

        // Black keys on top
        for (semitone, _note_name, x_pos) in PIANO_BLACK_KEYS.iter() {
            let key_x = octave_x + *x_pos * white_key_w;
            let key_rect = Rect::new(key_x, piano_y, black_key_w, black_key_h);

            let note_offset = octave_offset * 12 + *semitone;
            let midi_note = state.octave * 12 + note_offset;
            let is_hovered = ctx.mouse.inside(&key_rect);
            let is_key_pressed = is_note_key_down(note_offset);

            // Background - cyan highlight when key pressed
            let bg = if is_key_pressed {
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

            // Keyboard shortcut label
            if let Some(key_label) = get_key_label(note_offset) {
                let label_color = if is_key_pressed { WHITE } else { Color::new(0.6, 0.6, 0.6, 1.0) };
                draw_text(key_label, key_x + 7.0, piano_y + black_key_h - 8.0, 10.0, label_color);
            }
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

    // === EFFECT KNOBS ===
    let effects_y = info_y + 30.0;
    let ch = state.current_channel;

    draw_text("Effects Preview", piano_x, effects_y, 14.0, TEXT_COLOR);

    let knob_radius = 28.0;
    let knob_spacing = 70.0;
    let knob_y = effects_y + 50.0;

    // Knob definitions: (index, label, value, is_bipolar)
    let knob_data = [
        (0, "Pan", state.preview_pan[ch], true),
        (1, "Reverb", state.preview_reverb[ch], false),
        (2, "Chorus", state.preview_chorus[ch], false),
        (3, "Mod", state.preview_modulation[ch], false),
        (4, "Expr", state.preview_expression[ch], false),
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
                    1 => state.set_preview_reverb(clamped),
                    2 => state.set_preview_chorus(clamped),
                    3 => state.set_preview_modulation(clamped),
                    4 => state.set_preview_expression(clamped),
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
                1 => state.set_preview_reverb(new_val),
                2 => state.set_preview_chorus(new_val),
                3 => state.set_preview_modulation(new_val),
                4 => state.set_preview_expression(new_val),
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
    draw_text("Click keys to preview | Use keyboard (Z-M, Q-U) to enter notes",
              piano_x, help_y, 12.0, TEXT_DIM);
    draw_text("[ ] = prev/next instrument | +/- = octave up/down",
              piano_x, help_y + 17.0, 12.0, TEXT_DIM);
    draw_text("Drag knobs to adjust | Click value to type",
              piano_x, help_y + 34.0, 12.0, TEXT_DIM);
}

/// Handle keyboard and mouse input
fn handle_input(_ctx: &mut UiContext, state: &mut TrackerState) {
    // Navigation
    if is_key_pressed(KeyCode::Up) {
        state.cursor_up();
    }
    if is_key_pressed(KeyCode::Down) {
        state.cursor_down();
    }
    if is_key_pressed(KeyCode::Left) {
        state.cursor_left();
    }
    if is_key_pressed(KeyCode::Right) {
        state.cursor_right();
    }
    if is_key_pressed(KeyCode::Tab) {
        if is_key_down(KeyCode::LeftShift) || is_key_down(KeyCode::RightShift) {
            state.prev_channel();
        } else {
            state.next_channel();
        }
    }

    // Page up/down
    if is_key_pressed(KeyCode::PageUp) {
        for _ in 0..16 {
            state.cursor_up();
        }
    }
    if is_key_pressed(KeyCode::PageDown) {
        for _ in 0..16 {
            state.cursor_down();
        }
    }

    // Home/End
    if is_key_pressed(KeyCode::Home) {
        state.current_row = 0;
        state.scroll_row = 0;
    }
    if is_key_pressed(KeyCode::End) {
        if let Some(pattern) = state.current_pattern() {
            state.current_row = pattern.length - 1;
        }
    }

    // Playback
    if is_key_pressed(KeyCode::Space) {
        state.toggle_playback();
    }
    if is_key_pressed(KeyCode::Escape) {
        state.stop_playback();
    }

    // Octave
    if is_key_pressed(KeyCode::KpAdd) || (is_key_down(KeyCode::LeftShift) && is_key_pressed(KeyCode::Equal)) {
        state.octave = (state.octave + 1).min(9);
        state.set_status(&format!("Octave: {}", state.octave), 1.0);
    }
    if is_key_pressed(KeyCode::KpSubtract) || is_key_pressed(KeyCode::Minus) {
        state.octave = state.octave.saturating_sub(1);
        state.set_status(&format!("Octave: {}", state.octave), 1.0);
    }

    // Instrument selection (for current channel)
    if is_key_pressed(KeyCode::LeftBracket) {
        let new_inst = state.current_instrument().saturating_sub(1);
        state.set_current_instrument(new_inst);
        state.set_status(&format!("Instrument: {:02}", new_inst), 1.0);
    }
    if is_key_pressed(KeyCode::RightBracket) {
        let new_inst = (state.current_instrument() + 1).min(127);
        state.set_current_instrument(new_inst);
        state.set_status(&format!("Instrument: {:02}", new_inst), 1.0);
    }

    // Edit step
    if is_key_pressed(KeyCode::F9) {
        state.edit_step = state.edit_step.saturating_sub(1);
        state.set_status(&format!("Edit step: {}", state.edit_step), 1.0);
    }
    if is_key_pressed(KeyCode::F10) {
        state.edit_step = (state.edit_step + 1).min(16);
        state.set_status(&format!("Edit step: {}", state.edit_step), 1.0);
    }

    // Delete
    if is_key_pressed(KeyCode::Delete) || is_key_pressed(KeyCode::Backspace) {
        state.delete_note();
    }

    // Note entry (only in Pattern view, when in edit mode and in note column)
    if state.view == TrackerView::Pattern && state.edit_mode && state.current_column == 0 {
        // Check for note keys
        let note_keys = [
            KeyCode::Z, KeyCode::S, KeyCode::X, KeyCode::D, KeyCode::C,
            KeyCode::V, KeyCode::G, KeyCode::B, KeyCode::H, KeyCode::N,
            KeyCode::J, KeyCode::M,
            KeyCode::Q, KeyCode::Key2, KeyCode::W, KeyCode::Key3, KeyCode::E,
            KeyCode::R, KeyCode::Key5, KeyCode::T, KeyCode::Key6, KeyCode::Y,
            KeyCode::Key7, KeyCode::U,
        ];

        for key in note_keys {
            if is_key_pressed(key) {
                if let Some(pitch) = TrackerState::key_to_note(key, state.octave) {
                    state.enter_note(pitch);
                }
            }
        }

        // Note off with period or backtick
        if is_key_pressed(KeyCode::Period) || is_key_pressed(KeyCode::Apostrophe) {
            state.enter_note_off();
        }
    }

    // Effect entry (in Pattern view, edit mode, effect column = 3)
    if state.view == TrackerView::Pattern && state.edit_mode && state.current_column == 3 {
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

    // Effect parameter entry (in Pattern view, edit mode, fx_param column = 4)
    if state.view == TrackerView::Pattern && state.edit_mode && state.current_column == 4 {
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

    // In Instruments view, allow keyboard to preview sounds without entering notes
    if state.view == TrackerView::Instruments {
        let note_keys = [
            KeyCode::Z, KeyCode::S, KeyCode::X, KeyCode::D, KeyCode::C,
            KeyCode::V, KeyCode::G, KeyCode::B, KeyCode::H, KeyCode::N,
            KeyCode::J, KeyCode::M,
            KeyCode::Q, KeyCode::Key2, KeyCode::W, KeyCode::Key3, KeyCode::E,
            KeyCode::R, KeyCode::Key5, KeyCode::T, KeyCode::Key6, KeyCode::Y,
            KeyCode::Key7, KeyCode::U,
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
