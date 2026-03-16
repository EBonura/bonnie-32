//! UI theme — matches v1 macroquad color scheme exactly.
//! Key identity: teal/cyan accent, charcoal background, VT323 pixel font.

use egui::{Color32, FontId, FontFamily, Stroke, Visuals, style::*, epaint::CornerRadius};

// ---------------------------------------------------------------------------
// Base palette — matched from v1 screenshots
// ---------------------------------------------------------------------------
pub const BG: Color32           = Color32::from_rgb(20,  20,  24);   // main window bg
pub const PANEL_BG: Color32     = Color32::from_rgb(26,  26,  30);   // panel bg
pub const HEADER_BG: Color32    = Color32::from_rgb(18,  18,  22);   // toolbar/header bg
pub const CONTENT_BG: Color32   = Color32::from_rgb(22,  22,  26);   // inner content areas

pub const TEXT: Color32         = Color32::from_rgb(204, 204, 217);  // 0.8 * 255 — v1 TEXT_COLOR
pub const TEXT_DIM: Color32     = Color32::from_rgb(102, 102, 115);  // 0.4 * 255 — v1 TEXT_DIM

pub const WIDGET_BG: Color32    = Color32::from_rgb(38,  38,  46);
pub const WIDGET_HOVER: Color32 = Color32::from_rgb(52,  52,  62);

// Cyan/teal accent — the defining color of v1's identity
pub const ACCENT: Color32       = Color32::from_rgb(0,   180, 180);
pub const ACCENT_HOVER: Color32 = Color32::from_rgb(0,   210, 210);
pub const ACCENT_DIM: Color32   = Color32::from_rgb(0,   120, 120);

pub const SEPARATOR: Color32    = Color32::from_rgb(45,  45,  52);

// ---------------------------------------------------------------------------
// Tracker-specific (v1 theme.rs exact values * 255)
// ---------------------------------------------------------------------------
pub const ROW_EVEN: Color32       = Color32::from_rgb(33,  33,  38);  // 0.13 * 255
pub const ROW_ODD: Color32        = Color32::from_rgb(28,  28,  33);  // 0.11 * 255
pub const ROW_BEAT: Color32       = Color32::from_rgb(41,  36,  31);  // 0.16, 0.14, 0.12
pub const ROW_HIGHLIGHT: Color32  = Color32::from_rgb(51,  64,  77);  // 0.2, 0.25, 0.3
pub const CURSOR_COLOR: Color32   = Color32::from_rgba_premultiplied(77, 128, 204, 204);
pub const PLAYBACK_ROW: Color32   = Color32::from_rgba_premultiplied(102, 51, 51, 153);

pub const NOTE_COLOR: Color32     = Color32::from_rgb(230, 217, 128); // 0.9, 0.85, 0.5
pub const INST_COLOR: Color32     = Color32::from_rgb(128, 204, 128); // 0.5, 0.8, 0.5
pub const VOL_COLOR: Color32      = Color32::from_rgb(128, 179, 230); // 0.5, 0.7, 0.9
pub const FX_COLOR: Color32       = Color32::from_rgb(230, 128, 179); // 0.9, 0.5, 0.7

// ---------------------------------------------------------------------------
// World editor grid colors
// ---------------------------------------------------------------------------
pub const GRID_BG: Color32        = Color32::from_rgb(18,  18,  22);
pub const GRID_LINE: Color32      = Color32::from_rgb(36,  36,  42);
pub const GRID_AXIS_X: Color32    = Color32::from_rgb(80,  35,  35);
pub const GRID_AXIS_Z: Color32    = Color32::from_rgb(35,  35,  80);

pub const SECTOR_FLOOR: Color32   = Color32::from_rgb(40,  65,  40);
pub const SECTOR_CEIL: Color32    = Color32::from_rgb(40,  40,  65);
pub const SECTOR_BORDER: Color32  = Color32::from_rgb(55,  80,  55);
pub const SECTOR_HOVER: Color32   = Color32::from_rgb(0,   160, 100);
pub const SECTOR_SELECT: Color32  = Color32::from_rgb(0,   180, 180); // matches accent
pub const WALL_COLOR: Color32     = Color32::from_rgb(140,  90,  60);

// ---------------------------------------------------------------------------
// Font size constants (v1 values, plus bigger for modern display)
// ---------------------------------------------------------------------------
pub const FONT_SIZE_UI: f32      = 15.0;  // body / button text
pub const FONT_SIZE_SMALL: f32   = 13.0;
pub const FONT_SIZE_HEADING: f32 = 17.0;
pub const FONT_SIZE_MONO: f32    = 16.0;  // VT323 at 16px reads clearly
pub const ICON_SIZE_SM: f32      = 16.0;
pub const ICON_SIZE_MD: f32      = 20.0;
pub const ICON_SIZE_LG: f32      = 24.0;

