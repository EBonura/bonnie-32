//! About / Home tab — faithful port of v1's landing page.
//!
//! Sections, text, and link row match the v1 landing.rs exactly.
//! Logo is loaded lazily from assets/branding/logo.png on first draw.

use egui::{Color32, RichText, ScrollArea, TextureHandle, Vec2};
use crate::editor::theme;

// v1 landing page exact accent — slightly more cyan than our teal, kept for fidelity
const ACCENT: Color32  = Color32::from_rgb(0, 191, 229);
const MUTED:  Color32  = Color32::from_rgb(153, 153, 165);

pub struct AboutPanel {
    logo: Option<TextureHandle>,
    logo_loaded: bool,
}

impl AboutPanel {
    pub fn new() -> Self {
        Self { logo: None, logo_loaded: false }
    }

    pub fn draw(&mut self, egui_ctx: &egui::Context) {
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(theme::BG))
            .show(egui_ctx, |ui| {
                self.draw_content(ui);
            });
    }

    /// Inline variant — draws the content directly into `ui` (for split panes).
    pub fn draw_content(&mut self, ui: &mut egui::Ui) {
        // Lazy-load logo on first draw
        if !self.logo_loaded {
            self.logo_loaded = true;
            if let Ok(bytes) = std::fs::read("assets/branding/logo.png") {
                if let Ok(img) = image::load_from_memory(&bytes) {
                    let img = img.to_rgba8();
                    let size = [img.width() as usize, img.height() as usize];
                    let color_image = egui::ColorImage::from_rgba_unmultiplied(size, &img.into_raw());
                    self.logo = Some(ui.ctx().load_texture(
                        "about_logo",
                        color_image,
                        egui::TextureOptions::LINEAR,
                    ));
                }
            }
        }

        ScrollArea::vertical()
            .auto_shrink([false; 2])
            .show(ui, |ui| {
                        // Constrain content to a readable max width, centred
                        let avail = ui.available_width();
                        let content_w = (avail - 80.0).min(1000.0).max(300.0);

                        ui.add_space(40.0);

                        // ── LOGO / TITLE ──────────────────────────────────
                        ui.vertical_centered(|ui| {
                            if let Some(logo) = &self.logo {
                                let logo_w = content_w.min(500.0);
                                let size   = logo.size_vec2();
                                let logo_h = logo_w * size.y / size.x;
                                ui.image((logo.id(), Vec2::new(logo_w, logo_h)));
                                ui.add_space(8.0);
                            } else {
                                ui.label(
                                    RichText::new("BONNIE-32")
                                        .color(ACCENT)
                                        .size(32.0)
                                        .strong(),
                                );
                                ui.add_space(4.0);
                            }

                            ui.label(
                                RichText::new("A Fantasy Console for PS1-Era 3D Games")
                                    .color(MUTED)
                                    .size(theme::FONT_SIZE_UI + 2.0),
                            );
                        });

                        ui.add_space(40.0);

                        // Center a content column of content_w width
                        ui.with_layout(egui::Layout::top_down(egui::Align::Center), |ui| {
                            ui.set_max_width(content_w);
                            // ── SECTIONS ─────────────────────────────────
                            draw_section(ui, "What is BONNIE-32?",
                                "A complete toolkit for making low-poly 3D games targeting the PS1 aesthetic. \
                                Model, texture, compose music, and build levels in one place.\n\n\
                                Each tool is focused and lightweight, designed around the constraints and \
                                limitations of early 3D. The software rasterizer natively produces typical PS1 \
                                quirks: affine texture mapping, vertex snapping, limited color depth, and no \
                                sub-pixel precision. Each effect can be toggled on or off.\n\n\
                                Built in Rust, runs on Windows, Mac, Linux, and browser.");

                            draw_section(ui, "Why build this?",
                                "After making two games in PICO-8, I knew I wanted to jump to 3D. I've had this \
                                dream forever: how would a Souls-like have looked and controlled on PS1? Just grab \
                                the Souls formula we all know and love and try to make it work in 1999, would that \
                                have even been possible given the limitations?\n\n\
                                So I began trying Godot, Love2D, Picotron, and each felt wrong. I felt more like \
                                I was bending the tool for something it wasn't designed for. I also tried targeting \
                                real PS1 hardware, but I couldn't deal with the primitive SDKs and the distribution \
                                nightmare that could have ensued. I missed the all-in-one feeling that PICO-8 gives, \
                                and how easy it is to have other people play your game. So I decided to build my own \
                                to fill the gap following the same principles.");

                            draw_section(ui, "Where to start",
                                "Use the tabs at the top to switch between the available tools:\n\n\
                                World – Build levels using a sector-based editor in the style of the Tomb Raider \
                                Level Editor. Features a 2D grid view, 3D preview, and portals.\n\n\
                                Assets – A low-poly mesh modeler featuring Blender-style controls, extrusion, \
                                multi-object editing, and a shared texture atlas. Heavily influenced by PicoCAD.\n\n\
                                Paint – Create indexed textures with limited palettes. Draw with 4-bit or 8-bit \
                                color depth, apply dithering patterns, and manage a library of reusable textures.\n\n\
                                Music – A pattern-based tracker for composing music. Supports SF2 soundfonts, \
                                up to 8 channels, and classic tracker effects like arpeggio and vibrato.");

                            // ── FAQ ───────────────────────────────────────
                            ui.add_space(4.0);
                            ui.label(RichText::new("FAQ").color(ACCENT).size(theme::FONT_SIZE_HEADING));
                            ui.add_space(8.0);

                            draw_faq(ui, "Is this a game or a tool?",
                                "Both! The primary goal is to ship a Souls-like game set in a PS1-style world. \
                                But BONNIE-32 and its creative tools are part of the package – think PICO-8, \
                                but for PS1-era 3D games. Everything you need to build, texture, and compose.");

                            draw_faq(ui, "Why not use Unity/Unreal/Godot?",
                                "Those engines are designed for modern games. Getting true PS1-style rendering \
                                requires fighting against their design. Building from scratch lets me embrace the \
                                limitations rather than simulate them.");

                            draw_faq(ui, "Will this be on Steam?",
                                "That's the plan! The native build is intended for Steam distribution. The web \
                                version serves as a free demo and development playground.\n\n\
                                This will always be fully open source. Even if there's a paid Steam version, \
                                you can always clone the repo and build it yourself for free.");

                            draw_faq(ui, "Can I use this to make my own game?",
                                "Absolutely – feel free to use BONNIE-32 however you like! Contributing assets or \
                                ideas would be awesome, but you're welcome to build your own thing too. Like any \
                                fantasy console, there are intentional constraints – embrace them! Note: Some code \
                                and assets have their own licenses. Please review THIRD_PARTY.md before distributing.");

                            draw_faq(ui, "Will you add scripting language support?",
                                "Maybe, but it's not the immediate plan. The focus is on building a PS1-like \
                                platform with modern, flexible tools. Scripting might come later if there's a \
                                clear need for it.");

                            draw_faq(ui, "Was this made with AI?",
                                "Kinda – I use Claude Code extensively to speed up development. But this isn't \
                                \"vibe coding\" where you just accept whatever the AI generates. Every design \
                                decision, architecture choice, and feature is mine. I'm a software engineer by \
                                trade, so the AI is a tool that helps me write code faster, not a replacement \
                                for understanding what I'm building. I review, refactor, and often rewrite what \
                                it produces.");

                            draw_faq(ui, "What's with the name \"BONNIE-32\"?",
                                "\"Bonnie\" comes from my last name – back in my music days, we jokingly called \
                                our makeshift recording setup \"Bonnie Studios\". The \"-32\" follows the fantasy \
                                console naming convention (like PICO-8) and hints at the 32-bit PS1 era this \
                                platform emulates.");

                            // ── FOOTER ───────────────────────────────────
                            ui.add_space(20.0);
                            ui.label(
                                RichText::new("Created by Emanuele Bonura")
                                    .color(theme::TEXT)
                                    .size(theme::FONT_SIZE_UI),
                            );
                            ui.add_space(8.0);
                            ui.horizontal(|ui| {
                                ui.hyperlink_to("GitHub",          "https://github.com/EBonura/bonnie-32");
                                ui.label(RichText::new("|").color(MUTED));
                                ui.hyperlink_to("itch.io",         "https://bonnie-games.itch.io/");
                                ui.label(RichText::new("|").color(MUTED));
                                ui.hyperlink_to("Buy Me a Coffee", "https://buymeacoffee.com/bonniegames");
                            });
                            ui.add_space(40.0);
                        });
                    });
    }
}

