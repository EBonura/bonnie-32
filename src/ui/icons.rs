//! Lucide icon support
//!
//! Uses the Lucide icon font for crisp vector icons at any size.

use macroquad::prelude::*;

/// Lucide icon codepoints
pub mod icon {
    // File operations
    pub const SAVE: char = '\u{e14d}';
    pub const SAVE_AS: char = '\u{e40f}';  // save-all (Save As)
    pub const FOLDER_OPEN: char = '\u{e247}';
    pub const FILE_PLUS: char = '\u{e0c9}';

    // Edit operations
    pub const UNDO: char = '\u{e19b}';
    pub const REDO: char = '\u{e143}';

    // Playback / Transport
    pub const PLAY: char = '\u{e13c}';
    pub const PAUSE: char = '\u{e131}';
    pub const SQUARE: char = '\u{e167}';      // Stop (also used as shape)
    pub const SKIP_BACK: char = '\u{e15f}';   // Rewind to start
    pub const SKIP_FORWARD: char = '\u{e160}';

    // UI / Navigation
    pub const PLUS: char = '\u{e13d}';
    pub const MINUS: char = '\u{e11c}';
    pub const MOVE: char = '\u{e121}';
    pub const CIRCLE_CHEVRON_LEFT: char = '\u{e4de}';
    pub const CIRCLE_CHEVRON_RIGHT: char = '\u{e4df}';
    pub const CHEVRON_UP: char = '\u{e071}';
    pub const CHEVRON_DOWN: char = '\u{e06e}';

    // Link/Unlink (for vertex mode)
    pub const LINK: char = '\u{e104}';
    pub const UNLINK: char = '\u{e19a}';

    // Editor tools
    pub const BOX: char = '\u{e061}';
    pub const LAYERS: char = '\u{e529}';
    pub const GRID: char = '\u{e0e9}';
    pub const DOOR_CLOSED: char = '\u{e09a}';  // Portal (doorway between rooms)

    // PS1 effect toggles
    pub const WAVES: char = '\u{e283}';       // Affine texture mapping (warpy)
    pub const MAGNET: char = '\u{e2b5}';      // Vertex snapping (jitter)
    pub const MONITOR: char = '\u{e11d}';     // Low resolution mode
    pub const SUN: char = '\u{e178}';         // Lighting/shading
    pub const BLEND: char = '\u{e59c}';       // Dithering (color blending)

    // Music editor
    pub const MUSIC: char = '\u{e122}';       // Music/notes
    pub const PIANO: char = '\u{e2ea}';       // Piano (keyboard icon)
    pub const LIST_MUSIC: char = '\u{e10b}';  // Arrangement/playlist

    // Tab bar icons
    pub const HOUSE: char = '\u{e0f5}';           // Home tab
    pub const GLOBE: char = '\u{e0e8}';           // World tab
    pub const PERSON_STANDING: char = '\u{e21e}'; // Assets tab

    // Properties panel icons
    pub const FOOTPRINTS: char = '\u{e3b9}';      // Walkable surface

    // Browser / Examples
    pub const BOOK_OPEN: char = '\u{e05f}';       // Examples browser
}

/// Draw a Lucide icon centered in a rect
pub fn draw_icon_centered(font: Option<&Font>, icon: char, rect: &super::Rect, size: f32, color: Color) {
    let text = icon.to_string();

    // Icon fonts typically have square glyphs where width ≈ height ≈ font size
    // Use font size directly for more accurate centering
    let icon_size = size;

    // Center horizontally: rect center - half icon width
    let x = rect.x + (rect.w - icon_size) * 0.5;

    // Center vertically: for text, baseline is at bottom, so we need to offset
    // The icon is roughly `size` tall, and baseline is at y position
    // So y = rect.center_y + half_icon_height (since baseline is at bottom of glyph)
    let y = rect.y + (rect.h + icon_size) * 0.5;

    // Round to integer pixels to avoid blurry subpixel rendering
    draw_text_ex(
        &text,
        x.round(),
        y.round(),
        TextParams {
            font,
            font_size: size as u16,
            color,
            ..Default::default()
        },
    );
}
