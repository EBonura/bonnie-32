//! Skeleton/Bone visualization for the modeler
//!
//! Renders bones as Blender-style octahedrons with hierarchy connections.
//! TR-style bones: fixed offsets define topology, keyframes animate mesh rotations.

use crate::rasterizer::{
    Framebuffer, Vec3, Color as RasterColor, Camera, OrthoProjection,
    world_to_screen_with_ortho, world_to_screen_with_ortho_depth, draw_3d_line_clipped,
};
use super::state::{ModelerState, RigBone, rotate_by_euler};

/// Get bone color based on state
fn bone_color_default() -> RasterColor {
    RasterColor::new(200, 200, 200) // Light gray
}

fn bone_color_selected() -> RasterColor {
    RasterColor::new(80, 255, 80) // Green
}

fn bone_color_hovered() -> RasterColor {
    RasterColor::new(100, 180, 255) // Light blue
}

fn bone_color_tip_hovered() -> RasterColor {
    RasterColor::new(255, 180, 80) // Orange for tip hover
}

fn bone_color_root() -> RasterColor {
    RasterColor::new(255, 220, 100) // Yellow
}

fn bone_color_creating() -> RasterColor {
    RasterColor::new(255, 150, 50) // Orange
}

fn bone_color_hierarchy_line() -> RasterColor {
    RasterColor::new(100, 100, 100) // Dark gray
}

/// Draw all bones in the skeleton
pub fn draw_skeleton(
    fb: &mut Framebuffer,
    state: &ModelerState,
    ortho: Option<&OrthoProjection>,
) {
    let skeleton = state.skeleton();

    // Skip if no bones or bones hidden
    if skeleton.is_empty() && state.bone_creation.is_none() {
        return;
    }

    if !state.show_bones {
        return;
    }

    // Get Skeleton component opacity (0=visible, 7=hidden)
    let skeleton_info = state.asset.components.iter()
        .enumerate()
        .find(|(_, c)| matches!(c, crate::asset::AssetComponent::Skeleton { .. }));
    let skeleton_hidden = skeleton_info
        .map(|(idx, _)| state.is_component_hidden(idx))
        .unwrap_or(false);
    if skeleton_hidden {
        return;
    }
    let camera = &state.camera;

    // Draw hierarchy lines at full color (bone octahedrons handle transparency via editor_alpha)
    let line_color = bone_color_hierarchy_line();
    for (idx, bone) in skeleton.iter().enumerate() {
        if let Some(parent_idx) = bone.parent {
            let (child_pos, _) = state.get_bone_world_transform(idx);
            let (parent_pos, _) = state.get_bone_world_transform(parent_idx);

            draw_3d_line_clipped(fb, camera, parent_pos, child_pos, line_color);
        }
    }
}

/// Draw bone tip and base dots as overlays on all bones
/// Tips get a small dot, hovered tips get a larger highlighted dot
pub fn draw_bone_dots(
    fb: &mut Framebuffer,
    state: &ModelerState,
    ortho: Option<&OrthoProjection>,
) {
    let skeleton = state.skeleton();
    if skeleton.is_empty() || !state.show_bones {
        return;
    }

    // Check if skeleton component is hidden
    let skeleton_hidden = state.asset.components.iter()
        .enumerate()
        .find(|(_, c)| matches!(c, crate::asset::AssetComponent::Skeleton { .. }))
        .map(|(idx, _)| state.is_component_hidden(idx))
        .unwrap_or(false);
    if skeleton_hidden {
        return;
    }

    let camera = &state.camera;
    let tip_color = RasterColor::new(220, 220, 230); // Subtle light dot
    let tip_selected_color = RasterColor::new(80, 255, 80); // Green (matches selected bone)
    let tip_hovered_color = RasterColor::new(255, 180, 80); // Orange (matches tip hover)
    let base_color = RasterColor::new(150, 150, 160); // Dimmer dot for base

    for (idx, _bone) in skeleton.iter().enumerate() {
        let (base_pos, _) = state.get_bone_world_transform(idx);
        let tip_pos = state.get_bone_tip_position(idx);

        // Draw tip dot
        if let Some((tx, ty)) = world_to_screen_with_ortho(
            tip_pos, camera.position, camera.basis_x, camera.basis_y, camera.basis_z,
            fb.width, fb.height, ortho,
        ) {
            let (radius, color) = if state.hovered_bone_tip == Some(idx) {
                (11, tip_hovered_color) // Large orange dot when hovered
            } else if state.selected_bone == Some(idx) {
                (8, tip_selected_color) // Green dot on selected bone's tip
            } else {
                (5, tip_color) // Visible dot
            };
            fb.draw_circle(tx as i32, ty as i32, radius, color);
        }

        // Draw base dot (smaller, subtler)
        if let Some((bx, by)) = world_to_screen_with_ortho(
            base_pos, camera.position, camera.basis_x, camera.basis_y, camera.basis_z,
            fb.width, fb.height, ortho,
        ) {
            let (radius, color) = if state.hovered_bone == Some(idx) {
                (11, bone_color_hovered()) // Large blue dot when body/base hovered
            } else if state.selected_bone == Some(idx) {
                (8, tip_selected_color) // Green dot on selected bone's base
            } else {
                (5, base_color) // Visible dot
            };
            fb.draw_circle(bx as i32, by as i32, radius, color);
        }
    }
}

