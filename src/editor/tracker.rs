//! Tracker panel — egui UI for the music editor

use crate::tracker::TrackerState;
use crate::tracker::TrackerView;
use crate::tracker::pattern::Note;
use super::icons::{icon, icon_button, icon_toggle};
use super::theme;

pub struct TrackerPanel {
    pub state: TrackerState,
}

impl TrackerPanel {
    pub fn new() -> Self {
        Self {
            state: TrackerState::new(),
        }
    }

    pub fn draw(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("tracker_transport").show(ctx, |ui| {
            self.draw_transport(ui);
        });

        egui::SidePanel::right("tracker_instruments")
            .default_width(200.0)
            .show(ctx, |ui| {
                self.draw_instrument_panel(ui);
            });

        egui::CentralPanel::default().show(ctx, |ui| {
            match self.state.view {
                TrackerView::Pattern     => self.draw_pattern_view(ui),
                TrackerView::Arrangement => self.draw_arrangement_view(ui),
            }
        });

        self.handle_keyboard(ctx);
    }

    pub fn draw_inside(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        egui::TopBottomPanel::top("tracker_transport")
            .show_inside(ui, |ui| {
                self.draw_transport(ui);
            });

        egui::SidePanel::right("tracker_instruments")
            .default_width(200.0)
            .show_inside(ui, |ui| {
                self.draw_instrument_panel(ui);
            });

        egui::CentralPanel::default().show_inside(ui, |ui| {
            match self.state.view {
                TrackerView::Pattern     => self.draw_pattern_view(ui),
                TrackerView::Arrangement => self.draw_arrangement_view(ui),
            }
        });

        self.handle_keyboard(&ctx);
    }

    fn handle_keyboard(&mut self, ctx: &egui::Context) {
        // Don't process keys if a text field has focus
        if ctx.wants_keyboard_input() {
            return;
        }

        ctx.input(|i| {
            // Space = play/stop
            if i.key_pressed(egui::Key::Space) {
                self.state.toggle_play();
            }

            // Ctrl combos (highest priority, checked before bare navigation)
            if i.modifiers.command {
                if i.key_pressed(egui::Key::A) {
                    self.state.select_all();
                    return;
                }
                if i.key_pressed(egui::Key::C) {
                    self.state.copy_selection();
                    return;
                }
                if i.key_pressed(egui::Key::X) {
                    self.state.cut_selection();
                    return;
                }
                if i.key_pressed(egui::Key::V) {
                    self.state.paste_clipboard();
                    return;
                }
                // Pattern navigation (Ctrl+Left/Right)
                if i.key_pressed(egui::Key::ArrowLeft) {
                    self.state.prev_pattern();
                    return;
                }
                if i.key_pressed(egui::Key::ArrowRight) {
                    self.state.next_pattern();
                    return;
                }
                // Octave (Ctrl+Up/Down)
                if i.key_pressed(egui::Key::ArrowUp) {
                    if self.state.octave < 9 { self.state.octave += 1; }
                    return;
                }
                if i.key_pressed(egui::Key::ArrowDown) {
                    if self.state.octave > 0 { self.state.octave -= 1; }
                    return;
                }
            }

            // Bare navigation
            if i.key_pressed(egui::Key::ArrowUp)   { self.state.move_cursor_up(); }
            if i.key_pressed(egui::Key::ArrowDown)  { self.state.move_cursor_down(); }
            if i.key_pressed(egui::Key::ArrowLeft)  { self.state.move_cursor_left(); }
            if i.key_pressed(egui::Key::ArrowRight) { self.state.move_cursor_right(); }
            if i.key_pressed(egui::Key::PageUp)     { self.state.move_cursor_page_up(); }
            if i.key_pressed(egui::Key::PageDown)   { self.state.move_cursor_page_down(); }
            if i.key_pressed(egui::Key::Home)       { self.state.move_cursor_home(); }
            if i.key_pressed(egui::Key::End)        { self.state.move_cursor_end(); }

            // Delete = clear note
            if i.key_pressed(egui::Key::Delete) || i.key_pressed(egui::Key::Backspace) {
                self.state.delete_note();
            }

            // Grave/backtick = note off
            if i.key_pressed(egui::Key::Backtick) {
                self.state.enter_note_off();
            }

            // Note input keys (only when not pressing modifiers)
            if !i.modifiers.command && !i.modifiers.alt {
                let note_keys = [
                    egui::Key::Z, egui::Key::S, egui::Key::X, egui::Key::D,
                    egui::Key::C, egui::Key::V, egui::Key::G, egui::Key::B,
                    egui::Key::H, egui::Key::N, egui::Key::J, egui::Key::M,
                    egui::Key::Q, egui::Key::Num2, egui::Key::W, egui::Key::Num3,
                    egui::Key::E, egui::Key::R, egui::Key::Num5, egui::Key::T,
                    egui::Key::Num6, egui::Key::Y, egui::Key::Num7, egui::Key::U,
                ];

                for key in note_keys {
                    if i.key_pressed(key) {
                        if let Some(pitch) = self.state.key_to_pitch(key) {
                            self.state.enter_note(pitch);
                        }
                        break;
                    }
                }
            }
        });
    }

