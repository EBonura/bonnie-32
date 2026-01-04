//! Landing page / Home tab
//!
//! Displays introduction, motivation, and FAQ for Bonnie Engine.

use macroquad::prelude::*;
use crate::ui::{Rect, draw_link_row};
use crate::VERSION;

/// Wrap text to fit within a given pixel width
/// Returns a vector of lines that fit within max_width
fn wrap_text(text: &str, font_size: f32, max_width: f32) -> Vec<String> {
    let mut lines = Vec::new();

    // First split by explicit newlines to preserve paragraph breaks
    for paragraph in text.split('\n') {
        if paragraph.is_empty() {
            lines.push(String::new());
            continue;
        }

        let words: Vec<&str> = paragraph.split_whitespace().collect();
        if words.is_empty() {
            lines.push(String::new());
            continue;
        }

        let mut current_line = String::new();

        for word in words {
            let test_line = if current_line.is_empty() {
                word.to_string()
            } else {
                format!("{} {}", current_line, word)
            };

            // Measure text width (approximate: ~0.5 * font_size per character for monospace-ish)
            // macroquad's measure_text can be slow, so use approximation
            let char_width = font_size * 0.55;
            let test_width = test_line.len() as f32 * char_width;

            if test_width <= max_width || current_line.is_empty() {
                current_line = test_line;
            } else {
                lines.push(current_line);
                current_line = word.to_string();
            }
        }

        if !current_line.is_empty() {
            lines.push(current_line);
        }
    }

    lines
}

/// Colors matching the editor theme
const BG_COLOR: Color = Color::new(0.10, 0.10, 0.12, 1.0);
const TEXT_COLOR: Color = Color::new(0.9, 0.9, 0.9, 1.0);
const MUTED_COLOR: Color = Color::new(0.6, 0.6, 0.65, 1.0);
const ACCENT_COLOR: Color = Color::new(0.0, 0.75, 0.9, 1.0);
const SECTION_BG: Color = Color::new(0.12, 0.12, 0.14, 1.0);

/// State for the landing page (scroll position)
pub struct LandingState {
    pub scroll_y: f32,
}

impl LandingState {
    pub fn new() -> Self {
        Self { scroll_y: 0.0 }
    }
}

