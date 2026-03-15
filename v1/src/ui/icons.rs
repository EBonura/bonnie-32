//! Lucide icon support
//!
//! Uses the Lucide icon font for crisp vector icons at any size.

use macroquad::prelude::*;

/// Lucide icon codepoints
/// Note: Not all icons are currently used - this is a library of available icons
#[allow(dead_code)]
pub mod icon {
    // File operations
    pub const SAVE: char = '\u{e14d}';
    pub const SAVE_AS: char = '\u{e40f}';  // save-all (Save As)
    pub const FOLDER_OPEN: char = '\u{e247}';
    pub const FILE_PLUS: char = '\u{e0c9}';
    pub const DOWNLOAD: char = '\u{e099}';   // download

    // Edit operations
    pub const UNDO: char = '\u{e19b}';
    pub const REDO: char = '\u{e143}';

    // Playback / Transport
    pub const PLAY: char = '\u{e13c}';
    pub const PAUSE: char = '\u{e12e}';
    pub const SQUARE: char = '\u{e167}';      // Stop (also used as shape)
    pub const SKIP_BACK: char = '\u{e15f}';   // Rewind to start
    pub const SKIP_FORWARD: char = '\u{e160}';

    // UI / Navigation
    pub const PLUS: char = '\u{e13d}';
    pub const MINUS: char = '\u{e11c}';
    pub const TRASH: char = '\u{e18d}';
    pub const MOVE: char = '\u{e121}';
    pub const CIRCLE_CHEVRON_LEFT: char = '\u{e4de}';
    pub const CIRCLE_CHEVRON_RIGHT: char = '\u{e4df}';
    pub const CHEVRON_UP: char = '\u{e070}';
    pub const CHEVRON_DOWN: char = '\u{e06d}';
    pub const CHEVRON_LEFT: char = '\u{e06e}';
    pub const CHEVRON_RIGHT: char = '\u{e06f}';

    // Link/Unlink (for vertex mode)
    pub const LINK: char = '\u{e103}';      // link-2
    pub const LINK_OFF: char = '\u{e104}';  // link-2-off

    // Editor tools
    pub const BOX: char = '\u{e061}';
    pub const BRICK_WALL: char = '\u{e581}';      // brick-wall (wall tool)
    pub const LAYERS: char = '\u{e529}';
    pub const GRID: char = '\u{e0e9}';
    pub const DOOR_CLOSED: char = '\u{e09a}';  // Portal (doorway between rooms)

    // Transform tools (Assets editor)
    pub const POINTER: char = '\u{e1e8}';      // Select tool
    pub const ROTATE_3D: char = '\u{e2ea}';    // Rotate tool
    pub const SCALE_3D: char = '\u{e2eb}';     // Scale tool
    pub const MAXIMIZE_2: char = '\u{e113}';   // UV editor (expand/maximize)
    pub const BRUSH: char = '\u{e1d3}';        // Paint mode
    pub const PAINT_BUCKET: char = '\u{e2e6}'; // Fill tool (paint-bucket)
    pub const GIT_BRANCH: char = '\u{e1f4}';   // Hierarchy
    pub const SCAN: char = '\u{e257}';         // Face selection
    pub const CIRCLE_DOT: char = '\u{e345}';   // Vertex selection
    pub const BONE: char = '\u{e358}';         // Bone selection

    // PS1 effect toggles
    pub const WAVES: char = '\u{e283}';       // Affine texture mapping (warpy)
    pub const MAGNET: char = '\u{e2b5}';      // Vertex snapping (jitter)
    pub const MONITOR: char = '\u{e11d}';     // Low resolution mode
    pub const SUN: char = '\u{e178}';         // Lighting/shading
    pub const BLEND: char = '\u{e59c}';       // Dithering (color blending)
    pub const PROPORTIONS: char = '\u{e5cf}'; // Aspect ratio toggle (4:3 vs stretch)
    pub const ARROW_DOWN_UP: char = '\u{e1c7}'; // Z-buffer / depth sorting (arrow-down-up)
    pub const PALETTE: char = '\u{e12f}';     // RGB555 color mode (15-bit PS1 color)
    pub const HASH: char = '\u{e0eb}';        // Fixed-point math (precision/jitter)

