//! Collision System
//!
//! TR-style cylinder collision against sector-based level geometry.
//! The player is modeled as a vertical cylinder that collides with
//! floor/ceiling heights in each sector.

use crate::rasterizer::Vec3;
use crate::world::Level;
use super::components::{CharacterController, character};

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
    debug_log: bool,
) -> CollisionResult {
    let radius = controller.radius;
    let height = controller.height;
    let step_height = controller.step_height;
    let room_hint = Some(controller.current_room);

    // Proposed new position (horizontal only - vertical handled separately)
    let mut new_pos = position + Vec3::new(velocity.x, 0.0, velocity.z) * delta_time;

    // Apply gravity to vertical velocity (accumulates over time like OpenLara)
    // OpenLara: speed += GRAVITY * 30.0 * deltaTime
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

    // DEBUG: Track collision decision path
    let mut debug_path = String::new();
    let pos_before_center = new_pos;

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
                debug_path.push_str(&format!("STEPUP(diff={:.1}) ", height_diff));
            } else {
                // Wall - push back horizontally
                new_pos.x = position.x;
                new_pos.z = position.z;
                hit_wall = true;
                debug_path.push_str(&format!("WALL(diff={:.1}) ", height_diff));
            }
        } else if foot_y <= info.floor + 1.0 {
            // On the ground (with small tolerance)
            grounded = true;
            new_pos.y = info.floor;
            debug_path.push_str(&format!("GROUND(gap={:.1}) ", foot_y - info.floor));
        } else {
            // Falling - above ground with no contact
            debug_path.push_str(&format!("FALLING(above={:.1}) ", foot_y - info.floor));
        }

        // Ceiling collision
        if head_y > info.ceiling {
            new_pos.y = info.ceiling - height;
            hit_ceiling = true;
            debug_path.push_str("CEILING ");
        }

        // DEBUG: Log center collision details
        if debug_log {
            println!(
                "COL|in:({:.0},{:.0},{:.0})|vel:({:.0},{:.1},{:.0})|vv:{:.1}|dt:{:.4}|prop:({:.0},{:.0},{:.0})|flr:{:.0}|ceil:{:.0}|rm:{}|g:{}|{}",
                position.x, position.y, position.z,
                velocity.x, velocity.y, velocity.z,
                controller.vertical_velocity,
                delta_time,
                pos_before_center.x, pos_before_center.y, pos_before_center.z,
                info.floor, info.ceiling,
                info.room,
                if controller.grounded { "Y" } else { "N" },
                debug_path
            );
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

        if debug_log {
            println!(
                "COL|in:({:.0},{:.0},{:.0})|vel:({:.0},{:.1},{:.0})|vv:{:.1}|dt:{:.4}|prop:({:.0},{:.0},{:.0})|NO_FLOOR_INFO->BLOCKED|rm_hint:{}|g:{}",
                position.x, position.y, position.z,
                velocity.x, velocity.y, velocity.z,
                controller.vertical_velocity,
                delta_time,
                pos_before_center.x, pos_before_center.y, pos_before_center.z,
                controller.current_room,
                if controller.grounded { "Y" } else { "N" }
            );
        }
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
                // Wall collision - push back in the direction of the collision
                let push_x = if corner.x < new_pos.x { radius } else { -radius };
                let push_z = if corner.z < new_pos.z { radius } else { -radius };

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
            let push_x = if corner.x < new_pos.x { radius } else { -radius };
            let push_z = if corner.z < new_pos.z { radius } else { -radius };
            new_pos.x = position.x;
            new_pos.z = position.z;
            hit_wall = true;
        }
    }

    // Get final floor height
    let floor_height = level.get_floor_height(new_pos, Some(current_room))
        .unwrap_or(new_pos.y);

    // DEBUG: Log final result
    if debug_log {
        println!(
            "COL_OUT|out:({:.0},{:.0},{:.0})|flr_h:{:.0}|g:{}|wall:{}|ceil:{}|vv_out:{:.1}",
            new_pos.x, new_pos.y, new_pos.z,
            floor_height,
            if grounded { "Y" } else { "N" },
            if hit_wall { "Y" } else { "N" },
            if hit_ceiling { "Y" } else { "N" },
            vert_vel
        );
    }

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
    debug_log: bool,
) -> Vec3 {
    let result = collide_cylinder(level, position, velocity, controller, delta_time, debug_log);

    // Update controller state
    let prev_grounded = controller.grounded;
    let prev_vv = controller.vertical_velocity;
    controller.grounded = result.grounded;
    controller.current_room = result.room;

    // Update vertical velocity from collision result
    // Reset if grounded or hit ceiling, otherwise use accumulated value
    let vv_action: &str;
    if result.grounded {
        controller.vertical_velocity = 0.0;
        vv_action = "RESET_GND";
    } else if result.hit_ceiling {
        controller.vertical_velocity = 0.0;
        vv_action = "RESET_CEIL";
    } else {
        // Keep accumulated velocity for next frame
        controller.vertical_velocity = result.vertical_velocity;
        vv_action = "KEEP";
    }

    // DEBUG: Log state transition
    if debug_log {
        println!(
            "M&S|g:{}->{} vv:{:.1}->{:.1} act:{} pos:({:.0},{:.0},{:.0})",
            if prev_grounded { "Y" } else { "N" },
            if result.grounded { "Y" } else { "N" },
            prev_vv,
            controller.vertical_velocity,
            vv_action,
            result.position.x, result.position.y, result.position.z
        );
    }

    result.position
}