/// Draw the landing page
pub fn draw_landing(rect: Rect, state: &mut LandingState, ctx: &crate::ui::UiContext) {
    // DPI scale for high-DPI displays (layout only, not font sizes)
    let dpi = screen_dpi_scale();

    // Background
    draw_rectangle(rect.x, rect.y, rect.w, rect.h, BG_COLOR);

    // Handle scrolling
    let scroll_delta = ctx.mouse.scroll * 3.0;
    state.scroll_y += scroll_delta;
    state.scroll_y = state.scroll_y.min(0.0); // Can't scroll above top

    // Enable scissor clipping to prevent content from overflowing into tab bar
    // Scissor uses physical pixels, so scale by DPI
    gl_use_default_material();
    unsafe {
        get_internal_gl().quad_gl.scissor(
            Some((
                (rect.x * dpi) as i32,
                (rect.y * dpi) as i32,
                (rect.w * dpi) as i32,
                (rect.h * dpi) as i32
            ))
        );
    }

    // Content area with padding (all in logical pixels)
    let padding = 40.0;
    let content_width = (rect.w - padding * 2.0).min(1000.0).round();
    let content_x = (rect.x + (rect.w - content_width) / 2.0).round();
    let mut y = (rect.y + padding + state.scroll_y).round();

    // === HEADER ===
    let title = format!("Bonnie Engine v{}", VERSION);
    draw_text(&title, content_x, y + 32.0, 32.0, ACCENT_COLOR);
    y += 44.0;

    draw_text("A PS1-Style Game Engine Built From Scratch", content_x, y + 18.0, 18.0, MUTED_COLOR);
    y += 54.0;

    // === INTRO SECTION ===
    y = draw_section(content_x, y, content_width, "What is this?",
        "Bonnie Engine is a complete game development environment built from scratch in Rust, designed to recreate the authentic PlayStation 1 aesthetic.\n\nEverything you see - the software rasterizer, the editor UI, the level format - is custom code. The world-building system takes heavy inspiration from the Tomb Raider series, which remains one of the best examples of how complex 3D worlds could be achieved on PS1 hardware.\n\nA key principle: everything runs as a single platform, both natively and in the browser. Same code, same tools, same experience - no compromises on either side."
    );

    // === PS1 FEATURES SECTION ===
    y = draw_section(content_x, y, content_width, "Authentic PS1 rendering",
        "The software rasterizer recreates the quirks that defined the PS1 look:\n\n- Affine texture mapping (no perspective correction = that signature warping)\n- Vertex snapping to integer coordinates (the subtle jitter on moving objects)\n- Limited color depth and dithering\n- No sub-pixel precision (polygons \"pop\" when they move)\n\nThese aren't post-processing effects - they're how the renderer actually works."
    );

    // === WHY SECTION ===
    y = draw_section(content_x, y, content_width, "Why build this?",
        "It started with a question: what would a Souls-like have looked like on a PS1? There are great examples like Bloodborne PSX by Lilith Walther, built in Unity. I wanted to try my own approach from scratch.\n\nBut I can see this expanding beyond Souls-like games. The engine could support tactical RPGs (think FF Tactics), platformers, survival horror, or any genre that benefits from the PS1 aesthetic. The goal is a flexible creative tool.\n\nModern retro-style games typically achieve the aesthetic top-down with shaders and post-processing, often with great results. I wanted to try the opposite: a bottom-up approach with a real software rasterizer that works like the PS1's GTE.\n\nI tried several approaches before landing here: first LOVR, then Picotron, even coding for actual PS1 hardware. Each had limitations - primitive SDKs, distribution headaches, or not enough flexibility. Rust + WASM turned out to be the sweet spot: native performance, browser deployment, and a modern toolchain."
    );

    // === WHERE TO START SECTION ===
    y = draw_section(content_x, y, content_width, "Where to start",
        "Use the tabs at the top to switch between the available tools:\n\nWorld - Build levels using a sector-based editor inspired by classic tools like the Tomb Raider Level Editor. Features a 2D grid view, 3D preview, and portals.\n\nAssets - A low-poly mesh modeler designed for PS1-style models. Includes Blender-style controls (G/S/R for grab/scale/rotate), extrude, multi-object editing, and a shared texture atlas. PicoCAD was a major influence here.\n\nMusic - A pattern-based tracker for composing music. Supports SF2 soundfonts, up to 8 channels, and classic tracker effects like arpeggio and vibrato."
    );

    // === FAQ SECTION ===
    draw_text("FAQ", content_x, y + 16.0, 16.0, ACCENT_COLOR);
    y += 30.0;

    y = draw_faq_item(content_x, y, content_width,
        "Is this a game or an engine?",
        "Both! The primary goal is to ship a Souls-like game set in a PS1-style world. But the engine and creative tools are part of the package - think RPG Maker, but for PS1-era 3D games. Everything you need to build, animate, and compose.",
    );

    y = draw_faq_item(content_x, y, content_width,
        "Why not use Unity/Unreal/Godot?",
        "Those engines are designed for modern games. Getting true PS1-style rendering requires fighting against their design. Building from scratch lets me embrace the limitations rather than simulate them.",
    );

    y = draw_faq_item(content_x, y, content_width,
        "Will this be on Steam?",
        "That's the plan! The native build is intended for Steam distribution. The web version serves as a free demo and development playground.\n\nThis will always be fully open source. Even if there's a paid Steam version, you can always clone the repo and build it yourself for free.",
    );

    y = draw_faq_item(content_x, y, content_width,
        "Can I use this to make my own game?",
        "Absolutely - feel free to use this however you like! Contributing assets or ideas to my project would be awesome, but you're welcome to build your own thing too. Just keep in mind this isn't a general-purpose engine - it's tailored to my specific vision, so it may lack features you'd expect. Note: Some code and assets have their own licenses. Please review THIRD_PARTY.md before using or distributing anything.",
    );

    y = draw_faq_item(content_x, y, content_width,
        "Will you add scripting language support?",
        "Maybe, but it's not the immediate plan. The focus is on building a PS1-like platform with modern, flexible tools. Scripting might come later if there's a clear need for it.",
    );

    y = draw_faq_item(content_x, y, content_width,
        "Was this made with AI?",
        "Kinda - I use Claude Code extensively to speed up development. But this isn't \"vibe coding\" where you just accept whatever the AI generates. Every design decision, architecture choice, and feature is mine. I'm a software engineer by trade, so the AI is a tool that helps me write code faster, not a replacement for understanding what I'm building. I review, refactor, and often rewrite what it produces.",
    );

    y = draw_faq_item(content_x, y, content_width,
        "What's with the name \"Bonnie\"?",
        "Back in my short but intense music career as a metal guitarist, we'd record demos on a cheap laptop with makeshift gear in whatever garage was available. We jokingly called it \"Bonnie Studios\" - a playful twist on my last name. This engine carries on that DIY spirit.",
    );

    // === FOOTER ===
    y += 20.0;
    draw_text("Created by Emanuele Bonura", content_x, y + 16.0, 16.0, TEXT_COLOR);
    y += 28.0;

    // Clickable links row
    let link_color = MUTED_COLOR;
    let hover_color = ACCENT_COLOR;
    draw_link_row(
        content_x,
        y + 14.0,
        &[
            ("GitHub", "https://github.com/EBonura/bonnie-engine"),
            ("itch.io", "https://bonnie-games.itch.io/"),
            ("Buy Me a Coffee", "https://buymeacoffee.com/bonniegames"),
        ],
        "  |  ",
        14.0,
        link_color,
        hover_color,
        MUTED_COLOR,
        ctx,
    );
    y += 30.0;

    // Clamp scroll to content
    let content_height = y - rect.y - state.scroll_y;
    let max_scroll = -(content_height - rect.h + padding).max(0.0);
    state.scroll_y = state.scroll_y.max(max_scroll);

    // Disable scissor clipping
    unsafe {
        get_internal_gl().quad_gl.scissor(None);
    }
}

