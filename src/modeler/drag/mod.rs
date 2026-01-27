//! Drag Tracker System for the Modeler
//!
//! Provides structured drag handling for gizmos and selection operations.
//! Inspired by TrenchBroom's HandleDragTracker pattern.
//!
//! Key types:
//! - `DragManager`: Manages the active drag operation
//! - `ActiveDrag`: Enum of all possible drag types
//! - Specific trackers: MoveTracker, RotateTracker, ScaleTracker, BoxSelectTracker

mod move_tracker;
mod rotate_tracker;
mod scale_tracker;
mod box_select;

pub use move_tracker::MoveTracker;
pub use rotate_tracker::RotateTracker;
pub use scale_tracker::ScaleTracker;
pub use box_select::BoxSelectTracker;

use crate::rasterizer::{Vec3, Camera, screen_to_ray, ray_line_closest_point, ray_circle_angle};
use crate::ui::{DragState, DragStatus, DragConfig, SnapMode, Axis, apply_drag_update};

/// The type of active drag operation
#[derive(Debug, Clone)]
pub enum ActiveDrag {
    /// No drag in progress
    None,
    /// Moving vertices/selection along axis or plane
    Move(MoveTracker),
    /// Rotating vertices/selection around an axis
    Rotate(RotateTracker),
    /// Scaling vertices/selection from center
    Scale(ScaleTracker),
    /// Box selection rectangle
    BoxSelect(BoxSelectTracker),
}

impl Default for ActiveDrag {
    fn default() -> Self {
        ActiveDrag::None
    }
}

impl ActiveDrag {
    pub fn is_active(&self) -> bool {
        !matches!(self, ActiveDrag::None)
    }

    pub fn is_move(&self) -> bool {
        matches!(self, ActiveDrag::Move(_))
    }

    /// Check if this is a free move (no axis constraint - screen-space movement)
    pub fn is_free_move(&self) -> bool {
        matches!(self, ActiveDrag::Move(t) if t.axis.is_none())
    }

    pub fn is_rotate(&self) -> bool {
        matches!(self, ActiveDrag::Rotate(_))
    }

    pub fn is_scale(&self) -> bool {
        matches!(self, ActiveDrag::Scale(_))
    }

    pub fn is_box_select(&self) -> bool {
        matches!(self, ActiveDrag::BoxSelect(_))
    }
}

/// Manages drag operations for the modeler
#[derive(Debug, Clone, Default)]
pub struct DragManager {
    /// Current active drag operation
    pub active: ActiveDrag,
    /// State of the current drag (position, mouse, etc.)
    pub state: Option<DragState>,
    /// Current drag configuration (picker, snapping)
    pub config: Option<DragConfig>,
}

impl DragManager {
    pub fn new() -> Self {
        Self::default()
    }

    /// Check if a drag is currently active
    pub fn is_dragging(&self) -> bool {
        self.active.is_active()
    }

    /// Start a move drag operation
    pub fn start_move(
        &mut self,
        initial_position: Vec3,
        initial_mouse: (f32, f32),
        axis: Option<Axis>,
        vertex_indices: Vec<usize>,
        initial_positions: Vec<(usize, Vec3)>,
        snap_enabled: bool,
        grid_size: f32,
    ) {
        let tracker = MoveTracker::new(axis, vertex_indices, initial_positions);
        let config = tracker.create_config(initial_position, snap_enabled, grid_size);

        self.active = ActiveDrag::Move(tracker);
        self.state = Some(DragState::new(initial_position, Vec3::ZERO, initial_mouse));
        self.config = Some(config);
    }

    /// Start a move drag operation with camera info for proper offset calculation.
    /// This prevents snapping when clicking on a gizmo axis.
    pub fn start_move_3d(
        &mut self,
        initial_position: Vec3,
        initial_mouse: (f32, f32),
        axis: Option<Axis>,
        vertex_indices: Vec<usize>,
        initial_positions: Vec<(usize, Vec3)>,
        snap_enabled: bool,
        grid_size: f32,
        camera: &Camera,
        viewport_width: usize,
        viewport_height: usize,
    ) {
        let tracker = MoveTracker::new(axis, vertex_indices, initial_positions);
        let config = tracker.create_config(initial_position, snap_enabled, grid_size);

        // Calculate handle_offset so the first pick returns initial_position
        // This prevents snapping when clicking on a gizmo axis
        let handle_offset = if let Some(axis) = axis {
            // Cast ray from mouse click and find where it intersects the axis line
            let ray = screen_to_ray(initial_mouse.0, initial_mouse.1, viewport_width, viewport_height, camera);
            if let Some((closest, _dist)) = ray_line_closest_point(&ray, initial_position, axis.unit_vector()) {
                // offset = initial_position - closest, so closest + offset = initial_position
                initial_position - closest
            } else {
                Vec3::ZERO
            }
        } else {
            Vec3::ZERO
        };

        self.active = ActiveDrag::Move(tracker);
        self.state = Some(DragState::new(initial_position, handle_offset, initial_mouse));
        self.config = Some(config);
    }