/// Draw a single bone as a Blender-style octahedron
///
/// The octahedron has:
/// - Base vertex at the bone's origin
/// - Tip vertex at the bone's endpoint
/// - 4 vertices in a ring at 20% along the bone (the "width" of the bone)
fn draw_bone_octahedron(
    fb: &mut Framebuffer,
    camera: &Camera,
    ortho: Option<&OrthoProjection>,
    base: Vec3,
    tip: Vec3,
    color: RasterColor,
) {
    // Calculate bone direction and length
    let direction = tip - base;
    let length = direction.len();
    if length < 0.001 {
        return; // Degenerate bone
    }

    let dir_norm = direction * (1.0 / length);

    // Find perpendicular axes for the bone's local space
    let (perp1, perp2) = compute_perpendicular_axes(dir_norm);

    // Width of the bone (at the widest point)
    let width = RigBone::DEFAULT_WIDTH;

    // Position of the ring (20% along the bone from base)
    let ring_center = base + dir_norm * (length * 0.2);

    // 4 ring vertices
    let ring = [
        ring_center + perp1 * width,
        ring_center + perp2 * width,
        ring_center - perp1 * width,
        ring_center - perp2 * width,
    ];

    // Project all vertices to screen space
    let project = |p: Vec3| -> Option<(i32, i32, f32)> {
        world_to_screen_with_ortho_depth(
            p,
            camera.position,
            camera.basis_x,
            camera.basis_y,
            camera.basis_z,
            fb.width,
            fb.height,
            ortho,
        ).map(|(x, y, z)| (x as i32, y as i32, z))
    };

    let base_s = project(base);
    let tip_s = project(tip);
    let ring_s: [Option<(i32, i32, f32)>; 4] = [
        project(ring[0]),
        project(ring[1]),
        project(ring[2]),
        project(ring[3]),
    ];

    // Draw filled faces
    // Base pyramid (base to ring)
    for i in 0..4 {
        let next = (i + 1) % 4;
        if let (Some(b), Some(r0), Some(r1)) = (base_s, ring_s[i], ring_s[next]) {
            draw_filled_triangle_3d(fb, b, r0, r1, color);
        }
    }

    // Tip pyramid (ring to tip)
    for i in 0..4 {
        let next = (i + 1) % 4;
        if let (Some(t), Some(r0), Some(r1)) = (tip_s, ring_s[i], ring_s[next]) {
            draw_filled_triangle_3d(fb, t, r1, r0, color);
        }
    }

    // Draw edges for definition (slightly darker)
    let edge_color = RasterColor::new(
        (color.r as u16 * 2 / 3) as u8,
        (color.g as u16 * 2 / 3) as u8,
        (color.b as u16 * 2 / 3) as u8,
    );

    // Edges from base to ring
    for r in &ring_s {
        if let (Some((bx, by, bz)), Some((rx, ry, rz))) = (base_s, *r) {
            fb.draw_line_3d(bx, by, bz, rx, ry, rz, edge_color);
        }
    }

    // Edges from ring to tip
    for r in &ring_s {
        if let (Some((tx, ty, tz)), Some((rx, ry, rz))) = (tip_s, *r) {
            fb.draw_line_3d(tx, ty, tz, rx, ry, rz, edge_color);
        }
    }

    // Ring edges
    for i in 0..4 {
        let next = (i + 1) % 4;
        if let (Some((x0, y0, z0)), Some((x1, y1, z1))) = (ring_s[i], ring_s[next]) {
            fb.draw_line_3d(x0, y0, z0, x1, y1, z1, edge_color);
        }
    }
}

