//! UI theme — translated from v1's macroquad Color constants to egui Color32.

use egui::{Color32, Stroke, Visuals, style::*, epaint::CornerRadius};

// ---------------------------------------------------------------------------
// Base palette (matches v1 exactly)
// ---------------------------------------------------------------------------
pub const BG: Color32          = Color32::from_rgb(28,  28,  33);   // 0.11 * 255
pub const HEADER: Color32      = Color32::from_rgb(38,  38,  46);   // 0.15 * 255
pub const TEXT: Color32        = Color32::from_rgb(204, 204, 217);  // 0.8 * 255
pub const TEXT_DIM: Color32    = Color32::from_rgb(102, 102, 115);  // 0.4 * 255

pub const PANEL_BG: Color32    = Color32::from_rgb(22,  22,  27);
pub const WIDGET_BG: Color32   = Color32::from_rgb(45,  45,  55);
pub const WIDGET_HOVER: Color32= Color32::from_rgb(60,  60,  72);
pub const ACCENT: Color32      = Color32::from_rgb(80,  120, 200);
pub const ACCENT_HOVER: Color32= Color32::from_rgb(100, 145, 230);

// ---------------------------------------------------------------------------
// Tracker-specific (ported from v1 theme.rs)
// ---------------------------------------------------------------------------
pub const ROW_EVEN: Color32       = Color32::from_rgb(33,  33,  38);
pub const ROW_ODD: Color32        = Color32::from_rgb(28,  28,  33);
pub const ROW_BEAT: Color32       = Color32::from_rgb(41,  36,  31);
pub const ROW_HIGHLIGHT: Color32  = Color32::from_rgb(51,  64,  77);
pub const CURSOR_COLOR: Color32   = Color32::from_rgba_premultiplied(77, 128, 204, 204);
pub const PLAYBACK_ROW: Color32   = Color32::from_rgba_premultiplied(102, 51, 51, 153);

pub const NOTE_COLOR: Color32     = Color32::from_rgb(230, 217, 128);
pub const INST_COLOR: Color32     = Color32::from_rgb(128, 204, 128);
pub const VOL_COLOR: Color32      = Color32::from_rgb(128, 179, 230);
pub const FX_COLOR: Color32       = Color32::from_rgb(230, 128, 179);

// ---------------------------------------------------------------------------
// World editor grid colors
// ---------------------------------------------------------------------------
pub const GRID_BG: Color32        = Color32::from_rgb(20,  20,  25);
pub const GRID_LINE: Color32      = Color32::from_rgb(40,  40,  45);
pub const GRID_AXIS_X: Color32    = Color32::from_rgb(80,  40,  40);
pub const GRID_AXIS_Z: Color32    = Color32::from_rgb(40,  40,  80);

pub const SECTOR_FLOOR: Color32   = Color32::from_rgb(55,  75,  55);
pub const SECTOR_CEIL: Color32    = Color32::from_rgb(55,  55,  80);
pub const SECTOR_BORDER: Color32  = Color32::from_rgb(70,  90,  70);
pub const SECTOR_HOVER: Color32   = Color32::from_rgb(80, 120,  80);
pub const SECTOR_SELECT: Color32  = Color32::from_rgb(80, 130, 200);
pub const WALL_COLOR: Color32     = Color32::from_rgb(140,  90,  60);

// ---------------------------------------------------------------------------
// Apply theme to egui context
// ---------------------------------------------------------------------------
pub fn apply(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();

    // Load Lucide icon font.
    // Added as fallback to BOTH Proportional and Monospace so icon codepoints
    // resolve automatically in any RichText without needing a special FontId.
    if let Ok(lucide_bytes) = std::fs::read("assets/fonts/lucide.ttf") {
        fonts.font_data.insert(
            "lucide".to_owned(),
            egui::FontData::from_owned(lucide_bytes).into(),
        );
        // Named family for explicit use
        fonts
            .families
            .entry(egui::FontFamily::Name("lucide".into()))
            .or_default()
            .push("lucide".to_owned());
        // Fallback on proportional — makes icons work in plain RichText
        fonts
            .families
            .entry(egui::FontFamily::Proportional)
            .or_default()
            .push("lucide".to_owned());
        // Fallback on monospace too (tracker row numbers, code editor)
        fonts
            .families
            .entry(egui::FontFamily::Monospace)
            .or_default()
            .push("lucide".to_owned());
    } else {
        log::warn!("Lucide font not found at assets/fonts/lucide.ttf — icons will show as boxes");
    }

    // Load VT323 mono font (used for tracker row numbers, code)
    if let Ok(vt323_bytes) = std::fs::read("assets/fonts/VT323-Regular.ttf") {
        fonts.font_data.insert(
            "VT323".to_owned(),
            egui::FontData::from_owned(vt323_bytes).into(),
        );
        fonts
            .families
            .entry(egui::FontFamily::Monospace)
            .or_default()
            .insert(0, "VT323".to_owned());
    } else {
        log::warn!("VT323 font not found");
    }

    ctx.set_fonts(fonts);
    ctx.set_visuals(make_visuals());
}

fn make_visuals() -> Visuals {
    let mut v = Visuals::dark();

    v.override_text_color = Some(TEXT);

    // Window / panel backgrounds
    v.window_fill = PANEL_BG;
    v.panel_fill  = PANEL_BG;
    v.faint_bg_color = BG;
    v.extreme_bg_color = Color32::from_rgb(15, 15, 18);

    v.window_corner_radius = CornerRadius::same(4);
    v.window_stroke = Stroke::new(1.0, Color32::from_rgb(55, 55, 65));

    // Widgets
    let mut w = WidgetVisuals {
        bg_fill:   WIDGET_BG,
        weak_bg_fill: Color32::from_rgb(38, 38, 46),
        bg_stroke: Stroke::new(1.0, Color32::from_rgb(60, 60, 72)),
        corner_radius:  CornerRadius::same(3),
        fg_stroke: Stroke::new(1.0, TEXT),
        expansion: 0.0,
    };
    v.widgets.noninteractive = WidgetVisuals {
        bg_fill: PANEL_BG,
        weak_bg_fill: PANEL_BG,
        bg_stroke: Stroke::new(1.0, Color32::from_rgb(50, 50, 60)),
        fg_stroke: Stroke::new(1.0, TEXT_DIM),
        corner_radius: CornerRadius::same(3),
        expansion: 0.0,
    };
    v.widgets.inactive = w.clone();
    w.bg_fill = WIDGET_HOVER;
    w.bg_stroke = Stroke::new(1.0, Color32::from_rgb(80, 80, 95));
    v.widgets.hovered = w.clone();
    w.bg_fill = ACCENT;
    w.fg_stroke = Stroke::new(1.5, Color32::WHITE);
    v.widgets.active = w.clone();
    w.bg_fill = Color32::from_rgb(60, 90, 150);
    w.fg_stroke = Stroke::new(1.0, Color32::WHITE);
    v.widgets.open = w;

    v.selection.bg_fill = Color32::from_rgba_premultiplied(80, 120, 200, 120);
    v.selection.stroke = Stroke::new(1.0, ACCENT_HOVER);

    v.hyperlink_color = ACCENT_HOVER;
    v.warn_fg_color   = Color32::from_rgb(220, 180, 60);
    v.error_fg_color  = Color32::from_rgb(220, 80, 80);

    v
}
