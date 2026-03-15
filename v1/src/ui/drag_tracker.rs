//! Handle Drag Tracker System
//!
//! Inspired by TrenchBroom's HandleDragTracker pattern.
//! Provides a structured, composable system for handling drag operations
//! with proper 3D ray casting.
//!
//! Key concepts:
//! - `DragState`: Tracks initial/current positions and mouse coords
//! - `DragStatus`: Continue, Deny (reject position), or End
//! - `DragTracker` trait: Start, update, end, cancel, modifier change
//! - Pickers: Propose positions along lines, planes, circles
//! - Snappers: Snap positions to grid (relative or absolute)

use crate::rasterizer::{Vec3, Camera, OrthoProjection, screen_to_ray_auto, ray_line_closest_point, ray_plane_intersection, ray_circle_angle};

/// The status of a drag operation after an update
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DragStatus {
    /// Drag continues normally, position was applied
    Continue,
    /// Drag continues but this specific position was rejected (e.g., collision)
    Deny,
    /// Drag should end (e.g., object was deleted, validation failed permanently)
    End,
}

/// State of an active drag operation
#[derive(Debug, Clone)]
pub struct DragState {
    /// Initial handle position at drag start (in world space)
    pub initial_position: Vec3,
    /// Current handle position (updated as drag progresses)
    pub current_position: Vec3,
    /// Offset from click point to handle center (for accurate picking)
    pub handle_offset: Vec3,
    /// Initial mouse position in screen coords
    pub initial_mouse: (f32, f32),
    /// Current mouse position in screen coords
    pub current_mouse: (f32, f32),
    /// Initial angle (for rotation drags)
    pub initial_angle: f32,
    /// Current angle (for rotation drags)
    pub current_angle: f32,
    /// Center in screen coords (for rotation - calculate angle from this point)
    pub center_screen: (f32, f32),
    /// Camera snapshot at drag start (for consistent ray casting)
    pub start_camera: Option<Camera>,
    /// Viewport dimensions at drag start (framebuffer width, height)
    pub start_viewport: Option<(usize, usize)>,
    /// Viewport screen transform at drag start (draw_x, draw_y, draw_w, draw_h)
    /// Used to convert screen mouse coords to framebuffer coords consistently
    pub start_viewport_transform: Option<(f32, f32, f32, f32)>,
}

impl DragState {
    /// Create a new drag state
    pub fn new(
        initial_position: Vec3,
        handle_offset: Vec3,
        initial_mouse: (f32, f32),
    ) -> Self {
        Self {
            initial_position,
            current_position: initial_position,
            handle_offset,
            initial_mouse,
            current_mouse: initial_mouse,
            initial_angle: 0.0,
            current_angle: 0.0,
            center_screen: (0.0, 0.0),
            start_camera: None,
            start_viewport: None,
            start_viewport_transform: None,
        }
    }

    /// Create drag state for rotation
    pub fn new_rotation(
        center: Vec3,
        initial_angle: f32,
        initial_mouse: (f32, f32),
        center_screen: (f32, f32),
    ) -> Self {
        Self {
            initial_position: center,
            current_position: center,
            handle_offset: Vec3::ZERO,
            initial_mouse,
            current_mouse: initial_mouse,
            initial_angle,
            current_angle: initial_angle,
            center_screen,
            start_camera: None,
            start_viewport: None,
            start_viewport_transform: None,
        }
    }

    /// Create drag state for rotation with camera snapshot for consistent ray casting
    pub fn new_rotation_3d(
        center: Vec3,
        initial_angle: f32,
        initial_mouse: (f32, f32),
        center_screen: (f32, f32),
        camera: Camera,
        viewport_width: usize,
        viewport_height: usize,
        viewport_transform: (f32, f32, f32, f32), // (draw_x, draw_y, draw_w, draw_h)
    ) -> Self {
        Self {
            initial_position: center,
            current_position: center,
            handle_offset: Vec3::ZERO,
            initial_mouse,
            current_mouse: initial_mouse,
            initial_angle,
            current_angle: initial_angle,
            center_screen,
            start_camera: Some(camera),
            start_viewport: Some((viewport_width, viewport_height)),
            start_viewport_transform: Some(viewport_transform),
        }
    }

