//! Collision System
//!
//! TR-style cylinder collision against sector-based level geometry.
//! The player is modeled as a vertical cylinder that collides with
//! floor/ceiling heights in each sector.
//!
//! Also provides entity-entity overlap tests (sphere-sphere, sphere-AABB)
//! for hitbox/hurtbox combat and trigger volume detection.

use crate::rasterizer::Vec3;
use crate::world::Level;
use super::components::{CharacterController, CollisionShape, character};

/// Result of a collision check
#[derive(Debug, Clone, Copy)]
pub struct CollisionResult {
    /// Corrected position after collision
    pub position: Vec3,
    /// Is the entity on the ground?
    pub grounded: bool,
    /// Current room index
    pub room: usize,
    /// Did we hit a wall? (horizontal collision)
    pub hit_wall: bool,
    /// Did we hit the ceiling?
    pub hit_ceiling: bool,
    /// Floor height at final position
    pub floor_height: f32,
    /// Updated vertical velocity (accumulated gravity)
    pub vertical_velocity: f32,
}

/// Perform cylinder collision against level geometry
///
/// This follows the OpenLara approach:
/// 1. Check floor height at 4 corner points of the cylinder
/// 2. For each point, check if floor is too high (wall) or too low (drop)
/// 3. Apply step-up for small height differences
/// 4. Push back from walls
pub fn collide_cylinder(
    level: &Level,
    position: Vec3,
    velocity: Vec3,
    controller: &CharacterController,
    delta_time: f32,
) -> CollisionResult {
    let radius = controller.radius;
    let height = controller.height;
    let step_height = controller.step_height;
    let room_hint = Some(controller.current_room);

    // Proposed new position (horizontal only - vertical handled separately)
    let mut new_pos = position + Vec3::new(velocity.x, 0.0, velocity.z) * delta_time;

    // Apply gravity to vertical velocity (accumulates over time like OpenLara)
    let gravity = level.player_settings.gravity;
    let mut vert_vel = controller.vertical_velocity;
    if !controller.grounded {
        // Accumulate gravity into velocity
        vert_vel -= gravity * delta_time;
        vert_vel = vert_vel.max(-character::TERMINAL_VELOCITY);
    }
    // Apply accumulated vertical velocity to position
    new_pos.y = position.y + vert_vel * delta_time;

    let mut grounded = false;
    let mut hit_wall = false;
    let mut hit_ceiling = false;
    let mut current_room = controller.current_room;

    // Check floor at center
    if let Some(info) = level.get_floor_info(new_pos, room_hint) {
        current_room = info.room;

        // Floor collision
        let foot_y = new_pos.y; // Character position is at feet
        let head_y = new_pos.y + height;

        // Check if we're below the floor
        if foot_y < info.floor {
            // Check if it's a steppable height difference
            let height_diff = info.floor - foot_y;
            if height_diff <= step_height {
                // Step up
                new_pos.y = info.floor;
                grounded = true;
            } else {
                // Wall - push back horizontally
                new_pos.x = position.x;
                new_pos.z = position.z;
                hit_wall = true;
            }
        } else if foot_y <= info.floor + 1.0 {
            // On the ground (with small tolerance)
            grounded = true;
            new_pos.y = info.floor;
        }
        // else: Falling - above ground with no contact

        // Ceiling collision
        if head_y > info.ceiling {
            new_pos.y = info.ceiling - height;
            hit_ceiling = true;
        }
    } else {
        // Outside all rooms - treat as wall collision (like OpenLara)
        // Revert to original position entirely to prevent falling into void
        new_pos = position;
        hit_wall = true;
        // Preserve grounded state from previous frame to prevent gravity accumulation
        grounded = controller.grounded;
        // Reset vertical velocity to prevent fall-through
        vert_vel = 0.0;
    }

    // Check 4 corner points for wall collision (like OpenLara)
    let corners = [
        Vec3::new(new_pos.x - radius, new_pos.y, new_pos.z - radius),
        Vec3::new(new_pos.x + radius, new_pos.y, new_pos.z - radius),
        Vec3::new(new_pos.x + radius, new_pos.y, new_pos.z + radius),
        Vec3::new(new_pos.x - radius, new_pos.y, new_pos.z + radius),
    ];

    for corner in &corners {
        if let Some(info) = level.get_floor_info(*corner, Some(current_room)) {
            // If corner's floor is significantly higher than our position, it's a wall
            let height_diff = info.floor - new_pos.y;
            if height_diff > step_height {
                // Only push back the axis that's blocked
                let corner_x_only = Vec3::new(corner.x, new_pos.y, new_pos.z);
                let corner_z_only = Vec3::new(new_pos.x, new_pos.y, corner.z);

                if let Some(info_x) = level.get_floor_info(corner_x_only, Some(current_room)) {
                    if info_x.floor - new_pos.y > step_height {
                        new_pos.x = position.x;
                        hit_wall = true;
                    }
                }

                if let Some(info_z) = level.get_floor_info(corner_z_only, Some(current_room)) {
                    if info_z.floor - new_pos.y > step_height {
                        new_pos.z = position.z;
                        hit_wall = true;
                    }
                }
            }
        } else {
            // Corner is outside rooms - treat as wall
            new_pos.x = position.x;
            new_pos.z = position.z;
            hit_wall = true;
        }
    }

    // Get final floor height
    let floor_height = level.get_floor_height(new_pos, Some(current_room))
        .unwrap_or(new_pos.y);

    CollisionResult {
        position: new_pos,
        grounded,
        room: current_room,
        hit_wall,
        hit_ceiling,
        floor_height,
        vertical_velocity: vert_vel,
    }
}

