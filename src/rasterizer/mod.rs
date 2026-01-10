//! PS1-style software rasterizer
//! Ported from tipsy (https://github.com/nkanaev/tipsy)
//!
//! Features:
//! - Affine texture mapping (no perspective correction = PS1 warping)
//! - Vertex snapping (integer coords = PS1 jitter)
//! - Flat and Gouraud shading
//! - Z-buffer or painter's algorithm
//!
//! Note: Some utility functions are not yet used but are part of the rendering API.

#![allow(dead_code)]

mod math;
mod types;
mod render;
pub mod ray;
pub mod fixed;

pub use math::*;
pub use types::*;
pub use render::*;
pub use ray::{screen_to_ray, ray_line_closest_point, ray_plane_intersection, ray_circle_angle};
pub use fixed::{fixed_sin, fixed_cos, degrees_to_angle, TRIG_SCALE, TRIG_TABLE_SIZE, SIN_TABLE, COS_TABLE};

/// Screen dimensions (authentic PS1 resolution)
pub const WIDTH: usize = 320;
pub const HEIGHT: usize = 240;

/// High resolution dimensions (2x PS1)
pub const WIDTH_HI: usize = 640;
pub const HEIGHT_HI: usize = 480;