    /// Get the total position delta from start
    pub fn position_delta(&self) -> Vec3 {
        self.current_position - self.initial_position
    }

    /// Get the total angle delta from start (in radians)
    pub fn angle_delta(&self) -> f32 {
        self.current_angle - self.initial_angle
    }

    /// Get the mouse delta from start
    pub fn mouse_delta(&self) -> (f32, f32) {
        (
            self.current_mouse.0 - self.initial_mouse.0,
            self.current_mouse.1 - self.initial_mouse.1,
        )
    }

    /// Reset the initial position to current (for incremental movement)
    pub fn reset_initial(&mut self) {
        self.initial_position = self.current_position;
        self.initial_mouse = self.current_mouse;
        self.initial_angle = self.current_angle;
    }
}

/// Snapping mode for grid alignment
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SnapMode {
    /// No snapping
    #[default]
    None,
    /// Snap delta from initial position (movement feels more natural)
    Relative,
    /// Snap to absolute grid positions
    Absolute,
}

/// Axis constraint for movement
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Axis {
    X,
    Y,
    Z,
}

impl Axis {
    /// Get the unit vector for this axis
    pub fn unit_vector(&self) -> Vec3 {
        match self {
            Axis::X => Vec3::new(1.0, 0.0, 0.0),
            Axis::Y => Vec3::new(0.0, 1.0, 0.0),
            Axis::Z => Vec3::new(0.0, 0.0, 1.0),
        }
    }

    /// Get the perpendicular plane normal for this axis
    /// (the plane that contains movement along the other two axes)
    pub fn perpendicular_plane_normal(&self) -> Vec3 {
        self.unit_vector()
    }
}

/// Type of position picker to use
#[derive(Debug, Clone)]
pub enum PickerType {
    /// Pick along a line (for single-axis constraint)
    Line { origin: Vec3, direction: Vec3 },
    /// Pick on a plane (for two-axis movement or free movement)
    Plane { origin: Vec3, normal: Vec3 },
    /// Pick angle on a circle (for rotation)
    Circle { center: Vec3, axis: Vec3, ref_vector: Vec3 },
    /// Screen-space movement (for 2D UI or fallback)
    Screen { sensitivity: f32 },
}

/// Configuration for a drag operation
#[derive(Debug, Clone)]
pub struct DragConfig {
    /// How to pick the handle position
    pub picker: PickerType,
    /// Grid snapping mode
    pub snap_mode: SnapMode,
    /// Grid size for snapping
    pub grid_size: f32,
}

impl Default for DragConfig {
    fn default() -> Self {
        Self {
            picker: PickerType::Screen { sensitivity: 1.0 },
            snap_mode: SnapMode::None,
            grid_size: 1.0,
        }
    }
}

impl DragConfig {
    /// Create config for axis-constrained movement
    pub fn line(origin: Vec3, direction: Vec3) -> Self {
        Self {
            picker: PickerType::Line { origin, direction },
            ..Default::default()
        }
    }

    /// Create config for plane-constrained movement
    pub fn plane(origin: Vec3, normal: Vec3) -> Self {
        Self {
            picker: PickerType::Plane { origin, normal },
            ..Default::default()
        }
    }

    /// Create config for rotation
    pub fn circle(center: Vec3, axis: Vec3, ref_vector: Vec3) -> Self {
        Self {
            picker: PickerType::Circle { center, axis, ref_vector },
            ..Default::default()
        }
    }

    /// Enable relative snapping
    pub fn with_snap(mut self, grid_size: f32) -> Self {
        self.snap_mode = SnapMode::Relative;
        self.grid_size = grid_size;
        self
    }

    /// Enable absolute snapping
    pub fn with_absolute_snap(mut self, grid_size: f32) -> Self {
        self.snap_mode = SnapMode::Absolute;
        self.grid_size = grid_size;
        self
    }
}