    /// Start a rotate drag operation with camera snapshot for arc-following rotation
    pub fn start_rotate(
        &mut self,
        center: Vec3,
        initial_angle: f32,
        initial_mouse: (f32, f32),
        center_screen: (f32, f32),
        axis: Axis,
        vertex_indices: Vec<usize>,
        initial_positions: Vec<(usize, Vec3)>,
        snap_enabled: bool,
        snap_degrees: f32,
        camera: &Camera,
        viewport_width: usize,
        viewport_height: usize,
        viewport_transform: (f32, f32, f32, f32), // (draw_x, draw_y, draw_w, draw_h)
    ) {
        let tracker = RotateTracker::new(axis, center, vertex_indices, initial_positions);
        let config = tracker.create_config(snap_enabled, snap_degrees);

        self.active = ActiveDrag::Rotate(tracker);
        self.state = Some(DragState::new_rotation_3d(
            center,
            initial_angle,
            initial_mouse,
            center_screen,
            camera.clone(),
            viewport_width,
            viewport_height,
            viewport_transform,
        ));
        self.config = Some(config);
    }

    /// Start a scale drag operation
    pub fn start_scale(
        &mut self,
        center: Vec3,
        initial_mouse: (f32, f32),
        axis: Option<Axis>,
        vertex_indices: Vec<usize>,
        initial_positions: Vec<(usize, Vec3)>,
        center_screen: (f32, f32),
    ) {
        let tracker = ScaleTracker::new(axis, center, vertex_indices, initial_positions);
        let config = tracker.create_config(center);

        self.active = ActiveDrag::Scale(tracker);
        // Use new_rotation constructor to get center_screen support for distance-based scaling
        self.state = Some(DragState::new_rotation(center, 0.0, initial_mouse, center_screen));
        self.config = Some(config);
    }

    /// Start a box select drag operation
    pub fn start_box_select(&mut self, initial_mouse: (f32, f32)) {
        let tracker = BoxSelectTracker::new(initial_mouse);

        self.active = ActiveDrag::BoxSelect(tracker);
        self.state = Some(DragState::new(Vec3::ZERO, Vec3::ZERO, initial_mouse));
        self.config = None; // Box select doesn't use 3D picking
    }

