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

use crate::rasterizer::{Vec3, IVec3, Camera, screen_to_ray, ray_line_closest_point};
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
    /// initial_position_f32 is used for screen-space calculations
    pub fn start_move(
        &mut self,
        initial_position_f32: Vec3,
        initial_mouse: (f32, f32),
        axis: Option<Axis>,
        vertex_indices: Vec<usize>,
        initial_positions: Vec<(usize, IVec3)>,
        snap_enabled: bool,
        grid_size: i32,
    ) {
        let tracker = MoveTracker::new(axis, vertex_indices, initial_positions);
        let config = tracker.create_config(initial_position_f32, snap_enabled, grid_size);

        self.active = ActiveDrag::Move(tracker);
        self.state = Some(DragState::new(initial_position_f32, Vec3::ZERO, initial_mouse));
        self.config = Some(config);
    }

    /// Start a move drag operation with camera info for proper offset calculation.
    /// This prevents snapping when clicking on a gizmo axis.
    pub fn start_move_3d(
        &mut self,
        initial_position_f32: Vec3,
        initial_mouse: (f32, f32),
        axis: Option<Axis>,
        vertex_indices: Vec<usize>,
        initial_positions: Vec<(usize, IVec3)>,
        snap_enabled: bool,
        grid_size: i32,
        camera: &Camera,
        viewport_width: usize,
        viewport_height: usize,
    ) {
        let tracker = MoveTracker::new(axis, vertex_indices, initial_positions);
        let config = tracker.create_config(initial_position_f32, snap_enabled, grid_size);

        // Calculate handle_offset so the first pick returns initial_position_f32
        // This prevents snapping when clicking on a gizmo axis
        let handle_offset = if let Some(axis) = axis {
            // Cast ray from mouse click and find where it intersects the axis line
            let ray = screen_to_ray(initial_mouse.0, initial_mouse.1, viewport_width, viewport_height, camera);
            if let Some((closest, _dist)) = ray_line_closest_point(&ray, initial_position_f32, axis.unit_vector()) {
                // offset = initial_position_f32 - closest, so closest + offset = initial_position_f32
                initial_position_f32 - closest
            } else {
                Vec3::ZERO
            }
        } else {
            Vec3::ZERO
        };

        self.active = ActiveDrag::Move(tracker);
        self.state = Some(DragState::new(initial_position_f32, handle_offset, initial_mouse));
        self.config = Some(config);
    }

    /// Start a rotate drag operation
    /// center is used for screen-space calculations and passed as float
    pub fn start_rotate(
        &mut self,
        center: Vec3,
        center_int: IVec3,
        initial_angle: f32,
        initial_mouse: (f32, f32),
        center_screen: (f32, f32),
        axis: Axis,
        vertex_indices: Vec<usize>,
        initial_positions: Vec<(usize, IVec3)>,
        snap_enabled: bool,
        snap_degrees: f32,
    ) {
        let tracker = RotateTracker::new(axis, center_int, vertex_indices, initial_positions);
        let config = tracker.create_config(snap_enabled, snap_degrees);

        self.active = ActiveDrag::Rotate(tracker);
        self.state = Some(DragState::new_rotation(center, initial_angle, initial_mouse, center_screen));
        self.config = Some(config);
    }

    /// Start a scale drag operation
    pub fn start_scale(
        &mut self,
        center: Vec3,
        center_int: IVec3,
        initial_mouse: (f32, f32),
        axis: Option<Axis>,
        vertex_indices: Vec<usize>,
        initial_positions: Vec<(usize, IVec3)>,
    ) {
        let tracker = ScaleTracker::new(axis, center_int, vertex_indices, initial_positions);
        let config = tracker.create_config(center);

        self.active = ActiveDrag::Scale(tracker);
        self.state = Some(DragState::new(center, Vec3::ZERO, initial_mouse));
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
                        // Convert float delta to integer delta (scale by INT_SCALE)
                        use crate::rasterizer::INT_SCALE;
                        let delta_int = IVec3::new(
                            (delta.x * INT_SCALE as f32).round() as i32,
                            (delta.y * INT_SCALE as f32).round() as i32,
                            (delta.z * INT_SCALE as f32).round() as i32,
                        );
                        let new_positions = tracker.compute_new_positions(delta_int);
                        return DragUpdateResult::Move {
                            status: update.status,
                            positions: new_positions,
                        };
                    }
                }
                DragUpdateResult::Denied
            }

            ActiveDrag::Rotate(tracker) => {
                // Use screen-space angle calculation (more intuitive for gizmos)
                // Calculate angle from vectors relative to center_screen
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
                // Scale uses screen-space delta for now
                let delta = state.mouse_delta();
                let scale_factor = 1.0 + delta.0 * 0.01; // Horizontal drag = scale
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
                    self.config.as_ref().map(|c| c.grid_size as i32).unwrap_or(1),
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
        use crate::rasterizer::INT_SCALE;

        if !self.is_dragging() {
            return None;
        }

        let result = match &self.active {
            ActiveDrag::None => None,
            ActiveDrag::Move(tracker) => {
                let state = self.state.as_ref()?;
                let delta = state.position_delta();
                // Convert float delta to integer delta
                let delta_int = IVec3::new(
                    (delta.x * INT_SCALE as f32).round() as i32,
                    (delta.y * INT_SCALE as f32).round() as i32,
                    (delta.z * INT_SCALE as f32).round() as i32,
                );
                Some(DragEndResult::Move {
                    delta: delta_int,
                    final_positions: tracker.compute_new_positions(delta_int),
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
                let delta = state.mouse_delta();
                let factor = 1.0 + delta.0 * 0.01;
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
    pub fn cancel(&mut self) -> Option<Vec<(usize, IVec3)>> {
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
    /// Move drag updated - positions are integer coordinates
    Move {
        status: DragStatus,
        positions: Vec<(usize, IVec3)>,
    },
    /// Rotate drag updated - positions are integer coordinates
    Rotate {
        status: DragStatus,
        angle: f32,
        positions: Vec<(usize, IVec3)>,
    },
    /// Scale drag updated - positions are integer coordinates
    Scale {
        status: DragStatus,
        factor: f32,
        positions: Vec<(usize, IVec3)>,
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
        delta: IVec3,
        final_positions: Vec<(usize, IVec3)>,
    },
    Rotate {
        angle: f32,
        final_positions: Vec<(usize, IVec3)>,
    },
    Scale {
        factor: f32,
        final_positions: Vec<(usize, IVec3)>,
    },
    BoxSelect {
        start: (f32, f32),
        end: (f32, f32),
    },
}
