//! Lucide icon codepoints — same as v1, macroquad rendering stripped.
//! Usage: `egui::RichText::new(icon::PLAY.to_string()).font(icons::font(16.0))`

pub mod icon {
    // File operations
    pub const SAVE: char = '\u{e14d}';
    pub const SAVE_AS: char = '\u{e40f}';
    pub const FOLDER_OPEN: char = '\u{e247}';
    pub const FILE_PLUS: char = '\u{e0c9}';
    pub const DOWNLOAD: char = '\u{e099}';

    // Edit operations
    pub const UNDO: char = '\u{e19b}';
    pub const REDO: char = '\u{e143}';
    pub const COPY: char = '\u{e08b}';   // Lucide "copy"
    pub const CLIPBOARD: char = '\u{e07a}'; // Lucide "clipboard"
    pub const ARROW_UP: char = '\u{e1ca}';  // Lucide "arrow-up"
    pub const ARROW_DOWN: char = '\u{e1c8}'; // Lucide "arrow-down"

    // Playback / Transport
    pub const PLAY: char = '\u{e13c}';
    pub const PAUSE: char = '\u{e12e}';
    pub const SQUARE: char = '\u{e167}';
    pub const SKIP_BACK: char = '\u{e15f}';
    pub const SKIP_FORWARD: char = '\u{e160}';

    // UI / Navigation
    pub const PLUS: char = '\u{e13d}';
    pub const MINUS: char = '\u{e11c}';
    pub const TRASH: char = '\u{e18d}';
    pub const MOVE: char = '\u{e121}';
    pub const CHEVRON_UP: char = '\u{e070}';
    pub const CHEVRON_DOWN: char = '\u{e06d}';
    pub const CHEVRON_LEFT: char = '\u{e06e}';
    pub const CHEVRON_RIGHT: char = '\u{e06f}';

    // Link/Unlink
    pub const LINK: char = '\u{e103}';
    pub const LINK_OFF: char = '\u{e104}';

    // Editor tools
    pub const BOX: char = '\u{e061}';
    pub const BRICK_WALL: char = '\u{e581}';
    pub const LAYERS: char = '\u{e529}';
    pub const GRID: char = '\u{e0e9}';
    pub const DOOR_CLOSED: char = '\u{e09a}';

    // Transform / Select tools
    pub const POINTER: char = '\u{e1e8}';
    pub const ROTATE_3D: char = '\u{e2ea}';
    pub const SCALE_3D: char = '\u{e2eb}';
    pub const MAXIMIZE_2: char = '\u{e113}';
    pub const BRUSH: char = '\u{e1d3}';
    pub const PAINT_BUCKET: char = '\u{e2e6}';
    pub const GIT_BRANCH: char = '\u{e1f4}';
    pub const SCAN: char = '\u{e257}';
    pub const CIRCLE_DOT: char = '\u{e345}';
    pub const BONE: char = '\u{e358}';

    // PS1 effect toggles
    pub const WAVES: char = '\u{e283}';
    pub const MAGNET: char = '\u{e2b5}';
    pub const MONITOR: char = '\u{e11d}';
    pub const SUN: char = '\u{e178}';
    pub const BLEND: char = '\u{e59c}';
    pub const PROPORTIONS: char = '\u{e5cf}';
    pub const ARROW_DOWN_UP: char = '\u{e1c7}';
    pub const PALETTE: char = '\u{e12f}';
    pub const HASH: char = '\u{e0eb}';

    // Music editor
    pub const MUSIC: char = '\u{e122}';
    pub const LIST_MUSIC: char = '\u{e10b}';
    pub const NOTEBOOK_PEN: char = '\u{e596}';

    // Tab bar / nav icons
    pub const HOUSE: char = '\u{e0f5}';
    pub const GLOBE: char = '\u{e0e8}';
    pub const PERSON_STANDING: char = '\u{e21e}';

    // Properties
    pub const FOOTPRINTS: char = '\u{e3b9}';
    pub const MAP_PIN: char = '\u{e111}';
    pub const BOOK_OPEN: char = '\u{e05f}';

    // UV editing / Mirror
    pub const FLIP_HORIZONTAL: char = '\u{e35d}';
    pub const FLIP_VERTICAL: char = '\u{e35f}';
    pub const COLUMNS_2: char = '\u{e085}';
    pub const ROTATE_CW: char = '\u{e149}';
    pub const REFRESH_CW: char = '\u{e145}';
    pub const RATIO: char = '\u{e4e8}';

    // Geometry operations
    pub const UNFOLD_VERTICAL: char = '\u{e1a0}';
    pub const SLASH: char = '\u{e261}';
    pub const DIAMOND: char = '\u{e2d2}';

    // Camera modes
    pub const EYE: char = '\u{e0ba}';
    pub const EYE_OFF: char = '\u{e0bb}';

    // Lock
    pub const LOCK: char = '\u{e109}';
    pub const LOCK_OPEN: char = '\u{e10a}';

    // Checkbox
    pub const SQUARE_CHECK: char = '\u{e16a}';
    pub const CHECK: char = '\u{e06b}';

    // Input
    pub const GAMEPAD_2: char = '\u{e0df}';

    // View / Zoom
    pub const FOCUS: char = '\u{e29e}';
    pub const ZOOM_IN: char = '\u{e1b6}';
    pub const ZOOM_OUT: char = '\u{e1b7}';

    // Close / Cancel
    pub const CIRCLE_X: char = '\u{e084}';
    pub const ARROW_BIG_LEFT: char = '\u{e1e2}';

    // Texture editor drawing tools
    pub const PENCIL: char = '\u{e1f9}';
    pub const ERASER: char = '\u{e28f}';
    pub const PENCIL_LINE: char = '\u{e4f0}';
    pub const RECTANGLE_HORIZONTAL: char = '\u{e376}';
    pub const CIRCLE: char = '\u{e076}';
    pub const DROPLET: char = '\u{e0b4}';
    pub const PIPETTE: char = '\u{e4c6}';
    pub const WAND: char = '\u{e1a8}';
}

/// Create a FontId for the Lucide icon font at a given size.
pub fn font(size: f32) -> egui::FontId {
    egui::FontId::new(size, egui::FontFamily::Name("lucide".into()))
}

/// Convenience: icon RichText at a given size.
pub fn icon_text(ch: char, size: f32) -> egui::RichText {
    egui::RichText::new(ch.to_string()).font(font(size))
}

/// Icon button: returns true if clicked.
pub fn icon_button(ui: &mut egui::Ui, ch: char, size: f32, tooltip: &str) -> bool {
    ui.add(egui::Button::new(icon_text(ch, size)).frame(false))
        .on_hover_text(tooltip)
        .clicked()
}

/// Selectable icon button with active highlight (teal when active, matches v1).
pub fn icon_toggle(ui: &mut egui::Ui, ch: char, size: f32, active: bool, tooltip: &str) -> bool {
    let text = icon_text(ch, size).color(if active {
        egui::Color32::from_rgb(0, 210, 210)  // v1 cyan accent
    } else {
        egui::Color32::from_rgb(140, 140, 155)
    });
    ui.add(egui::Button::new(text).frame(false))
        .on_hover_text(tooltip)
        .clicked()
}
