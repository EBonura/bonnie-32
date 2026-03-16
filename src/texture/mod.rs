//! Texture asset system for PS1-style indexed textures
//!
//! UserTexture combines an indexed texture with its CLUT (palette) into a single
//! self-contained asset. Supports 4-bit (16 colors) and 8-bit (256 colors) modes.

#![allow(dead_code)]

mod user_texture;

pub use user_texture::{UserTexture, TextureSize, TextureSource, TextureError};
