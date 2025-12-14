//! World module - TR1-style room-based level system
//!
//! Clean architecture for PS1-style 3D environments:
//! - Room-based geometry with portal connectivity
//! - Visibility culling through portals
//! - Tile-based collision detection
//!
//! Note: Many API items are not yet used by the editor but are part of the
//! intended game runtime API.

#![allow(dead_code)]

mod geometry;
mod level;

pub use geometry::*;
pub use level::*;