    /// Update the current drag with new mouse position
    /// Returns the drag status and optionally updated vertex positions
    pub fn update(
        &mut self,
        mouse_pos: (f32, f32),
        camera: &Camera,
        viewport_width: usize,
        viewport_height: usize,
    ) -> DragUpdateResult {
        let state = match &mut self.state {
            Some(s) => s,
            None => return DragUpdateResult::None,
        };

        state.current_mouse = mouse_pos;

        match &mut self.active {
            ActiveDrag::None => DragUpdateResult::None,

            ActiveDrag::Move(tracker) => {
                if let Some(config) = &self.config {
                    let update = apply_drag_update(
                        config,
                        state,
                        mouse_pos,
                        camera,
                        viewport_width,
                        viewport_height,
                    );

                    if let Some(new_pos) = update.new_position {
                        state.current_position = new_pos;
                        let delta = state.position_delta();
                        let new_positions = tracker.compute_new_positions(delta);
                        return DragUpdateResult::Move {
                            status: update.status,
                            positions: new_positions,
                        };
                    }
                }
                DragUpdateResult::Denied
            }

            ActiveDrag::Rotate(tracker) => {
                // Arc-following rotation: use ray-circle intersection with stored camera
                // This makes rotation feel like grabbing the ring
                //
                // IMPORTANT: mouse_pos is in RAW SCREEN coordinates.
                // We convert it to framebuffer coordinates using the stored viewport transform.
                // This ensures consistent behavior regardless of which viewport calls update.
                if let (Some(ref start_camera), Some((vp_w, vp_h)), Some((draw_x, draw_y, draw_w, draw_h))) =
                    (&state.start_camera, state.start_viewport, state.start_viewport_transform)
                {
                    // Convert screen mouse to framebuffer coordinates using stored transform
                    let fb_mouse = (
                        (mouse_pos.0 - draw_x) / draw_w * vp_w as f32,
                        (mouse_pos.1 - draw_y) / draw_h * vp_h as f32,
                    );

                    // Get reference vector for angle=0 (perpendicular to rotation axis)
                    let ref_vector = match tracker.axis {
                        Axis::X => Vec3::new(0.0, 1.0, 0.0),
                        Axis::Y => Vec3::new(1.0, 0.0, 0.0),
                        Axis::Z => Vec3::new(1.0, 0.0, 0.0),
                    };
                    let axis_vec = tracker.axis.unit_vector();

                    // Cast ray from current mouse position using stored camera
                    let ray = screen_to_ray(fb_mouse.0, fb_mouse.1, vp_w, vp_h, start_camera);
                    if let Some(current_angle) = ray_circle_angle(&ray, tracker.center, axis_vec, ref_vector) {
                        let angle_delta = current_angle - state.initial_angle;
                        state.current_angle = current_angle;
                        let new_positions = tracker.compute_new_positions(angle_delta);

                        return DragUpdateResult::Rotate {
                            status: DragStatus::Continue,
                            angle: state.current_angle,
                            positions: new_positions,
                        };
                    }
                }

                // Fallback to screen-space if ray casting fails
                let start_vec = (
                    state.initial_mouse.0 - state.center_screen.0,
                    state.initial_mouse.1 - state.center_screen.1,
                );
                let current_vec = (
                    mouse_pos.0 - state.center_screen.0,
                    mouse_pos.1 - state.center_screen.1,
                );

                let start_angle = start_vec.1.atan2(start_vec.0);
                let current_angle = current_vec.1.atan2(current_vec.0);
                let angle_delta = current_angle - start_angle;

                state.current_angle = state.initial_angle + angle_delta;
                let new_positions = tracker.compute_new_positions(angle_delta);

                DragUpdateResult::Rotate {
                    status: DragStatus::Continue,
                    angle: state.current_angle,
                    positions: new_positions,
                }
            }

            ActiveDrag::Scale(tracker) => {
                // Scale uses signed projection along the initial click direction
                // Drag in the direction you clicked = scale up
                // Drag in the opposite direction = scale down
                let initial_dx = state.initial_mouse.0 - state.center_screen.0;
                let initial_dy = state.initial_mouse.1 - state.center_screen.1;
                let initial_dist = (initial_dx * initial_dx + initial_dy * initial_dy).sqrt();

                // Mouse movement from initial click position
                let move_dx = state.current_mouse.0 - state.initial_mouse.0;
                let move_dy = state.current_mouse.1 - state.initial_mouse.1;

                // Project movement onto the initial direction (signed)
                let signed_dist = if initial_dist > 0.1 {
                    // Dot product with normalized initial direction
                    (move_dx * initial_dx + move_dy * initial_dy) / initial_dist
                } else {
                    // Clicked very close to center, use horizontal movement
                    move_dx
                };

                // Linear: each 200 pixels of movement = 100% scale change
                let scale_factor = (1.0 + signed_dist * 0.005).max(0.01);

                let new_positions = tracker.compute_new_positions(scale_factor);
                DragUpdateResult::Scale {
                    status: DragStatus::Continue,
                    factor: scale_factor,
                    positions: new_positions,
                }
            }

            ActiveDrag::BoxSelect(tracker) => {
                tracker.current_mouse = mouse_pos;
                DragUpdateResult::BoxSelect {
                    start: tracker.start_mouse,
                    current: mouse_pos,
                }
            }
        }
    }

    /// Change axis constraint mid-drag (for move/scale)
    pub fn set_axis(&mut self, axis: Option<Axis>) {
        let state = match &self.state {
            Some(s) => s,
            None => return,
        };

        match &mut self.active {
            ActiveDrag::Move(tracker) => {
                tracker.axis = axis;
                self.config = Some(tracker.create_config(
                    state.initial_position,
                    self.config.as_ref().map(|c| c.snap_mode != SnapMode::None).unwrap_or(false),
                    self.config.as_ref().map(|c| c.grid_size).unwrap_or(1.0),
                ));
            }
            ActiveDrag::Scale(tracker) => {
                tracker.axis = axis;
                self.config = Some(tracker.create_config(state.initial_position));
            }
            _ => {}
        }
    }

