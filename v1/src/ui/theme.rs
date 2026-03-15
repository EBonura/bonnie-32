//! UI Theme - Shared colors and styling constants
//!
//! Centralized color definitions for consistent look across all editor panels.

use macroquad::prelude::Color;

// =============================================================================
// Base UI Colors
// =============================================================================

/// Dark background color
pub const BG_COLOR: Color = Color::new(0.11, 0.11, 0.13, 1.0);

/// Header/toolbar background
pub const HEADER_COLOR: Color = Color::new(0.15, 0.15, 0.18, 1.0);

/// Primary text color
pub const TEXT_COLOR: Color = Color::new(0.8, 0.8, 0.85, 1.0);

/// Dimmed/secondary text
pub const TEXT_DIM: Color = Color::new(0.4, 0.4, 0.45, 1.0);

// =============================================================================
// Font Sizes
// =============================================================================

/// Header/title text size
pub const FONT_SIZE_HEADER: f32 = 14.0;

/// Standard content text size
pub const FONT_SIZE_CONTENT: f32 = 12.0;

/// Small/detail text size
pub const FONT_SIZE_SMALL: f32 = 10.0;

// =============================================================================
// Dropdown/Menu Colors
// =============================================================================

/// Dropdown menu background
pub const DROPDOWN_BG: Color = Color::new(0.176, 0.176, 0.196, 1.0); // ~45, 45, 50

/// Dropdown menu border
pub const DROPDOWN_BORDER: Color = Color::new(0.314, 0.314, 0.314, 1.0); // ~80, 80, 80

/// Dropdown item hover background
pub const DROPDOWN_HOVER: Color = Color::new(0.235, 0.314, 0.392, 1.0); // ~60, 80, 100

/// Dropdown trigger background
pub const DROPDOWN_TRIGGER_BG: Color = Color::new(0.196, 0.196, 0.216, 1.0); // ~50, 50, 55

/// Dropdown trigger hover background
pub const DROPDOWN_TRIGGER_HOVER: Color = Color::new(0.235, 0.235, 0.275, 1.0); // ~60, 60, 70

// =============================================================================
// Tracker-specific colors
// =============================================================================

/// Even row background
pub const ROW_EVEN: Color = Color::new(0.13, 0.13, 0.15, 1.0);

/// Odd row background
pub const ROW_ODD: Color = Color::new(0.11, 0.11, 0.13, 1.0);

/// Beat marker row
pub const ROW_BEAT: Color = Color::new(0.16, 0.14, 0.12, 1.0);

/// Highlighted/selected row
pub const ROW_HIGHLIGHT: Color = Color::new(0.2, 0.25, 0.3, 1.0);

/// Cursor highlight
pub const CURSOR_COLOR: Color = Color::new(0.3, 0.5, 0.8, 0.8);

/// Playback position indicator
pub const PLAYBACK_ROW_COLOR: Color = Color::new(0.4, 0.2, 0.2, 0.6);

/// Note column color
pub const NOTE_COLOR: Color = Color::new(0.9, 0.85, 0.5, 1.0);

/// Instrument column color
pub const INST_COLOR: Color = Color::new(0.5, 0.8, 0.5, 1.0);

/// Volume column color
pub const VOL_COLOR: Color = Color::new(0.5, 0.7, 0.9, 1.0);

/// Effect column color
pub const FX_COLOR: Color = Color::new(0.9, 0.5, 0.7, 1.0);
