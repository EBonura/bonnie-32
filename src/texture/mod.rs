//! Texture asset system for PS1-style indexed textures
//!
//! Provides a unified system for creating, editing, and managing indexed textures
//! that are independent of levels and mesh projects. Textures are stored as `.ron`
//! files with Brotli compression.
//!
//! ## Overview
//!
//! - **UserTexture**: The core texture asset type containing palette indices and a CLUT
//! - **TextureLibrary**: Discovery and caching of user textures from `assets/textures-user/`
//!
//! ## Size Rules
//!
//! - **64x64 textures**: Usable in both World Editor and Mesh Editor
//! - **Other sizes** (8x8, 16x16, 32x32, 128x128, 256x256): Mesh Editor only
//!
//! ## File Format
//!
//! Textures are stored as RON files with Brotli compression (same as levels/meshes).
//! The format includes:
//! - Name and dimensions
//! - CLUT depth (4-bit/16 colors or 8-bit/256 colors)
//! - Palette indices for each pixel
//! - RGB555 color palette

mod user_texture;
mod texture_library;
mod texture_editor;

pub use user_texture::{UserTexture, TextureSize, TextureError};
pub use texture_library::{TextureLibrary, TEXTURES_USER_DIR};
pub use texture_editor::{
    DrawTool, BrushShape, TextureEditorState, TextureUndoEntry,
    draw_texture_canvas, draw_tool_panel, draw_palette_panel,
    screen_to_texture,
};