/// Simple move-and-slide collision for entities
///
/// Moves the entity by velocity, sliding along walls if blocked.
pub fn move_and_slide(
    level: &Level,
    position: Vec3,
    velocity: Vec3,
    controller: &mut CharacterController,
    delta_time: f32,
) -> Vec3 {
    let result = collide_cylinder(level, position, velocity, controller, delta_time);

    // Update controller state
    controller.grounded = result.grounded;
    controller.current_room = result.room;

    // Update vertical velocity from collision result
    // Reset if grounded or hit ceiling, otherwise use accumulated value
    if result.grounded || result.hit_ceiling {
        controller.vertical_velocity = 0.0;
    } else {
        // Keep accumulated velocity for next frame
        controller.vertical_velocity = result.vertical_velocity;
    }

    result.position
}

// =============================================================================
// Entity-Entity Overlap Tests
// =============================================================================

/// Test if two collision shapes overlap, given their world-space positions.
/// Returns the approximate overlap point if they intersect.
pub fn shapes_overlap(
    pos_a: Vec3, shape_a: &CollisionShape,
    pos_b: Vec3, shape_b: &CollisionShape,
) -> Option<Vec3> {
    match (shape_a, shape_b) {
        (CollisionShape::Sphere { radius: ra }, CollisionShape::Sphere { radius: rb }) => {
            sphere_sphere(pos_a, *ra, pos_b, *rb)
        }
        (CollisionShape::Sphere { radius }, CollisionShape::Box { half_extents }) => {
            sphere_aabb(pos_a, *radius, pos_b, *half_extents)
        }
        (CollisionShape::Box { half_extents }, CollisionShape::Sphere { radius }) => {
            sphere_aabb(pos_b, *radius, pos_a, *half_extents)
        }
        (CollisionShape::Box { half_extents: ha }, CollisionShape::Box { half_extents: hb }) => {
            aabb_aabb(pos_a, *ha, pos_b, *hb)
        }
        (CollisionShape::Sphere { radius }, CollisionShape::Capsule { radius: cr, height }) => {
            sphere_capsule(pos_a, *radius, pos_b, *cr, *height)
        }
        (CollisionShape::Capsule { radius: cr, height }, CollisionShape::Sphere { radius }) => {
            sphere_capsule(pos_b, *radius, pos_a, *cr, *height)
        }
        // Capsule-capsule and capsule-box: approximate as sphere for simplicity
        (CollisionShape::Capsule { radius: ra, height: ha }, CollisionShape::Capsule { radius: rb, height: hb }) => {
            // Treat each capsule as a sphere centered at its midpoint
            let mid_a = Vec3::new(pos_a.x, pos_a.y + ha * 0.5, pos_a.z);
            let mid_b = Vec3::new(pos_b.x, pos_b.y + hb * 0.5, pos_b.z);
            sphere_sphere(mid_a, ra + ha * 0.5, mid_b, rb + hb * 0.5)
        }
        (CollisionShape::Capsule { radius, height }, CollisionShape::Box { half_extents }) => {
            let mid = Vec3::new(pos_a.x, pos_a.y + height * 0.5, pos_a.z);
            sphere_aabb(mid, radius + height * 0.5, pos_b, *half_extents)
        }
        (CollisionShape::Box { half_extents }, CollisionShape::Capsule { radius, height }) => {
            let mid = Vec3::new(pos_b.x, pos_b.y + height * 0.5, pos_b.z);
            sphere_aabb(mid, radius + height * 0.5, pos_a, *half_extents)
        }
    }
}