    // Music editor
    pub const MUSIC: char = '\u{e122}';       // Music/notes
    pub const PIANO: char = '\u{e2ea}';       // Piano (keyboard icon)
    pub const LIST_MUSIC: char = '\u{e10b}';  // Arrangement/playlist
    pub const NOTEBOOK_PEN: char = '\u{e596}'; // Arrangement (notebook with pen)

    // Tab bar icons
    pub const HOUSE: char = '\u{e0f5}';           // Home tab
    pub const GLOBE: char = '\u{e0e8}';           // World tab
    pub const PERSON_STANDING: char = '\u{e21e}'; // Assets tab

    // Properties panel icons
    pub const FOOTPRINTS: char = '\u{e3b9}';      // Walkable surface

    // Browser / Examples
    pub const BOOK_OPEN: char = '\u{e05f}';       // Examples browser

    // Level objects
    pub const MAP_PIN: char = '\u{e111}';         // Object placement (map-pin)

    // UV editing / Mirror
    pub const FLIP_HORIZONTAL: char = '\u{e35d}'; // flip-horizontal
    pub const FLIP_VERTICAL: char = '\u{e35f}';   // flip-vertical
    pub const COLUMNS_2: char = '\u{e085}';       // columns-2 (mirror editing)
    pub const ROTATE_CW: char = '\u{e149}';       // rotate-cw
    pub const REFRESH_CW: char = '\u{e145}';      // refresh-cw (reset)
    pub const RATIO: char = '\u{e4e8}';           // ratio (1:1 texel mapping)

    // Geometry operations
    pub const UNFOLD_VERTICAL: char = '\u{e1a0}'; // unfold-vertical (extrude)
    pub const SLASH: char = '\u{e261}';           // slash
    pub const DIAMOND: char = '\u{e2d2}';         // diamond (diagonal wall)

    // Camera modes
    pub const EYE: char = '\u{e0ba}';             // Free camera (eye)
    pub const EYE_OFF: char = '\u{e0bb}';         // Hidden (eye-off)
    pub const ORBIT: char = '\u{e12e}';           // Orbit camera (orbit icon)

    // Lock icons
    pub const LOCK: char = '\u{e109}';            // Locked
    pub const LOCK_OPEN: char = '\u{e10a}';       // Unlocked

    // Checkbox icons
    pub const SQUARE_CHECK: char = '\u{e16a}';    // square-check (checked checkbox)
    pub const CHECK: char = '\u{e06b}';           // check (checkmark)

    // Input / Controller
    pub const GAMEPAD_2: char = '\u{e0df}';       // Gamepad controller

    // View / Focus / Zoom
    pub const FOCUS: char = '\u{e29e}';           // Focus / crosshair target
    pub const SQUARE_SQUARE: char = '\u{e60e}';   // Nested squares / center on selection
    pub const ZOOM_IN: char = '\u{e1b6}';         // zoom-in
    pub const ZOOM_OUT: char = '\u{e1b7}';        // zoom-out

    // Close / Cancel / Back
    pub const CIRCLE_X: char = '\u{e084}';        // Circle with X (close button)
    pub const ARROW_BIG_LEFT: char = '\u{e1e2}';  // arrow-big-left (back button)

    // Drawing tools (texture editor)
    pub const PENCIL: char = '\u{e1f9}';          // pencil (single pixel / small brush)
    pub const ERASER: char = '\u{e28f}';          // eraser
    pub const PENCIL_LINE: char = '\u{e4f0}';     // pencil-line (line tool)
    pub const RECTANGLE_HORIZONTAL: char = '\u{e376}'; // rectangle-horizontal
    pub const CIRCLE: char = '\u{e076}';          // circle (ellipse shape)
    pub const DROPLET: char = '\u{e0b4}';         // droplet (fill toggle)
    pub const PIPETTE: char = '\u{e4c6}';         // pipette (eyedropper/color picker)
    pub const WAND: char = '\u{e1a8}';            // wand-2 (magic select by color)
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