/// Compute two perpendicular axes to a given direction vector
fn compute_perpendicular_axes(dir: Vec3) -> (Vec3, Vec3) {
    // Find a vector not parallel to dir
    let up = if dir.y.abs() < 0.9 {
        Vec3::new(0.0, 1.0, 0.0)
    } else {
        Vec3::new(1.0, 0.0, 0.0)
    };

    // Cross products to get perpendicular axes
    let perp1 = cross(dir, up).normalize();
    let perp2 = cross(dir, perp1).normalize();

    (perp1, perp2)
}

/// Cross product of two Vec3
fn cross(a: Vec3, b: Vec3) -> Vec3 {
    Vec3::new(
        a.y * b.z - a.z * b.y,
        a.z * b.x - a.x * b.z,
        a.x * b.y - a.y * b.x,
    )
}

/// Draw a filled triangle (for bone faces)
/// Draws on top of existing content (like gizmos)
fn draw_filled_triangle_3d(
    fb: &mut Framebuffer,
    p0: (i32, i32, f32),
    p1: (i32, i32, f32),
    p2: (i32, i32, f32),
    color: RasterColor,
) {
    // Sort vertices by y coordinate (ignore z for now - draw on top like gizmos)
    let mut pts = [(p0.0, p0.1), (p1.0, p1.1), (p2.0, p2.1)];
    pts.sort_by(|a, b| a.1.cmp(&b.1));
    let (x0, y0) = pts[0];
    let (x1, y1) = pts[1];
    let (x2, y2) = pts[2];

    if y2 == y0 {
        return; // Degenerate triangle
    }

    let total_height = (y2 - y0) as f32;

    for y in y0.max(0)..=y2.min(fb.height as i32 - 1) {
        let second_half = y > y1 || y1 == y0;
        let segment_height = if second_half {
            (y2 - y1) as f32
        } else {
            (y1 - y0) as f32
        };

        if segment_height == 0.0 {
            continue;
        }

        let alpha = (y - y0) as f32 / total_height;
        let beta = if second_half {
            (y - y1) as f32 / segment_height
        } else {
            (y - y0) as f32 / segment_height
        };

        let mut ax = x0 as f32 + (x2 - x0) as f32 * alpha;
        let mut bx = if second_half {
            x1 as f32 + (x2 - x1) as f32 * beta
        } else {
            x0 as f32 + (x1 - x0) as f32 * beta
        };

        if ax > bx {
            std::mem::swap(&mut ax, &mut bx);
        }

        let x_start = (ax as i32).max(0);
        let x_end = (bx as i32).min(fb.width as i32 - 1);

        for x in x_start..=x_end {
            fb.set_pixel(x as usize, y as usize, color);
        }
    }
}

/// Ray-bone intersection test for picking
/// Returns distance to the bone if hit, None otherwise
pub fn ray_bone_intersect(
    ray_origin: Vec3,
    ray_dir: Vec3,
    base: Vec3,
    tip: Vec3,
    bone_radius: f32,
) -> Option<f32> {
    // Simple cylinder/capsule intersection for bone picking
    // Using a simplified line-to-line distance check

    let bone_dir = tip - base;
    let bone_len = bone_dir.len();
    if bone_len < 0.001 {
        return None;
    }

    let bone_norm = bone_dir * (1.0 / bone_len);

    // Find closest point on ray to bone line
    // Using parametric line-line distance
    let w0 = ray_origin - base;
    let a = ray_dir.dot(ray_dir);
    let b = ray_dir.dot(bone_norm);
    let c = bone_norm.dot(bone_norm);
    let d = ray_dir.dot(w0);
    let e = bone_norm.dot(w0);

    let denom = a * c - b * b;
    if denom.abs() < 0.0001 {
        return None; // Parallel lines
    }

    let t_ray = (b * e - c * d) / denom;
    let t_bone = (a * e - b * d) / denom;

    // Check if intersection is within bone length
    if t_bone < 0.0 || t_bone > bone_len {
        return None;
    }

    // Check distance to bone center line
    let closest_ray = ray_origin + ray_dir * t_ray;
    let closest_bone = base + bone_norm * t_bone;
    let dist = (closest_ray - closest_bone).len();

    // Use a larger radius near the bone's center for easier picking
    let t_normalized = t_bone / bone_len;
    let effective_radius = if t_normalized < 0.3 {
        bone_radius * (0.5 + t_normalized * 1.5)  // Tapers from base
    } else {
        bone_radius * (1.0 - (t_normalized - 0.3) / 0.7 * 0.5)  // Tapers to tip
    };

    if dist < effective_radius && t_ray > 0.0 {
        Some(t_ray)
    } else {
        None
    }
}