impl Default for AboutPanel {
    fn default() -> Self { Self::new() }
}

// ── Helpers ──────────────────────────────────────────────────────────────────

fn draw_section(ui: &mut egui::Ui, title: &str, body: &str) {
    egui::Frame::new()
        .fill(Color32::from_rgb(30, 30, 35))
        .corner_radius(egui::CornerRadius::same(4))
        .inner_margin(egui::Margin::same(16))
        .outer_margin(egui::Margin { left: 0, right: 0, top: 0, bottom: 20 })
        .show(ui, |ui| {
            ui.label(RichText::new(title).color(ACCENT).size(theme::FONT_SIZE_UI));
            ui.add_space(6.0);
            // Render each \n\n as a blank line gap, single \n as new paragraph
            for para in body.split("\n\n") {
                for line in para.split('\n') {
                    ui.label(RichText::new(line).color(theme::TEXT).size(theme::FONT_SIZE_UI));
                }
                ui.add_space(6.0);
            }
        });
}

fn draw_faq(ui: &mut egui::Ui, question: &str, answer: &str) {
    egui::Frame::new()
        .fill(Color32::from_rgb(30, 30, 35))
        .corner_radius(egui::CornerRadius::same(4))
        .inner_margin(egui::Margin::same(16))
        .outer_margin(egui::Margin { left: 0, right: 0, top: 0, bottom: 12 })
        .show(ui, |ui| {
            ui.label(RichText::new(question).color(ACCENT).size(theme::FONT_SIZE_UI));
            ui.add_space(4.0);
            for para in answer.split("\n\n") {
                for line in para.split('\n') {
                    ui.label(RichText::new(line).color(MUTED).size(theme::FONT_SIZE_UI));
                }
                ui.add_space(4.0);
            }
        });
}
