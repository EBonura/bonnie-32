//! Landing page / Home tab
//!
//! Displays introduction, motivation, and FAQ for BONNIE-32.

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
    pub logo_texture: Option<Texture2D>,
}

impl LandingState {
    pub fn new(logo_texture: Option<Texture2D>) -> Self {
        Self {
            scroll_y: 0.0,
            logo_texture,
        }
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
    // Draw logo if available, otherwise fallback to text
    if let Some(logo) = &state.logo_texture {
        // Logo is 800x296 at native size, scale to fit content width nicely
        let logo_max_width = content_width.min(500.0);
        let logo_scale = logo_max_width / logo.width();
        let logo_w = logo.width() * logo_scale;
        let logo_h = logo.height() * logo_scale;
        let logo_x = content_x + (content_width - logo_w) / 2.0;

        draw_texture_ex(
            logo,
            logo_x,
            y,
            WHITE,
            DrawTextureParams {
                dest_size: Some(vec2(logo_w, logo_h)),
                ..Default::default()
            },
        );
        y += logo_h + 8.0;
    } else {
        // Fallback to text title
        let title = format!("BONNIE-32 v{}", VERSION);
        draw_text(&title, content_x, y + 32.0, 32.0, ACCENT_COLOR);
        y += 44.0;
    }

    // Subtitle
    let subtitle = "A Fantasy Console for PS1-Era 3D Games";
    let subtitle_width = subtitle.len() as f32 * 18.0 * 0.55;
    draw_text(subtitle, content_x + (content_width - subtitle_width) / 2.0, y + 18.0, 18.0, MUTED_COLOR);
    y += 54.0;

    // === INTRO SECTION ===
    y = draw_section(content_x, y, content_width, "What is BONNIE-32?",
        "BONNIE-32 is a fantasy console for PS1-era 3D games. Think PICO-8, but for low-poly 3D.\n\nLike PICO-8 unlocked retro 2D gamedev with its constraints and all-in-one tooling, BONNIE-32 aims to do the same for late 90s-style 3D. Everything is built from scratch in Rust: the software rasterizer, the editor UI, the level format. The world-building system takes heavy inspiration from Tomb Raider.\n\nEverything runs as a single platform, both natively and in the browser. Same code, same tools, same experience."
    );

    // === PS1 FEATURES SECTION ===
    y = draw_section(content_x, y, content_width, "Authentic PS1 rendering",
        "The software rasterizer recreates the quirks that defined the PS1 look:\n\n- Affine texture mapping (no perspective correction = that signature warping)\n- Vertex snapping to integer coordinates (the subtle jitter on moving objects)\n- Limited color depth and dithering\n- No sub-pixel precision (polygons \"pop\" when they move)\n\nThese aren't post-processing effects - they're how the renderer actually works."
    );

    // === WHY SECTION ===
    y = draw_section(content_x, y, content_width, "Why build this?",
        "It started with a question: what would a Souls-like have looked like on a PS1?\n\nI tried Godot, Love2D, Picotron, even real PS1 hardware - nothing quite fit. Modern engines simulate the aesthetic with shaders. I wanted to embrace the limitations from the ground up with a real software rasterizer.\n\nThe result is something closer to a fantasy console than an engine. Fixed constraints, integrated tools, and a focus on making PS1-style games accessible to create and share."
    );

    // === WHERE TO START SECTION ===
    y = draw_section(content_x, y, content_width, "Where to start",
        "Use the tabs at the top to switch between the available tools:\n\nWorld - Build levels using a sector-based editor inspired by classic tools like the Tomb Raider Level Editor. Features a 2D grid view, 3D preview, and portals.\n\nAssets - A low-poly mesh modeler designed for PS1-style models. Includes Blender-style controls (G/S/R for grab/scale/rotate), extrude, multi-object editing, and a shared texture atlas. PicoCAD was a major influence here.\n\nPaint - Create custom indexed textures with PS1-style palettes. Draw with 4-bit or 8-bit color depth, apply dithering patterns, and manage a library of reusable textures.\n\nMusic - A pattern-based tracker for composing music. Supports SF2 soundfonts, up to 8 channels, and classic tracker effects like arpeggio and vibrato."
    );

    // === FAQ SECTION ===
    draw_text("FAQ", content_x, y + 16.0, 16.0, ACCENT_COLOR);
    y += 30.0;

    y = draw_faq_item(content_x, y, content_width,
        "Is this a game or a tool?",
        "Both! The primary goal is to ship a Souls-like game set in a PS1-style world. But BONNIE-32 and its creative tools are part of the package - think PICO-8, but for PS1-era 3D games. Everything you need to build, texture, and compose.",
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
        "Absolutely - feel free to use BONNIE-32 however you like! Contributing assets or ideas would be awesome, but you're welcome to build your own thing too. Like any fantasy console, there are intentional constraints - embrace them! Note: Some code and assets have their own licenses. Please review THIRD_PARTY.md before distributing.",
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
        "What's with the name \"BONNIE-32\"?",
        "\"Bonnie\" comes from my last name - back in my music days, we jokingly called our makeshift recording setup \"Bonnie Studios\". The \"-32\" follows the fantasy console naming convention (like PICO-8) and hints at the 32-bit PS1 era this platform emulates.",
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
            ("GitHub", "https://github.com/EBonura/bonnie-32"),
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