// ============================================================================
// Triangle-based bone rendering for unified pipeline
// ============================================================================

use crate::rasterizer::{Vertex as RasterVertex, Face as RasterFace, BlendMode, Vec2};

/// Generate vertices and faces for all skeleton bones
/// Returns (vertices, faces) ready for unified rendering
pub fn skeleton_to_triangles(
    state: &ModelerState,
    editor_alpha: u8,
) -> (Vec<RasterVertex>, Vec<RasterFace>) {
    let skeleton = state.skeleton();
    let mut vertices: Vec<RasterVertex> = Vec::new();
    let mut faces: Vec<RasterFace> = Vec::new();

    if skeleton.is_empty() {
        return (vertices, faces);
    }

    // Check if skeleton component is selected (for bone coloring)
    let skeleton_component_selected = state.selected_component
        .and_then(|idx| state.asset.components.get(idx))
        .map(|c| c.is_skeleton())
        .unwrap_or(false);

    // When a bone is selected, dim non-selected bones to near-minimum alpha
    let has_selection = state.selected_bone.is_some();
    let dim_alpha: u8 = 30; // opacity level 6 — one step above hidden

    for (idx, bone) in skeleton.iter().enumerate() {
        // Determine bone color based on state
        // Opacity is handled by editor_alpha on faces, not by vertex color dimming
        let color = if state.selected_bone == Some(idx) {
            bone_color_selected()
        } else if state.hovered_bone_tip == Some(idx) {
            bone_color_tip_hovered()
        } else if state.hovered_bone == Some(idx) {
            bone_color_hovered()
        } else if bone.parent.is_none() {
            bone_color_root()
        } else {
            bone_color_default()
        };

        // Selected bone gets full alpha; others get dimmed when there's a selection
        let bone_alpha = if has_selection && state.selected_bone != Some(idx) {
            editor_alpha.min(dim_alpha)
        } else {
            editor_alpha
        };

        let (base_pos, _rotation) = state.get_bone_world_transform(idx);
        let tip_pos = state.get_bone_tip_position(idx);

        // Generate octahedron triangles for this bone
        generate_bone_octahedron(
            base_pos, tip_pos, bone.display_width(), color, bone_alpha,
            &mut vertices, &mut faces,
        );
    }

    // Add bone creation preview if active
    if let Some(ref creation) = state.bone_creation {
        let color = bone_color_creating();
        let preview_width = RigBone::DEFAULT_WIDTH;
        generate_bone_octahedron(
            creation.start_pos, creation.end_pos, preview_width, color, 255,
            &mut vertices, &mut faces,
        );
    }

    (vertices, faces)
}

/// Compute world transform for a bone by walking up the hierarchy.
/// Standalone version of `ModelerState::get_bone_world_transform` that takes raw bone data.
/// Returns (position, rotation) in world space.
pub fn bone_world_transform(bones: &[RigBone], bone_idx: usize) -> (Vec3, Vec3) {
    if bone_idx >= bones.len() {
        return (Vec3::ZERO, Vec3::ZERO);
    }

    let mut position = Vec3::ZERO;
    let mut rotation = Vec3::ZERO;

    // Build chain from root to this bone
    let mut current = Some(bone_idx);
    let mut chain = Vec::new();
    while let Some(idx) = current {
        chain.push(idx);
        current = bones[idx].parent;
    }

    // Apply transforms from root to leaf
    for idx in chain.into_iter().rev() {
        let bone = &bones[idx];
        let rotated_pos = rotate_by_euler(bone.local_position, rotation);
        position = position + rotated_pos;
        rotation = rotation + bone.local_rotation;
    }

    (position, rotation)
}