    fn draw_transport(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            // View tabs
            let pat_active = self.state.view == TrackerView::Pattern;
            let arr_active = self.state.view == TrackerView::Arrangement;

            let pat_resp = ui.add(
                egui::Button::new(
                    egui::RichText::new("Pattern")
                        .color(if pat_active { theme::ACCENT } else { theme::TEXT_DIM })
                ).frame(false).min_size(egui::vec2(0.0, 28.0))
            );
            if pat_resp.clicked() { self.state.view = TrackerView::Pattern; }
            if pat_active {
                ui.painter().line_segment(
                    [pat_resp.rect.left_bottom(), pat_resp.rect.right_bottom()],
                    egui::Stroke::new(2.0, theme::ACCENT),
                );
            }

            let arr_resp = ui.add(
                egui::Button::new(
                    egui::RichText::new("Arrangement")
                        .color(if arr_active { theme::ACCENT } else { theme::TEXT_DIM })
                ).frame(false).min_size(egui::vec2(0.0, 28.0))
            );
            if arr_resp.clicked() { self.state.view = TrackerView::Arrangement; }
            if arr_active {
                ui.painter().line_segment(
                    [arr_resp.rect.left_bottom(), arr_resp.rect.right_bottom()],
                    egui::Stroke::new(2.0, theme::ACCENT),
                );
            }

            ui.separator();

            // Transport
            if icon_button(ui, icon::SKIP_BACK, theme::ICON_SIZE_MD, "Rewind to start") {
                self.state.playback_row = 0;
                self.state.current_row = 0;
            }
            let (play_icon, play_tip) = if self.state.playing {
                (icon::SQUARE, "Stop")
            } else {
                (icon::PLAY, "Play")
            };
            if icon_button(ui, play_icon, theme::ICON_SIZE_MD, play_tip) {
                self.state.toggle_play();
            }

            ui.separator();

            // BPM
            ui.label("BPM");
            let mut bpm = self.state.song.bpm as f32;
            if ui.add(egui::DragValue::new(&mut bpm).range(40.0..=300.0).speed(0.5)).changed() {
                self.state.song.bpm = bpm as u16;
            }
            if ui.small_button("Tap").clicked() {
                if let Some(bpm) = self.state.tap_tempo() {
                    self.state.song.bpm = bpm;
                }
            }

            ui.separator();

            // Octave
            ui.label("Oct");
            if icon_button(ui, icon::MINUS, theme::ICON_SIZE_SM, "Octave down") && self.state.octave > 0 {
                self.state.octave -= 1;
            }
            ui.label(format!("{}", self.state.octave));
            if icon_button(ui, icon::PLUS, theme::ICON_SIZE_SM, "Octave up") && self.state.octave < 9 {
                self.state.octave += 1;
            }

            ui.separator();

            // Edit mode
            if icon_toggle(ui, icon::PENCIL, theme::ICON_SIZE_MD, self.state.edit_mode, "Edit mode (record notes)") {
                self.state.edit_mode = !self.state.edit_mode;
            }

            ui.separator();

            // Pattern actions (only in pattern view)
            if self.state.view == TrackerView::Pattern {
                if icon_button(ui, icon::PLUS, theme::ICON_SIZE_SM, "New pattern") {
                    self.state.new_pattern();
                }
                if icon_button(ui, icon::LAYERS, theme::ICON_SIZE_SM, "Duplicate pattern") {
                    self.state.duplicate_pattern();
                }
                if icon_button(ui, icon::TRASH, theme::ICON_SIZE_SM, "Clear pattern") {
                    self.state.clear_pattern();
                }
                ui.separator();
            }

            // Pattern info
            ui.label(format!(
                "Pat {}/{} Row {}/{}",
                self.state.current_pattern_idx + 1,
                self.state.song.patterns.len(),
                self.state.current_row + 1,
                self.state.pattern_length(),
            ));

            // Song name (right-aligned)
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(&self.state.song.name);
            });
        });
    }

    fn draw_instrument_panel(&mut self, ui: &mut egui::Ui) {
        ui.heading("Instruments");
        ui.separator();

        let current_ch = self.state.current_channel;
        let current_inst = self.state.current_instrument();

        ui.label(format!("Channel {} — {}", current_ch + 1, gm_name(current_inst)));

        ui.separator();

        // Instrument list (scrollable)
        let preset_names = self.state.audio.get_preset_names();
        egui::ScrollArea::vertical().max_height(ui.available_height() - 120.0).show(ui, |ui| {
            for (_, program, name) in &preset_names {
                let selected = *program == current_inst;
                if ui.selectable_label(selected, format!("{:3}: {}", program, name)).clicked() {
                    self.state.set_current_instrument(*program);
                }
            }
        });

        ui.separator();

        // Channel settings
        ui.label("Channel Settings");
        let settings = self.state.song.get_channel_settings(current_ch);

        ui.horizontal(|ui| {
            ui.label("Pan:");
            let mut pan = settings.pan as f32;
            if ui.add(egui::DragValue::new(&mut pan).range(0.0..=127.0).speed(1.0)).changed() {
                if let Some(s) = self.state.song.get_channel_settings_mut(current_ch) {
                    s.pan = pan as u8;
                }
                self.state.set_preview_pan(pan as u8);
            }
        });

        ui.horizontal(|ui| {
            ui.label("Reverb:");
            let mut rev = settings.effect_amount as f32;
            if ui.add(egui::DragValue::new(&mut rev).range(0.0..=127.0).speed(1.0)).changed() {
                if let Some(s) = self.state.song.get_channel_settings_mut(current_ch) {
                    s.effect_amount = rev as u8;
                }
                self.state.audio.set_reverb_send(current_ch as i32, rev as i32);
            }
        });
    }

    fn draw_pattern_view(&mut self, ui: &mut egui::Ui) {
        let pattern = match self.state.current_pattern() {
            Some(p) => p.clone(),
            None => return,
        };

        let num_channels = pattern.num_channels();
        let row_height = 16.0;
        let note_col_width = 100.0;

        // Header
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Row").monospace().strong());
            for ch in 0..num_channels {
                let selected = ch == self.state.current_channel;
                let label = format!("Ch{}", ch + 1);
                let text = if selected {
                    egui::RichText::new(label).monospace().strong().color(egui::Color32::YELLOW)
                } else {
                    egui::RichText::new(label).monospace()
                };
                if ui.add_sized([note_col_width, row_height], egui::Label::new(text).sense(egui::Sense::click())).clicked() {
                    self.state.current_channel = ch;
                }
            }
        });

        ui.separator();

        // Pattern rows (scrollable)
        egui::ScrollArea::vertical().show(ui, |ui| {
            for row in 0..pattern.length {
                let is_current = row == self.state.current_row;
                let is_playback = self.state.playing && row == self.state.playback_row
                    && self.state.playback_pattern_idx == self.state.current_pattern_idx;

                let bg_color = if is_playback {
                    egui::Color32::from_rgb(60, 60, 20)
                } else if is_current {
                    egui::Color32::from_rgb(40, 40, 60)
                } else if row % (self.state.song.rows_per_beat as usize) == 0 {
                    egui::Color32::from_rgb(30, 30, 35)
                } else {
                    egui::Color32::TRANSPARENT
                };

                ui.horizontal(|ui| {
                    if bg_color != egui::Color32::TRANSPARENT {
                        let rect = ui.available_rect_before_wrap();
                        ui.painter().rect_filled(rect, 0.0, bg_color);
                    }

                    // Row number
                    let row_text = format!("{:3}", row);
                    ui.label(egui::RichText::new(row_text).monospace().color(egui::Color32::GRAY));

                    // Notes per channel
                    for ch in 0..num_channels {
                        let note = pattern.get(ch, row).copied().unwrap_or(Note::EMPTY);
                        let text = format_note(&note);

                        let text_color = if note.is_empty() {
                            egui::Color32::from_rgb(60, 60, 60)
                        } else if note.is_off() {
                            egui::Color32::from_rgb(200, 80, 80)
                        } else {
                            egui::Color32::from_rgb(180, 200, 220)
                        };

                        let label = egui::RichText::new(text).monospace().color(text_color);
                        if ui.add_sized([note_col_width, row_height], egui::Label::new(label).sense(egui::Sense::click())).clicked() {
                            self.state.current_row = row;
                            self.state.current_channel = ch;
                        }
                    }
                });
            }
        });
    }

    fn draw_arrangement_view(&mut self, ui: &mut egui::Ui) {
        let row_h   = 28.0;
        let pat_w   = 80.0;
        let num_pat = self.state.song.patterns.len();

        ui.horizontal(|ui| {
            // ── Left column: pattern list ─────────────────────────────────
            ui.vertical(|ui| {
                ui.label(egui::RichText::new("Patterns").strong());
                ui.separator();

                egui::ScrollArea::vertical()
                    .id_salt("arr_pat_list")
                    .max_height(ui.available_height() - 8.0)
                    .show(ui, |ui| {
                        for pat_idx in 0..num_pat {
                            let selected = pat_idx == self.state.current_pattern_idx;
                            let label = format!("{:02}", pat_idx + 1);
                            let resp = ui.add_sized(
                                [56.0, row_h],
                                egui::SelectableLabel::new(selected, label),
                            );
                            if resp.clicked() {
                                self.state.current_pattern_idx = pat_idx;
                                self.state.current_row = 0;
                            }
                        }
                    });
            });

            ui.separator();

            // ── Right column: arrangement sequence ───────────────────────
            ui.vertical(|ui| {
                ui.label(egui::RichText::new("Arrangement").strong());
                ui.separator();

                let arr_len = self.state.song.arrangement.len();

                egui::ScrollArea::vertical()
                    .id_salt("arr_sequence")
                    .max_height(ui.available_height() - 8.0)
                    .show(ui, |ui| {
                        let mut remove_slot: Option<usize> = None;
                        let mut move_up_slot: Option<usize> = None;
                        let mut move_dn_slot: Option<usize> = None;

                        for slot in 0..arr_len {
                            let pat_idx = self.state.song.arrangement[slot];
                            let is_playing = self.state.playing
                                && slot == self.state.playback_pattern_idx;

                            ui.horizontal(|ui| {
                                // Playing indicator
                                if is_playing {
                                    ui.colored_label(theme::ACCENT, icon::PLAY.to_string());
                                } else {
                                    ui.label(format!("{:02}", slot + 1));
                                }

                                // Pattern slot selector
                                let label = format!("Pat {:02}", pat_idx + 1);
                                let resp = ui.add_sized(
                                    [pat_w, row_h - 4.0],
                                    egui::SelectableLabel::new(
                                        !self.state.playing && slot == self.state.playback_pattern_idx,
                                        label,
                                    ),
                                );
                                if resp.clicked() {
                                    self.state.current_pattern_idx = pat_idx;
                                    self.state.view = TrackerView::Pattern;
                                }

                                // Drag-value to change which pattern this slot uses
                                let mut idx_val = pat_idx as f32;
                                if ui.add(
                                    egui::DragValue::new(&mut idx_val)
                                        .range(0.0..=(num_pat as f32 - 1.0))
                                        .speed(0.1)
                                        .prefix("=")
                                ).changed() {
                                    self.state.song.arrangement[slot] = idx_val as usize;
                                    self.state.dirty = true;
                                }

                                // Reorder / remove
                                if icon_button(ui, icon::CHEVRON_UP, theme::ICON_SIZE_SM, "Move up") && slot > 0 {
                                    move_up_slot = Some(slot);
                                }
                                if icon_button(ui, icon::CHEVRON_DOWN, theme::ICON_SIZE_SM, "Move down") && slot + 1 < arr_len {
                                    move_dn_slot = Some(slot);
                                }
                                if icon_button(ui, icon::MINUS, theme::ICON_SIZE_SM, "Remove slot") && arr_len > 1 {
                                    remove_slot = Some(slot);
                                }
                            });
                        }

                        // Apply deferred mutations
                        if let Some(s) = move_up_slot {
                            self.state.song.arrangement.swap(s - 1, s);
                            self.state.dirty = true;
                        }
                        if let Some(s) = move_dn_slot {
                            self.state.song.arrangement.swap(s, s + 1);
                            self.state.dirty = true;
                        }
                        if let Some(s) = remove_slot {
                            self.state.song.arrangement.remove(s);
                            self.state.dirty = true;
                        }

                        // Add slot button
                        ui.separator();
                        if ui.button("+ Add slot").clicked() {
                            self.state.song.arrangement.push(0);
                            self.state.dirty = true;
                        }
                    });
            });
        });
    }

    /// Advance playback by dt seconds
    pub fn tick(&mut self, dt: f64) {
        self.state.tick_playback(dt);
    }
}