// ============================================================================
// Position Pickers
// ============================================================================

/// Pick a position along a line using ray casting
pub fn pick_line(
    line_origin: Vec3,
    line_direction: Vec3,
    handle_offset: Vec3,
    mouse_pos: (f32, f32),
    camera: &Camera,
    viewport_width: usize,
    viewport_height: usize,
    ortho: Option<&OrthoProjection>,
) -> Option<Vec3> {
    let ray = screen_to_ray_auto(mouse_pos.0, mouse_pos.1, viewport_width, viewport_height, camera, ortho);

    // Offset line by handle_offset for accurate picking
    let (closest, _dist) = ray_line_closest_point(
        &ray,
        line_origin - handle_offset,
        line_direction,
    )?;

    Some(closest + handle_offset)
}

/// Pick a position on a plane using ray casting
pub fn pick_plane(
    plane_origin: Vec3,
    plane_normal: Vec3,
    handle_offset: Vec3,
    mouse_pos: (f32, f32),
    camera: &Camera,
    viewport_width: usize,
    viewport_height: usize,
    ortho: Option<&OrthoProjection>,
) -> Option<Vec3> {
    let ray = screen_to_ray_auto(mouse_pos.0, mouse_pos.1, viewport_width, viewport_height, camera, ortho);

    let t = ray_plane_intersection(&ray, plane_origin - handle_offset, plane_normal)?;
    Some(ray.at(t) + handle_offset)
}

/// Pick a rotation angle on a circle using ray casting
pub fn pick_circle_angle(
    center: Vec3,
    axis: Vec3,
    ref_vector: Vec3,
    mouse_pos: (f32, f32),
    camera: &Camera,
    viewport_width: usize,
    viewport_height: usize,
    ortho: Option<&OrthoProjection>,
) -> Option<f32> {
    let ray = screen_to_ray_auto(mouse_pos.0, mouse_pos.1, viewport_width, viewport_height, camera, ortho);
    ray_circle_angle(&ray, center, axis, ref_vector)
}

/// Pick position based on config
pub fn pick_position(
    config: &DragConfig,
    drag_state: &DragState,
    mouse_pos: (f32, f32),
    camera: &Camera,
    viewport_width: usize,
    viewport_height: usize,
    ortho: Option<&OrthoProjection>,
) -> Option<Vec3> {
    match &config.picker {
        PickerType::Line { origin, direction } => {
            pick_line(
                *origin,
                *direction,
                drag_state.handle_offset,
                mouse_pos,
                camera,
                viewport_width,
                viewport_height,
                ortho,
            )
        }
        PickerType::Plane { origin, normal } => {
            pick_plane(
                *origin,
                *normal,
                drag_state.handle_offset,
                mouse_pos,
                camera,
                viewport_width,
                viewport_height,
                ortho,
            )
        }
        PickerType::Circle { .. } => {
            // For circle, we don't pick a position, we pick an angle
            // Return the center position (actual rotation is handled separately)
            Some(drag_state.initial_position)
        }
        PickerType::Screen { sensitivity } => {
            // Screen-space movement fallback
            let delta = (
                (mouse_pos.0 - drag_state.initial_mouse.0) * sensitivity,
                (mouse_pos.1 - drag_state.initial_mouse.1) * sensitivity,
            );
            // Move in camera-relative XY plane
            // delta.1 > 0 when mouse moves down, and we want the selection to move down
            // basis_y points "up" in camera view, but in the projection positive cam_y = down on screen
            // So we use + to invert: mouse down → +delta.1 → +basis_y → larger cam_y → down on screen
            let world_delta = camera.basis_x * delta.0 + camera.basis_y * delta.1;
            Some(drag_state.initial_position + world_delta)
        }
    }
}

/// Pick angle based on config (for rotation)
pub fn pick_angle(
    config: &DragConfig,
    mouse_pos: (f32, f32),
    camera: &Camera,
    viewport_width: usize,
    viewport_height: usize,
    ortho: Option<&OrthoProjection>,
) -> Option<f32> {
    if let PickerType::Circle { center, axis, ref_vector } = &config.picker {
        pick_circle_angle(*center, *axis, *ref_vector, mouse_pos, camera, viewport_width, viewport_height, ortho)
    } else {
        None
    }
}

