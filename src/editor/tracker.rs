//! Tracker panel — egui UI for the music editor

use crate::tracker::TrackerState;
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
            self.draw_pattern_view(ui);
        });

        self.handle_keyboard(ctx);
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

            // Navigation
            if i.key_pressed(egui::Key::ArrowUp) {
                self.state.move_cursor_up();
            }
            if i.key_pressed(egui::Key::ArrowDown) {
                self.state.move_cursor_down();
            }
            if i.key_pressed(egui::Key::ArrowLeft) {
                self.state.move_cursor_left();
            }
            if i.key_pressed(egui::Key::ArrowRight) {
                self.state.move_cursor_right();
            }

            // Pattern navigation (Ctrl+Left/Right)
            if i.modifiers.command && i.key_pressed(egui::Key::ArrowLeft) {
                self.state.prev_pattern();
            }
            if i.modifiers.command && i.key_pressed(egui::Key::ArrowRight) {
                self.state.next_pattern();
            }

            // Octave (Ctrl+Up/Down)
            if i.modifiers.command && i.key_pressed(egui::Key::ArrowUp) {
                if self.state.octave < 9 {
                    self.state.octave += 1;
                }
            }
            if i.modifiers.command && i.key_pressed(egui::Key::ArrowDown) {
                if self.state.octave > 0 {
                    self.state.octave -= 1;
                }
            }

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
            // Rewind
            if icon_button(ui, icon::SKIP_BACK, theme::ICON_SIZE_MD, "Rewind to start") {
                self.state.playback_row = 0;
                self.state.current_row = 0;
            }

            // Play / Stop
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

            // Pattern info
            ui.label(format!(
                "Pat {}/{} Row {}/{}",
                self.state.current_pattern_idx + 1,
                self.state.song.arrangement.len(),
                self.state.current_row + 1,
                self.state.pattern_length(),
            ));

            // Song name
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