fn format_note(note: &Note) -> String {
    let pitch_str = match note.pitch {
        Some(0xFF) => "OFF".to_string(),
        Some(p) => {
            let names = ["C-", "C#", "D-", "D#", "E-", "F-", "F#", "G-", "G#", "A-", "A#", "B-"];
            format!("{}{}", names[(p % 12) as usize], p / 12)
        }
        None => "···".to_string(),
    };

    let inst_str = match note.instrument {
        Some(i) => format!("{:02X}", i),
        None => "··".to_string(),
    };

    let vol_str = match note.volume {
        Some(v) => format!("{:02X}", v),
        None => "··".to_string(),
    };

    let fx_str = match note.effect {
        Some(c) => format!("{}{:02X}", c, note.effect_param.unwrap_or(0)),
        None => "···".to_string(),
    };

    format!("{} {} {} {}", pitch_str, inst_str, vol_str, fx_str)
}

fn gm_name(program: u8) -> &'static str {
    const NAMES: [&str; 128] = [
        "Acoustic Grand Piano", "Bright Acoustic Piano", "Electric Grand Piano", "Honky-tonk Piano",
        "Electric Piano 1", "Electric Piano 2", "Harpsichord", "Clavi",
        "Celesta", "Glockenspiel", "Music Box", "Vibraphone",
        "Marimba", "Xylophone", "Tubular Bells", "Dulcimer",
        "Drawbar Organ", "Percussive Organ", "Rock Organ", "Church Organ",
        "Reed Organ", "Accordion", "Harmonica", "Tango Accordion",
        "Acoustic Guitar (nylon)", "Acoustic Guitar (steel)", "Electric Guitar (jazz)", "Electric Guitar (clean)",
        "Electric Guitar (muted)", "Overdriven Guitar", "Distortion Guitar", "Guitar Harmonics",
        "Acoustic Bass", "Electric Bass (finger)", "Electric Bass (pick)", "Fretless Bass",
        "Slap Bass 1", "Slap Bass 2", "Synth Bass 1", "Synth Bass 2",
        "Violin", "Viola", "Cello", "Contrabass",
        "Tremolo Strings", "Pizzicato Strings", "Orchestral Harp", "Timpani",
        "String Ensemble 1", "String Ensemble 2", "Synth Strings 1", "Synth Strings 2",
        "Choir Aahs", "Voice Oohs", "Synth Voice", "Orchestra Hit",
        "Trumpet", "Trombone", "Tuba", "Muted Trumpet",
        "French Horn", "Brass Section", "Synth Brass 1", "Synth Brass 2",
        "Soprano Sax", "Alto Sax", "Tenor Sax", "Baritone Sax",
        "Oboe", "English Horn", "Bassoon", "Clarinet",
        "Piccolo", "Flute", "Recorder", "Pan Flute",
        "Blown Bottle", "Shakuhachi", "Whistle", "Ocarina",
        "Lead 1 (square)", "Lead 2 (sawtooth)", "Lead 3 (calliope)", "Lead 4 (chiff)",
        "Lead 5 (charang)", "Lead 6 (voice)", "Lead 7 (fifths)", "Lead 8 (bass+lead)",
        "Pad 1 (new age)", "Pad 2 (warm)", "Pad 3 (polysynth)", "Pad 4 (choir)",
        "Pad 5 (bowed)", "Pad 6 (metallic)", "Pad 7 (halo)", "Pad 8 (sweep)",
        "FX 1 (rain)", "FX 2 (soundtrack)", "FX 3 (crystal)", "FX 4 (atmosphere)",
        "FX 5 (brightness)", "FX 6 (goblins)", "FX 7 (echoes)", "FX 8 (sci-fi)",
        "Sitar", "Banjo", "Shamisen", "Koto",
        "Kalimba", "Bag pipe", "Fiddle", "Shanai",
        "Tinkle Bell", "Agogo", "Steel Drums", "Woodblock",
        "Taiko Drum", "Melodic Tom", "Synth Drum", "Reverse Cymbal",
        "Guitar Fret Noise", "Breath Noise", "Seashore", "Bird Tweet",
        "Telephone Ring", "Helicopter", "Applause", "Gunshot",
    ];
    NAMES.get(program as usize).unwrap_or(&"Unknown")
}