/// Draw a section with title and body text (auto-wrapping)
fn draw_section(x: f32, y: f32, width: f32, title: &str, text: &str) -> f32 {
    let x = x.round();
    let y = y.round();
    let padding = 16.0;
    let text_x = x + padding;
    let text_width = width - padding * 2.0;

    let line_height = 22.0;
    let title_height = 26.0;
    let font_size = 16.0;

    // Wrap the text to fit the available width
    let lines = wrap_text(text, font_size, text_width);
    let section_height = title_height + padding + (lines.len() as f32 * line_height) + padding;

    draw_rectangle(x, y, width.round(), section_height, SECTION_BG);

    draw_text(title, text_x, y + padding + 16.0, font_size, ACCENT_COLOR);

    let mut text_y = y + padding + title_height;
    for line in &lines {
        draw_text(line, text_x, text_y + 16.0, font_size, TEXT_COLOR);
        text_y += line_height;
    }

    y + section_height + 20.0
}

/// Draw an FAQ item (auto-wrapping)
fn draw_faq_item(x: f32, y: f32, width: f32, question: &str, answer: &str) -> f32 {
    let x = x.round();
    let y = y.round();
    let padding = 16.0;
    let text_x = x + padding;
    let text_width = width - padding * 2.0;

    let line_height = 20.0;
    let font_size = 16.0;

    // Wrap the answer text to fit the available width
    let answer_lines = wrap_text(answer, font_size, text_width);
    let section_height = 26.0 + padding + (answer_lines.len() as f32 * line_height) + padding;

    draw_rectangle(x, y, width.round(), section_height, SECTION_BG);

    draw_text(question, text_x, y + padding + 16.0, font_size, ACCENT_COLOR);

    let mut text_y = y + padding + 26.0;
    for line in &answer_lines {
        draw_text(line, text_x, text_y + 16.0, font_size, MUTED_COLOR);
        text_y += line_height;
    }

    y + section_height + 12.0
}
