//! Immediate-mode UI library for the level editor
//!
//! Inspired by TRLE's layout:
//! - Resizable split panels (horizontal/vertical)
//! - 2D grid view, 3D preview, texture palette
//! - Property panels and toolbars
//!
//! Design principles:
//! - Immediate mode (no retained state, rebuilt each frame)
//! - Simple rectangle-based layout
//! - Macroquad integration for rendering

mod rect;
mod panel;
mod widgets;
mod input;
mod tabbar;
mod icons;
mod theme;

pub use rect::*;
pub use panel::*;
pub use widgets::*;
pub use input::*;
pub use tabbar::*;
pub use icons::*;
pub use theme::*;
