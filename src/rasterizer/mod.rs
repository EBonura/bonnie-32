//! PS1-style software rasterizer
//!
//! Ported from tipsy (https://github.com/nkanaev/tipsy)
//!
//! Features:
//! - Affine texture mapping (no perspective correction = PS1 warping)
//! - Vertex snapping (integer coords = PS1 jitter)
//! - Flat and Gouraud shading
//! - Z-buffer or painter's algorithm
//!
//! # Module Organization
//!
//! - `types` - Color, Texture, CLUT, Light, Vertex, Face, RasterSettings
//! - `math` - Vec3, Vec2, projection functions, clipping, geometry utilities
//! - `camera` - Camera struct for 3D rendering
//! - `render` - Framebuffer and mesh rendering functions
//! - `draw` - Drawing utilities (lines, grids, test geometry)
//! - `constants` - Screen resolution constants
//! - `ray` - Ray casting utilities
//! - `fixed` - Fixed-point math

#![allow(dead_code)]

// Sub-modules (exposed for namespaced access)
pub mod camera;
pub mod constants;
pub mod draw;
pub mod fixed;
pub mod math;
pub mod ray;
pub mod render;
pub mod types;

// =============================================================================
// Convenience re-exports for commonly used items
// =============================================================================

// Types - core data structures
pub use types::{
    BlendMode, Clut, ClutDepth, ClutId, Color, Color15,
    Face, IndexedTexture, Light, LightType, OrthoProjection,
    RasterSettings, RasterTimings, ShadingMode,
    Texture, Texture15, Vertex,
};

// Math - vectors, matrices, and projection
pub use math::{
    Vec2, Vec3, Mat4, NEAR_PLANE,
    perspective_transform, project, project_ortho,
    world_to_screen, world_to_screen_with_depth,
    world_to_screen_with_ortho, world_to_screen_with_ortho_depth,
    point_to_segment_distance, point_in_triangle_2d,
    clip_triangle_to_near_plane, ClipResult,
    mat4_identity, mat4_translation, mat4_rotation,
    mat4_mul, mat4_transform_point, mat4_from_position_rotation,
    barycentric, ray_triangle_intersect,
};

// Camera
pub use camera::Camera;

// Render - framebuffer and mesh rendering
pub use render::{Framebuffer, render_mesh, render_mesh_15};

// Draw utilities
pub use draw::{draw_3d_line_clipped, draw_floor_grid, create_test_cube};

// Constants
pub use constants::{WIDTH, HEIGHT, WIDTH_HI, HEIGHT_HI};

// Ray utilities (selective re-export)
pub use ray::{screen_to_ray, ray_line_closest_point, ray_plane_intersection, ray_circle_angle};