/// Test if a point is inside a collision shape at the given position
pub fn point_in_shape(point: Vec3, shape_pos: Vec3, shape: &CollisionShape) -> bool {
    match shape {
        CollisionShape::Sphere { radius } => {
            let d = point - shape_pos;
            d.dot(d) <= radius * radius
        }
        CollisionShape::Box { half_extents } => {
            let d = point - shape_pos;
            d.x.abs() <= half_extents.x && d.y.abs() <= half_extents.y && d.z.abs() <= half_extents.z
        }
        CollisionShape::Capsule { radius, height } => {
            // Capsule: cylinder from pos.y to pos.y+height with hemispherical caps
            let dy = point.y - shape_pos.y;
            let clamped_y = dy.clamp(0.0, *height);
            let closest = Vec3::new(shape_pos.x, shape_pos.y + clamped_y, shape_pos.z);
            let d = point - closest;
            d.dot(d) <= radius * radius
        }
    }
}

// --- Primitive overlap tests ---

fn sphere_sphere(pos_a: Vec3, ra: f32, pos_b: Vec3, rb: f32) -> Option<Vec3> {
    let diff = pos_b - pos_a;
    let dist_sq = diff.dot(diff);
    let combined = ra + rb;
    if dist_sq <= combined * combined {
        // Midpoint between the two centers as approximate contact point
        Some(pos_a + diff * 0.5)
    } else {
        None
    }
}

fn sphere_aabb(sphere_pos: Vec3, radius: f32, box_pos: Vec3, half: Vec3) -> Option<Vec3> {
    // Find the closest point on the AABB to the sphere center
    let clamped = Vec3::new(
        (sphere_pos.x - box_pos.x).clamp(-half.x, half.x) + box_pos.x,
        (sphere_pos.y - box_pos.y).clamp(-half.y, half.y) + box_pos.y,
        (sphere_pos.z - box_pos.z).clamp(-half.z, half.z) + box_pos.z,
    );
    let diff = sphere_pos - clamped;
    if diff.dot(diff) <= radius * radius {
        Some(clamped)
    } else {
        None
    }
}

fn aabb_aabb(pos_a: Vec3, ha: Vec3, pos_b: Vec3, hb: Vec3) -> Option<Vec3> {
    let dx = (pos_a.x - pos_b.x).abs();
    let dy = (pos_a.y - pos_b.y).abs();
    let dz = (pos_a.z - pos_b.z).abs();
    if dx <= ha.x + hb.x && dy <= ha.y + hb.y && dz <= ha.z + hb.z {
        Some((pos_a + pos_b) * 0.5)
    } else {
        None
    }
}

fn sphere_capsule(
    sphere_pos: Vec3, sphere_r: f32,
    cap_pos: Vec3, cap_r: f32, cap_h: f32,
) -> Option<Vec3> {
    // Find closest point on the capsule's line segment to the sphere center
    let dy = sphere_pos.y - cap_pos.y;
    let clamped_y = dy.clamp(0.0, cap_h);
    let closest = Vec3::new(cap_pos.x, cap_pos.y + clamped_y, cap_pos.z);
    sphere_sphere(sphere_pos, sphere_r, closest, cap_r)
}