// ============================================================================
// Position Snappers
// ============================================================================

/// Snap a single value to grid
pub fn snap_value(value: f32, grid_size: f32) -> f32 {
    if grid_size <= 0.0 {
        return value;
    }
    (value / grid_size).round() * grid_size
}

/// Snap a position to grid (absolute mode)
pub fn snap_position_absolute(position: Vec3, grid_size: f32) -> Vec3 {
    Vec3::new(
        snap_value(position.x, grid_size),
        snap_value(position.y, grid_size),
        snap_value(position.z, grid_size),
    )
}

/// Snap a position to grid relative to initial position
pub fn snap_position_relative(
    position: Vec3,
    initial_position: Vec3,
    grid_size: f32,
) -> Vec3 {
    let delta = position - initial_position;
    let snapped_delta = Vec3::new(
        snap_value(delta.x, grid_size),
        snap_value(delta.y, grid_size),
        snap_value(delta.z, grid_size),
    );
    initial_position + snapped_delta
}

/// Snap position based on mode
pub fn snap_position(
    position: Vec3,
    initial_position: Vec3,
    mode: SnapMode,
    grid_size: f32,
) -> Vec3 {
    match mode {
        SnapMode::None => position,
        SnapMode::Relative => snap_position_relative(position, initial_position, grid_size),
        SnapMode::Absolute => snap_position_absolute(position, grid_size),
    }
}

/// Snap an angle to increments (in radians)
pub fn snap_angle(angle: f32, initial_angle: f32, snap_radians: f32, mode: SnapMode) -> f32 {
    if snap_radians <= 0.0 {
        return angle;
    }
    match mode {
        SnapMode::None => angle,
        SnapMode::Relative => {
            let delta = angle - initial_angle;
            let snapped_delta = (delta / snap_radians).round() * snap_radians;
            initial_angle + snapped_delta
        }
        SnapMode::Absolute => {
            (angle / snap_radians).round() * snap_radians
        }
    }
}

// ============================================================================
// Drag Tracker Trait
// ============================================================================

/// Modifier key state for drag operations
#[derive(Debug, Clone, Copy, Default)]
pub struct Modifiers {
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
}

/// Trait for objects that handle drag operations
///
/// Implement this trait for each type of drag (move, rotate, scale, etc.)
pub trait DragTracker {
    /// Called when drag starts. Returns configuration for position picking.
    fn start(&mut self, drag_state: &DragState) -> DragConfig;

    /// Called each frame during drag with proposed new position.
    /// Return Continue to accept, Deny to reject, End to finish.
    fn update(&mut self, drag_state: &mut DragState, proposed: Vec3) -> DragStatus;

    /// Called when drag ends normally (commit changes)
    fn end(&mut self, drag_state: &DragState);

    /// Called when drag is cancelled (rollback changes)
    fn cancel(&mut self, drag_state: &DragState);

    /// Called when modifier keys change mid-drag.
    /// Return Some(config) to change pick behavior, None to keep current.
    fn modifier_changed(&mut self, _drag_state: &DragState, _modifiers: Modifiers) -> Option<DragConfig> {
        None
    }
}

// ============================================================================
// Helper: Apply drag update
// ============================================================================

/// Result of applying a drag update
pub struct DragUpdate {
    pub status: DragStatus,
    pub new_position: Option<Vec3>,
    pub new_angle: Option<f32>,
}

