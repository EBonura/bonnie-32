//! Scene/level system — TR1-style room-based levels
//!
//! Clean architecture for PS1-style 3D environments:
//! - Room-based geometry with portal connectivity
//! - Sector grid with floors, ceilings, and walls
//! - Tile-based collision detection

#![allow(dead_code)]

pub mod geometry;
pub mod level;

pub use geometry::*;
pub use level::*;
