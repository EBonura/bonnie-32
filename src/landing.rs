//! Landing page / Home tab
//!
//! Displays introduction, motivation, and FAQ for Bonnie Engine.

use macroquad::prelude::*;
use crate::ui::Rect;
use crate::VERSION;

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
pub fn draw_landing(rect: Rect, state: &mut LandingState) {
    // DPI scale for high-DPI displays (layout only, not font sizes)
    let dpi = screen_dpi_scale();

    // Background
    draw_rectangle(rect.x, rect.y, rect.w, rect.h, BG_COLOR);

    // Handle scrolling
    let scroll_delta = mouse_wheel().1 * 3.0;
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
    let content_width = (rect.w - padding * 2.0).min(800.0).round();
    let content_x = (rect.x + (rect.w - content_width) / 2.0).round();
    let mut y = (rect.y + padding + state.scroll_y).round();

    // === HEADER ===
    let title = format!("Bonnie Engine v{}", VERSION);
    draw_text(&title, content_x, y + 32.0, 32.0, ACCENT_COLOR);
    y += 44.0;

    draw_text("A PS1-Style Modern Game Engine", content_x, y + 18.0, 18.0, MUTED_COLOR);
    y += 54.0;

    // === INTRO SECTION ===
    y = draw_section(content_x, y, content_width, "What is this?", &[
        "Bonnie Engine is a complete game development environment built from scratch in",
        "Rust, designed to recreate the authentic PlayStation 1 aesthetic.",
        "",
        "Everything you see - the software rasterizer, the editor UI, the level format -",
        "is custom code. The world-building system takes heavy inspiration from the",
        "Tomb Raider series, which remains one of the best examples of how complex 3D",
        "worlds could be achieved on PS1 hardware.",
        "",
        "A key principle: everything runs as a single platform, both natively and in the",
        "browser. Same code, same tools, same experience - no compromises on either side.",
    ]);

    // === WHY SECTION ===
    y = draw_section(content_x, y, content_width, "Why build this?", &[
        "Mostly to answer the question: what would a Souls-like have looked like on a PS1?",
        "There are great examples like Bloodborne PSX by Lilith Walther, built in Unity.",
        "I wanted to try my own approach from scratch.",
        "",
        "Modern retro-style games typically achieve the aesthetic top-down with shaders",
        "and post-processing, often with great results. I wanted to try the opposite: a",
        "bottom-up approach with a real software rasterizer that works like the PS1's GTE.",
        "",
        "I tried several approaches before landing here: first LOVR, then Picotron, even",
        "coding for actual PS1 hardware. Each had limitations - primitive SDKs, distribution",
        "headaches, or not enough flexibility. Rust + WASM turned out to be the sweet spot:",
        "native performance, browser deployment, and a modern toolchain.",
    ]);

    // === FAQ SECTION ===
    draw_text("FAQ", content_x, y + 16.0, 16.0, ACCENT_COLOR);
    y += 30.0;

    y = draw_faq_item(content_x, y, content_width,
        "Is this a game or an engine?",
        "Both! The goal is to ship a complete Souls-like game, but the engine and\neditor are part of the package. Think of it like RPG Maker but for PS1 games.",
    );

    y = draw_faq_item(content_x, y, content_width,
        "Why not use Unity/Unreal/Godot?",
        "Those engines are designed for modern games. Getting true PS1-style rendering\nrequires fighting against their design. Building from scratch lets me embrace\nthe limitations rather than simulate them.",
    );

    y = draw_faq_item(content_x, y, content_width,
        "Will this be on Steam?",
        "Probably! The native build is intended for Steam distribution.\nThe web version may be offered as a free demo or a SaaS product - not to\nmonetize, just to cover hosting costs if the userbase grows.",
    );

    y = draw_faq_item(content_x, y, content_width,
        "Can I use this to make my own game?",
        "Absolutely - feel free to use this however you like! Contributing assets or ideas to my\nproject would be awesome, but you're welcome to build your own thing too. Just keep in mind\nthis isn't a general-purpose engine - it's tailored to my specific vision, so it may lack\nfeatures you'd expect. Note: Some code and assets have their own licenses. Please review\nTHIRD_PARTY.md before using or distributing anything.",
    );

    y = draw_faq_item(content_x, y, content_width,
        "Will you add scripting language support?",
        "Maybe, but it's not the immediate plan. The focus is on building a PS1-like\nplatform with modern, flexible tools. Scripting might come later if there's\na clear need for it.",
    );

    y = draw_faq_item(content_x, y, content_width,
        "What's with the name \"Bonnie\"?",
        "Back in my short but intense music career as a metal guitarist, we'd record\ndemos on a cheap laptop with makeshift gear in whatever garage was available.\nWe jokingly called it \"Bonnie Studios\" - a playful twist on my last name.\nThis engine carries on that DIY spirit.",
    );

    // === FOOTER ===
    y += 20.0;
    draw_text("Made with love and Rust", content_x, y + 16.0, 16.0, MUTED_COLOR);
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

/// Draw a section with title and body text
fn draw_section(x: f32, y: f32, width: f32, title: &str, lines: &[&str]) -> f32 {
    let x = x.round();
    let y = y.round();
    let text_x = x + 16.0;

    let line_height = 22.0;
    let title_height = 26.0;
    let padding = 16.0;
    let section_height = title_height + padding + (lines.len() as f32 * line_height) + padding;

    draw_rectangle(x, y, width.round(), section_height, SECTION_BG);

    draw_text(title, text_x, y + padding + 16.0, 16.0, ACCENT_COLOR);

    let mut text_y = y + padding + title_height;
    for line in lines {
        draw_text(line, text_x, text_y + 16.0, 16.0, TEXT_COLOR);
        text_y += line_height;
    }

    y + section_height + 20.0
}

/// Draw an FAQ item
fn draw_faq_item(x: f32, y: f32, width: f32, question: &str, answer: &str) -> f32 {
    let x = x.round();
    let y = y.round();
    let text_x = x + 16.0;
    let padding = 16.0;

    let line_height = 20.0;
    let answer_lines: Vec<&str> = answer.lines().collect();
    let section_height = 26.0 + padding + (answer_lines.len() as f32 * line_height) + padding;

    draw_rectangle(x, y, width.round(), section_height, SECTION_BG);

    draw_text(question, text_x, y + padding + 16.0, 16.0, ACCENT_COLOR);

    let mut text_y = y + padding + 26.0;
    for line in answer_lines {
        draw_text(line, text_x, text_y + 16.0, 16.0, MUTED_COLOR);
        text_y += line_height;
    }

    y + section_height + 12.0
}