/// Compute world position of a bone's tip.
/// Standalone version of `ModelerState::get_bone_tip_position`.
pub fn bone_tip_position(bones: &[RigBone], bone_idx: usize) -> Vec3 {
    if bone_idx >= bones.len() {
        return Vec3::ZERO;
    }

    let (base_pos, rotation) = bone_world_transform(bones, bone_idx);
    let bone = &bones[bone_idx];

    let rad_x = rotation.x.to_radians();
    let rad_z = rotation.z.to_radians();
    let cos_x = rad_x.cos();
    let direction = Vec3::new(
        rad_z.sin() * cos_x,
        rad_z.cos() * cos_x,
        -rad_x.sin(),
    ).normalize();

    base_pos + direction * bone.length
}

/// Generate bone octahedron triangles from raw skeleton data.
/// Simplified version for read-only rendering (asset browser, previews).
/// Uses default/root coloring only — no selection, hover, or creation preview.
pub fn skeleton_to_triangles_from_bones(
    bones: &[RigBone],
    alpha: u8,
) -> (Vec<RasterVertex>, Vec<RasterFace>) {
    let mut vertices: Vec<RasterVertex> = Vec::new();
    let mut faces: Vec<RasterFace> = Vec::new();

    if bones.is_empty() {
        return (vertices, faces);
    }

    for (idx, bone) in bones.iter().enumerate() {
        let color = if bone.parent.is_none() {
            bone_color_root()
        } else {
            bone_color_default()
        };

        let (base_pos, _) = bone_world_transform(bones, idx);
        let tip_pos = bone_tip_position(bones, idx);

        generate_bone_octahedron(
            base_pos, tip_pos, bone.display_width(), color, alpha,
            &mut vertices, &mut faces,
        );
    }

    (vertices, faces)
}

/// Generate octahedron vertices and faces for a single bone
fn generate_bone_octahedron(
    base: Vec3,
    tip: Vec3,
    bone_width: f32,
    color: RasterColor,
    editor_alpha: u8,
    vertices: &mut Vec<RasterVertex>,
    faces: &mut Vec<RasterFace>,
) {
    // Calculate bone direction and length
    let direction = tip - base;
    let length = direction.len();
    if length < 0.001 {
        return; // Degenerate bone
    }

    let dir_norm = direction * (1.0 / length);

    // Find perpendicular axes
    let (perp1, perp2) = compute_perpendicular_axes(dir_norm);

    // Width of the bone (at the widest point)
    let width = bone_width;

    // Position of the ring (20% along the bone from base)
    let ring_center = base + dir_norm * (length * 0.2);

    // 4 ring vertices
    let ring = [
        ring_center + perp1 * width,
        ring_center + perp2 * width,
        ring_center - perp1 * width,
        ring_center - perp2 * width,
    ];

    // Track starting vertex index
    let v_start = vertices.len();

    // Add 6 vertices: base, tip, and 4 ring vertices
    // Vertex 0: base
    vertices.push(RasterVertex {
        pos: base,
        uv: Vec2::new(0.0, 0.0),
        normal: dir_norm * -1.0, // Point away from tip
        color,
        bone_index: None,
    });
    // Vertex 1: tip
    vertices.push(RasterVertex {
        pos: tip,
        uv: Vec2::new(0.0, 0.0),
        normal: dir_norm,
        color,
        bone_index: None,
    });
    // Vertices 2-5: ring
    for i in 0..4 {
        let ring_normal = (ring[i] - ring_center).normalize();
        vertices.push(RasterVertex {
            pos: ring[i],
            uv: Vec2::new(0.0, 0.0),
            normal: ring_normal,
            color,
            bone_index: None,
        });
    }

    // Add 8 faces (triangles)
    // Base pyramid: 4 triangles from base to ring
    for i in 0..4 {
        let next = (i + 1) % 4;
        faces.push(RasterFace {
            v0: v_start + 0,           // base
            v1: v_start + 2 + i,       // ring[i]
            v2: v_start + 2 + next,    // ring[next]
            texture_id: None,
            black_transparent: false,
            blend_mode: BlendMode::Opaque,
            editor_alpha,
        });
    }

    // Tip pyramid: 4 triangles from ring to tip
    for i in 0..4 {
        let next = (i + 1) % 4;
        faces.push(RasterFace {
            v0: v_start + 1,           // tip
            v1: v_start + 2 + next,    // ring[next] (reversed winding)
            v2: v_start + 2 + i,       // ring[i]
            texture_id: None,
            black_transparent: false,
            blend_mode: BlendMode::Opaque,
            editor_alpha,
        });
    }
}
