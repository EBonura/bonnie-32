//! Blob Shadow System
//!
//! PS1-authentic blob shadows — dark ellipses projected beneath entities onto
//! floor geometry. Tomb Raider, Crash Bandicoot, and Spyro all used this technique.
//!
//! The shadow is rendered as a projected circle at floor height, darkening
//! whatever is beneath via subtractive blending. Shadow size and opacity scale
//! with distance from entity to floor (closer = smaller/darker, farther = larger/fainter).

use crate::rasterizer::{
    Vec3, Color, Framebuffer, Camera,
    perspective_transform, project,
};
use crate::world::Level;
use super::entity::Entity;
use super::World;

/// Maximum distance from entity feet to floor for shadow to be visible
const MAX_SHADOW_HEIGHT: f32 = 1500.0;

/// Base shadow radius when entity is on the ground
const BASE_SHADOW_RADIUS: f32 = 80.0;

/// Maximum shadow radius (when high up)
const MAX_SHADOW_RADIUS: f32 = 200.0;

/// Shadow darkness when entity is on the ground (0-255, higher = darker)
const MAX_SHADOW_ALPHA: u8 = 120;

/// Minimum shadow darkness (when far from floor)
const MIN_SHADOW_ALPHA: u8 = 30;

/// Shadow color (dark, slightly warm like PS1 games)
const SHADOW_COLOR: Color = Color { r: 0, g: 0, b: 0, blend: crate::rasterizer::BlendMode::Opaque };

/// Number of segments for the projected shadow circle
const SHADOW_SEGMENTS: usize = 8;

/// Render blob shadows for all entities with transforms.
///
/// For each entity:
/// 1. Raycast down to find floor height
/// 2. Calculate shadow size/opacity based on distance
/// 3. Project a circle of world-space points at floor height
/// 4. Rasterize the projected ellipse as a filled polygon with alpha
pub fn render_blob_shadows(
    fb: &mut Framebuffer,
    camera: &Camera,
    world: &World,
    level: &Level,
) {
    for (idx, transform) in world.transforms.iter() {
        let entity = Entity::new(idx, 0);
        let pos = transform.position;

        // Use room hint from controller if available, otherwise None
        let room_hint = world.controllers.get(entity).map(|c| c.current_room);

        // Get floor height below entity
        let floor_y = match level.get_floor_height(pos, room_hint) {
            Some(h) => h,
            None => continue,
        };

        // Distance from entity feet to floor
        let height_above_floor = pos.y - floor_y;

        // Skip if too high or below floor
        if height_above_floor < 0.0 || height_above_floor > MAX_SHADOW_HEIGHT {
            continue;
        }

        // Interpolation factor: 0 = on floor, 1 = at max height
        let t = (height_above_floor / MAX_SHADOW_HEIGHT).min(1.0);

        // Shadow radius grows with height (perspective faking)
        let shadow_radius = BASE_SHADOW_RADIUS + (MAX_SHADOW_RADIUS - BASE_SHADOW_RADIUS) * t;

        // Shadow gets fainter with height
        let alpha = MAX_SHADOW_ALPHA - ((MAX_SHADOW_ALPHA - MIN_SHADOW_ALPHA) as f32 * t) as u8;

        // Shadow center at floor level directly below entity
        let shadow_center = Vec3::new(pos.x, floor_y + 0.5, pos.z); // Slight offset above floor to avoid z-fighting

        render_shadow_ellipse(fb, camera, shadow_center, shadow_radius, alpha);
    }
}

/// Render a single shadow ellipse as a projected circle with scanline fill.
///
/// Projects circle vertices to screen space, then fills the resulting ellipse
/// using scanline rasterization with alpha darkening.
fn render_shadow_ellipse(
    fb: &mut Framebuffer,
    camera: &Camera,
    center: Vec3,
    radius: f32,
    alpha: u8,
) {
    // Generate world-space circle points
    let mut screen_points: Vec<(i32, i32)> = Vec::with_capacity(SHADOW_SEGMENTS);

    for i in 0..SHADOW_SEGMENTS {
        let angle = (i as f32 / SHADOW_SEGMENTS as f32) * std::f32::consts::TAU;
        let world_pt = Vec3::new(
            center.x + radius * angle.cos(),
            center.y,
            center.z + radius * angle.sin(),
        );

        // Project to screen
        let rel = world_pt - camera.position;
        let cam_space = perspective_transform(rel, camera.basis_x, camera.basis_y, camera.basis_z);

        if cam_space.z < 0.1 {
            return; // Shadow is behind camera — skip entirely
        }

        let screen = project(cam_space, fb.width, fb.height);
        screen_points.push((screen.x as i32, screen.y as i32));
    }

    if screen_points.len() < 3 {
        return;
    }

    // Project center to screen (once, outside the loop)
    let center_rel = center - camera.position;
    let center_cam = perspective_transform(center_rel, camera.basis_x, camera.basis_y, camera.basis_z);
    if center_cam.z < 0.1 {
        return;
    }
    let center_screen = project(center_cam, fb.width, fb.height);

    // Compute screen-space radius (once)
    let screen_radius_x = screen_points.iter()
        .map(|p| (p.0 as f32 - center_screen.x).abs())
        .fold(0.0_f32, f32::max);
    let screen_radius_y = screen_points.iter()
        .map(|p| (p.1 as f32 - center_screen.y).abs())
        .fold(0.0_f32, f32::max);
    let screen_radius = screen_radius_x.max(screen_radius_y).max(1.0);

    // Find screen bounding box
    let min_y = screen_points.iter().map(|p| p.1).min().unwrap_or(0).max(0);
    let max_y = screen_points.iter().map(|p| p.1).max().unwrap_or(0).min(fb.height as i32 - 1);

    // Scanline fill the projected polygon with alpha darkening
    let n = screen_points.len();
    for y in min_y..=max_y {
        // Find all X intersections at this scanline
        let mut x_intersections: Vec<i32> = Vec::new();

        for i in 0..n {
            let (x0, y0) = screen_points[i];
            let (x1, y1) = screen_points[(i + 1) % n];

            // Check if this edge crosses the scanline
            if (y0 <= y && y1 > y) || (y1 <= y && y0 > y) {
                let t = (y - y0) as f32 / (y1 - y0) as f32;
                let x = x0 as f32 + t * (x1 - x0) as f32;
                x_intersections.push(x as i32);
            }
        }

        x_intersections.sort();

        // Fill between pairs of intersections
        for pair in x_intersections.chunks(2) {
            if pair.len() == 2 {
                let x_start = pair[0].max(0) as usize;
                let x_end = (pair[1]).min(fb.width as i32 - 1) as usize;

                for x in x_start..=x_end {
                    let dx = x as f32 - center_screen.x;
                    let dy = y as f32 - center_screen.y;
                    let dist = (dx * dx + dy * dy).sqrt();
                    let normalized_dist = (dist / screen_radius).min(1.0);

                    // Soft circular falloff (quadratic for PS1-like soft edge)
                    let falloff = 1.0 - normalized_dist * normalized_dist;
                    let pixel_alpha = (alpha as f32 * falloff) as u8;

                    if pixel_alpha > 2 {
                        fb.set_pixel_alpha(x, y as usize, SHADOW_COLOR, pixel_alpha);
                    }
                }
            }
        }
    }
}
