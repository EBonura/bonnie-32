//! Level Editor
//!
//! TRLE-inspired layout:
//! - 2D grid view (top-down room editing)
//! - 3D viewport (software rendered preview)
//! - Texture palette
//! - Properties panel
//!
//! Note: Some editor state/selection API is not yet fully used.

#![allow(dead_code)]

mod state;
mod layout;
mod grid_view;
mod viewport_3d;
mod texture_palette;
mod texture_pack;
mod sample_levels;
mod level_browser;
pub mod actions;

pub use state::*;
pub use layout::*;
pub use texture_pack::TexturePack;
pub use sample_levels::*;
pub use level_browser::*;
// Actions used internally by layout.rs