// ---------------------------------------------------------------------------
// Apply theme to egui context — call once at startup
// ---------------------------------------------------------------------------
pub fn apply(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();

    // --- Lucide icon font ---
    match std::fs::read("assets/fonts/lucide.ttf") {
        Ok(bytes) => {
            fonts.font_data.insert("lucide".to_owned(), egui::FontData::from_owned(bytes).into());
            // Named family for explicit use
            fonts.families.entry(egui::FontFamily::Name("lucide".into())).or_default().push("lucide".to_owned());
            // Fallback on Proportional so icon chars render in plain RichText
            fonts.families.entry(egui::FontFamily::Proportional).or_default().push("lucide".to_owned());
            // Fallback on Monospace too (tracker, code editor)
            fonts.families.entry(egui::FontFamily::Monospace).or_default().push("lucide".to_owned());
        }
        Err(_) => log::warn!("Lucide font not found at assets/fonts/lucide.ttf"),
    }

    // --- VT323 pixel font — used as primary UI font (matches v1 tab bar) ---
    match std::fs::read("assets/fonts/VT323-Regular.ttf") {
        Ok(bytes) => {
            fonts.font_data.insert("VT323".to_owned(), egui::FontData::from_owned(bytes).into());
            // VT323 first in Proportional — this makes the whole UI use the pixel font
            fonts.families.entry(egui::FontFamily::Proportional).or_default().insert(0, "VT323".to_owned());
            // Also first in Monospace (tracker row numbers, code)
            fonts.families.entry(egui::FontFamily::Monospace).or_default().insert(0, "VT323".to_owned());
        }
        Err(_) => log::warn!("VT323 font not found at assets/fonts/VT323-Regular.ttf"),
    }

    ctx.set_fonts(fonts);

    // --- Text styles (larger sizes for VT323 readability) ---
    let mut style = (*ctx.style()).clone();
    style.text_styles = [
        (TextStyle::Small,     FontId::new(FONT_SIZE_SMALL,   FontFamily::Proportional)),
        (TextStyle::Body,      FontId::new(FONT_SIZE_UI,      FontFamily::Proportional)),
        (TextStyle::Monospace, FontId::new(FONT_SIZE_MONO,    FontFamily::Monospace)),
        (TextStyle::Button,    FontId::new(FONT_SIZE_UI,      FontFamily::Proportional)),
        (TextStyle::Heading,   FontId::new(FONT_SIZE_HEADING, FontFamily::Proportional)),
    ].into();
    // Slightly more breathing room between items
    style.spacing.item_spacing    = egui::vec2(8.0, 5.0);
    style.spacing.button_padding  = egui::vec2(8.0, 4.0);
    style.spacing.indent          = 14.0;
    ctx.set_style(style);

    ctx.set_visuals(make_visuals());
}

fn make_visuals() -> Visuals {
    let mut v = Visuals::dark();

    v.override_text_color = Some(TEXT);

    // Backgrounds
    v.window_fill      = PANEL_BG;
    v.panel_fill       = PANEL_BG;
    v.faint_bg_color   = CONTENT_BG;
    v.extreme_bg_color = BG;

    v.window_corner_radius = CornerRadius::same(4);
    v.window_stroke        = Stroke::new(1.0, SEPARATOR);
    v.popup_shadow         = egui::Shadow::NONE;

    // Widget states
    let base = WidgetVisuals {
        bg_fill:      WIDGET_BG,
        weak_bg_fill: Color32::from_rgb(32, 32, 38),
        bg_stroke:    Stroke::new(1.0, Color32::from_rgb(52, 52, 62)),
        corner_radius: CornerRadius::same(3),
        fg_stroke:    Stroke::new(1.0, TEXT),
        expansion:    0.0,
    };
    v.widgets.noninteractive = WidgetVisuals {
        bg_fill:      PANEL_BG,
        weak_bg_fill: PANEL_BG,
        bg_stroke:    Stroke::new(1.0, SEPARATOR),
        fg_stroke:    Stroke::new(1.0, TEXT_DIM),
        corner_radius: CornerRadius::same(3),
        expansion:    0.0,
    };
    v.widgets.inactive = base.clone();
    let mut hov = base.clone();
    hov.bg_fill   = WIDGET_HOVER;
    hov.bg_stroke = Stroke::new(1.0, ACCENT_DIM);
    v.widgets.hovered = hov;
    let mut act = base.clone();
    act.bg_fill   = ACCENT_DIM;
    act.fg_stroke = Stroke::new(1.5, Color32::WHITE);
    v.widgets.active = act;
    let mut open = base;
    open.bg_fill   = Color32::from_rgb(0, 70, 70);
    open.fg_stroke = Stroke::new(1.0, Color32::WHITE);
    v.widgets.open = open;

    // Selection — teal tint
    v.selection.bg_fill = Color32::from_rgba_premultiplied(0, 180, 180, 60);
    v.selection.stroke  = Stroke::new(1.0, ACCENT);

    v.hyperlink_color = ACCENT;
    v.warn_fg_color   = Color32::from_rgb(220, 180, 60);
    v.error_fg_color  = Color32::from_rgb(220, 80,  80);

    v
}