    /// Toggle snapping mid-drag
    pub fn set_snap(&mut self, enabled: bool, grid_size: f32) {
        if let Some(config) = &mut self.config {
            config.snap_mode = if enabled { SnapMode::Relative } else { SnapMode::None };
            config.grid_size = grid_size;
        }
    }

    /// End the drag operation (commit)
    pub fn end(&mut self) -> Option<DragEndResult> {
        if !self.is_dragging() {
            return None;
        }

        let result = match &self.active {
            ActiveDrag::None => None,
            ActiveDrag::Move(tracker) => {
                let state = self.state.as_ref()?;
                let delta = state.position_delta();
                Some(DragEndResult::Move {
                    delta,
                    final_positions: tracker.compute_new_positions(delta),
                })
            }
            ActiveDrag::Rotate(tracker) => {
                let state = self.state.as_ref()?;
                let angle = state.angle_delta();
                Some(DragEndResult::Rotate {
                    angle,
                    final_positions: tracker.compute_new_positions(angle),
                })
            }
            ActiveDrag::Scale(tracker) => {
                let state = self.state.as_ref()?;
                // Use same signed projection calculation as update
                let initial_dx = state.initial_mouse.0 - state.center_screen.0;
                let initial_dy = state.initial_mouse.1 - state.center_screen.1;
                let initial_dist = (initial_dx * initial_dx + initial_dy * initial_dy).sqrt();

                let move_dx = state.current_mouse.0 - state.initial_mouse.0;
                let move_dy = state.current_mouse.1 - state.initial_mouse.1;

                let signed_dist = if initial_dist > 0.1 {
                    (move_dx * initial_dx + move_dy * initial_dy) / initial_dist
                } else {
                    move_dx
                };

                let factor = (1.0 + signed_dist * 0.005).max(0.01);
                Some(DragEndResult::Scale {
                    factor,
                    final_positions: tracker.compute_new_positions(factor),
                })
            }
            ActiveDrag::BoxSelect(tracker) => {
                Some(DragEndResult::BoxSelect {
                    start: tracker.start_mouse,
                    end: tracker.current_mouse,
                })
            }
        };

        self.clear();
        result
    }

    /// Cancel the drag operation (rollback)
    pub fn cancel(&mut self) -> Option<Vec<(usize, Vec3)>> {
        if !self.is_dragging() {
            return None;
        }

        let original_positions = match &self.active {
            ActiveDrag::Move(tracker) => Some(tracker.initial_positions.clone()),
            ActiveDrag::Rotate(tracker) => Some(tracker.initial_positions.clone()),
            ActiveDrag::Scale(tracker) => Some(tracker.initial_positions.clone()),
            ActiveDrag::BoxSelect(_) => None,
            ActiveDrag::None => None,
        };

        self.clear();
        original_positions
    }

    /// Clear all drag state
    fn clear(&mut self) {
        self.active = ActiveDrag::None;
        self.state = None;
        self.config = None;
    }

    /// Get the current axis constraint (if any)
    pub fn current_axis(&self) -> Option<Axis> {
        match &self.active {
            ActiveDrag::Move(t) => t.axis,
            ActiveDrag::Rotate(t) => Some(t.axis),
            ActiveDrag::Scale(t) => t.axis,
            _ => None,
        }
    }
}

/// Result of a drag update
#[derive(Debug, Clone)]
pub enum DragUpdateResult {
    /// No drag active
    None,
    /// Drag update was denied (position couldn't be computed)
    Denied,
    /// Move drag updated
    Move {
        status: DragStatus,
        positions: Vec<(usize, Vec3)>,
    },
    /// Rotate drag updated
    Rotate {
        status: DragStatus,
        angle: f32,
        positions: Vec<(usize, Vec3)>,
    },
    /// Scale drag updated
    Scale {
        status: DragStatus,
        factor: f32,
        positions: Vec<(usize, Vec3)>,
    },
    /// Box select updated
    BoxSelect {
        start: (f32, f32),
        current: (f32, f32),
    },
}

/// Result of ending a drag
#[derive(Debug, Clone)]
pub enum DragEndResult {
    Move {
        delta: Vec3,
        final_positions: Vec<(usize, Vec3)>,
    },
    Rotate {
        angle: f32,
        final_positions: Vec<(usize, Vec3)>,
    },
    Scale {
        factor: f32,
        final_positions: Vec<(usize, Vec3)>,
    },
    BoxSelect {
        start: (f32, f32),
        end: (f32, f32),
    },
}