/// Apply a complete drag update cycle:
/// 1. Pick new position based on config
/// 2. Snap if enabled
/// 3. Return the result
pub fn apply_drag_update(
    config: &DragConfig,
    drag_state: &DragState,
    mouse_pos: (f32, f32),
    camera: &Camera,
    viewport_width: usize,
    viewport_height: usize,
    ortho: Option<&OrthoProjection>,
) -> DragUpdate {
    // Handle rotation separately
    if let PickerType::Circle { center, axis, ref_vector } = &config.picker {
        if let Some(angle) = pick_circle_angle(
            *center, *axis, *ref_vector,
            mouse_pos, camera, viewport_width, viewport_height,
            ortho,
        ) {
            let snapped_angle = if config.snap_mode != SnapMode::None {
                snap_angle(angle, drag_state.initial_angle, config.grid_size, config.snap_mode)
            } else {
                angle
            };

            return DragUpdate {
                status: DragStatus::Continue,
                new_position: None,
                new_angle: Some(snapped_angle),
            };
        } else {
            return DragUpdate {
                status: DragStatus::Deny,
                new_position: None,
                new_angle: None,
            };
        }
    }

    // Position-based dragging
    if let Some(proposed) = pick_position(
        config, drag_state, mouse_pos, camera, viewport_width, viewport_height, ortho,
    ) {
        let snapped = snap_position(
            proposed,
            drag_state.initial_position,
            config.snap_mode,
            config.grid_size,
        );

        DragUpdate {
            status: DragStatus::Continue,
            new_position: Some(snapped),
            new_angle: None,
        }
    } else {
        DragUpdate {
            status: DragStatus::Deny,
            new_position: None,
            new_angle: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_snap_value() {
        assert!((snap_value(1.3, 1.0) - 1.0).abs() < 0.001);
        assert!((snap_value(1.6, 1.0) - 2.0).abs() < 0.001);
        assert!((snap_value(2.5, 1.0) - 3.0).abs() < 0.001); // Round half up
        assert!((snap_value(-1.3, 1.0) - -1.0).abs() < 0.001);
    }

    #[test]
    fn test_snap_position_absolute() {
        let pos = Vec3::new(1.3, 2.7, -0.4);
        let snapped = snap_position_absolute(pos, 1.0);
        assert!((snapped.x - 1.0).abs() < 0.001);
        assert!((snapped.y - 3.0).abs() < 0.001);
        assert!((snapped.z - 0.0).abs() < 0.001);
    }

    #[test]
    fn test_snap_position_relative() {
        let initial = Vec3::new(0.5, 0.5, 0.5);
        let current = Vec3::new(1.8, 2.3, 0.9);
        let snapped = snap_position_relative(current, initial, 1.0);

        // Delta is (1.3, 1.8, 0.4), snapped to (1.0, 2.0, 0.0)
        // Result is initial + snapped_delta = (1.5, 2.5, 0.5)
        assert!((snapped.x - 1.5).abs() < 0.001);
        assert!((snapped.y - 2.5).abs() < 0.001);
        assert!((snapped.z - 0.5).abs() < 0.001);
    }

    #[test]
    fn test_drag_state_delta() {
        let mut state = DragState::new(
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::ZERO,
            (100.0, 100.0),
        );
        state.current_position = Vec3::new(5.0, 3.0, -2.0);
        state.current_mouse = (150.0, 120.0);

        let pos_delta = state.position_delta();
        assert!((pos_delta.x - 5.0).abs() < 0.001);
        assert!((pos_delta.y - 3.0).abs() < 0.001);
        assert!((pos_delta.z - -2.0).abs() < 0.001);

        let mouse_delta = state.mouse_delta();
        assert!((mouse_delta.0 - 50.0).abs() < 0.001);
        assert!((mouse_delta.1 - 20.0).abs() < 0.001);
    }

    #[test]
    fn test_axis_vectors() {
        let x = Axis::X.unit_vector();
        assert!((x.x - 1.0).abs() < 0.001 && x.y.abs() < 0.001 && x.z.abs() < 0.001);

        let y = Axis::Y.unit_vector();
        assert!(y.x.abs() < 0.001 && (y.y - 1.0).abs() < 0.001 && y.z.abs() < 0.001);

        let z = Axis::Z.unit_vector();
        assert!(z.x.abs() < 0.001 && z.y.abs() < 0.001 && (z.z - 1.0).abs() < 0.001);
    }
}
