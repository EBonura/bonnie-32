//! Modeler editor state

use std::path::PathBuf;
use crate::rasterizer::{Camera, Vec2, Vec3, Color, RasterSettings};
use super::model::{Model, PartTransform};
use super::spine::SpineModel;

/// Modeler view modes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelerView {
    Model,      // Edit mesh geometry
    UV,         // Edit UV mapping
    Paint,      // Texture + vertex color painting
    Hierarchy,  // Edit part hierarchy + pivots
    Animate,    // Timeline + keyframe animation
}

impl ModelerView {
    pub const ALL: [ModelerView; 5] = [
        ModelerView::Model,
        ModelerView::UV,
        ModelerView::Paint,
        ModelerView::Hierarchy,
        ModelerView::Animate,
    ];

    pub fn label(&self) -> &'static str {
        match self {
            ModelerView::Model => "Model",
            ModelerView::UV => "UV",
            ModelerView::Paint => "Paint",
            ModelerView::Hierarchy => "Hierarchy",
            ModelerView::Animate => "Animate",
        }
    }

    pub fn index(&self) -> usize {
        *self as usize
    }

    pub fn from_index(i: usize) -> Option<Self> {
        Self::ALL.get(i).copied()
    }
}

/// Selection modes for modeling
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectMode {
    Bone,
    Part,
    Vertex,
    Edge,
    Face,
}

impl SelectMode {
    pub const ALL: [SelectMode; 5] = [
        SelectMode::Bone,
        SelectMode::Part,
        SelectMode::Vertex,
        SelectMode::Edge,
        SelectMode::Face,
    ];

    pub fn label(&self) -> &'static str {
        match self {
            SelectMode::Bone => "Bone",
            SelectMode::Part => "Part",
            SelectMode::Vertex => "Vertex",
            SelectMode::Edge => "Edge",
            SelectMode::Face => "Face",
        }
    }

    pub fn index(&self) -> usize {
        *self as usize
    }
}

/// Current selection in the modeler
#[derive(Debug, Clone)]
pub enum ModelerSelection {
    None,
    Bones(Vec<usize>),
    Parts(Vec<usize>),
    Vertices { part: usize, verts: Vec<usize> },
    Edges { part: usize, edges: Vec<(usize, usize)> },
    Faces { part: usize, faces: Vec<usize> },
    /// Spine joint selection: (segment_index, joint_index)
    SpineJoints(Vec<(usize, usize)>),
    /// Spine segment/bone selection: (segment_index, bone_index) where bone_index is the index of the first joint
    /// A bone connects joint[bone_index] to joint[bone_index + 1]
    SpineBones(Vec<(usize, usize)>),
}

impl ModelerSelection {
    pub fn is_empty(&self) -> bool {
        match self {
            ModelerSelection::None => true,
            ModelerSelection::Bones(v) => v.is_empty(),
            ModelerSelection::Parts(v) => v.is_empty(),
            ModelerSelection::Vertices { verts, .. } => verts.is_empty(),
            ModelerSelection::Edges { edges, .. } => edges.is_empty(),
            ModelerSelection::Faces { faces, .. } => faces.is_empty(),
            ModelerSelection::SpineJoints(v) => v.is_empty(),
            ModelerSelection::SpineBones(v) => v.is_empty(),
        }
    }

    pub fn clear(&mut self) {
        *self = ModelerSelection::None;
    }

    /// Get selected spine joints if any
    pub fn spine_joints(&self) -> Option<&[(usize, usize)]> {
        match self {
            ModelerSelection::SpineJoints(joints) => Some(joints),
            _ => None,
        }
    }

    /// Get selected spine bones if any
    pub fn spine_bones(&self) -> Option<&[(usize, usize)]> {
        match self {
            ModelerSelection::SpineBones(bones) => Some(bones),
            _ => None,
        }
    }
}

/// Active transform tool
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransformTool {
    Select,
    Move,
    Rotate,
    Scale,
    Extrude,
}

impl TransformTool {
    pub fn label(&self) -> &'static str {
        match self {
            TransformTool::Select => "Select",
            TransformTool::Move => "Move (G)",
            TransformTool::Rotate => "Rotate (R)",
            TransformTool::Scale => "Scale (S)",
            TransformTool::Extrude => "Extrude (E)",
        }
    }
}

/// Paint mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaintMode {
    Texture,
    VertexColor,
}

/// Axis constraint for transforms
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Axis {
    X,
    Y,
    Z,
}

impl Axis {
    pub fn label(&self) -> &'static str {
        match self {
            Axis::X => "X",
            Axis::Y => "Y",
            Axis::Z => "Z",
        }
    }

    pub fn color(&self) -> Color {
        match self {
            Axis::X => Color::new(255, 80, 80),   // Red
            Axis::Y => Color::new(80, 255, 80),   // Green
            Axis::Z => Color::new(80, 80, 255),   // Blue
        }
    }

    pub fn to_vec3(&self) -> Vec3 {
        match self {
            Axis::X => Vec3::new(1.0, 0.0, 0.0),
            Axis::Y => Vec3::new(0.0, 1.0, 0.0),
            Axis::Z => Vec3::new(0.0, 0.0, 1.0),
        }
    }
}

/// Gizmo handle types - single axis or plane (two axes)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GizmoHandle {
    /// Single axis movement
    Axis(Axis),
    /// Plane movement (move along two axes, locked on third)
    Plane { axis1: Axis, axis2: Axis },
}

impl GizmoHandle {
    pub const XY: GizmoHandle = GizmoHandle::Plane { axis1: Axis::X, axis2: Axis::Y };
    pub const XZ: GizmoHandle = GizmoHandle::Plane { axis1: Axis::X, axis2: Axis::Z };
    pub const YZ: GizmoHandle = GizmoHandle::Plane { axis1: Axis::Y, axis2: Axis::Z };

    pub fn label(&self) -> &'static str {
        match self {
            GizmoHandle::Axis(a) => a.label(),
            GizmoHandle::Plane { axis1: Axis::X, axis2: Axis::Y } => "XY",
            GizmoHandle::Plane { axis1: Axis::X, axis2: Axis::Z } => "XZ",
            GizmoHandle::Plane { axis1: Axis::Y, axis2: Axis::Z } => "YZ",
            _ => "Plane",
        }
    }
}

/// Snap/quantization settings
#[derive(Debug, Clone, Copy)]
pub struct SnapSettings {
    pub enabled: bool,
    pub grid_size: f32,  // World units to snap to
}

impl Default for SnapSettings {
    fn default() -> Self {
        Self {
            enabled: true,  // Enabled by default
            grid_size: 5.0,  // 5 unit grid by default
        }
    }
}

impl SnapSettings {
    /// Snap a value to the grid
    pub fn snap(&self, value: f32) -> f32 {
        if self.enabled {
            (value / self.grid_size).round() * self.grid_size
        } else {
            value
        }
    }

    /// Snap a Vec3 to the grid
    pub fn snap_vec3(&self, v: Vec3) -> Vec3 {
        if self.enabled {
            Vec3::new(
                self.snap(v.x),
                self.snap(v.y),
                self.snap(v.z),
            )
        } else {
            v
        }
    }
}

/// Main modeler state
pub struct ModelerState {
    // Model data (old system - keeping for now)
    pub model: Model,
    pub current_file: Option<PathBuf>,

    // Spine model (new procedural system)
    pub spine_model: Option<SpineModel>,
    /// Cached mesh from spine (regenerated when spine changes)
    pub spine_mesh_dirty: bool,

    // View state
    pub view: ModelerView,
    pub select_mode: SelectMode,
    pub tool: TransformTool,
    pub selection: ModelerSelection,

    // Camera (orbit mode)
    pub camera: Camera,
    pub raster_settings: RasterSettings,
    pub orbit_target: Vec3,      // Point the camera orbits around
    pub orbit_distance: f32,     // Distance from target
    pub orbit_azimuth: f32,      // Horizontal angle (radians)
    pub orbit_elevation: f32,    // Vertical angle (radians)

    // UV Editor state
    pub uv_zoom: f32,
    pub uv_offset: Vec2,
    pub uv_selection: Vec<usize>,

    // Paint state
    pub paint_color: Color,
    pub brush_size: f32,
    pub paint_mode: PaintMode,

    // Hierarchy state
    pub hierarchy_expanded: Vec<bool>,

    // Animation state
    pub current_animation: usize,
    pub current_frame: u32,
    pub playing: bool,
    pub playback_time: f64,
    pub selected_keyframes: Vec<usize>,

    // Edit state
    pub undo_stack: Vec<Model>,
    pub redo_stack: Vec<Model>,
    pub dirty: bool,
    pub status_message: Option<(String, f64)>,

    // Transform state (for mouse drag)
    pub transform_active: bool,
    pub transform_start_mouse: (f32, f32),
    pub transform_start_positions: Vec<Vec3>,
    pub transform_start_rotations: Vec<Vec3>,
    pub axis_lock: Option<Axis>,

    // Spine joint drag state
    pub spine_drag_active: bool,
    pub spine_drag_start_mouse: (f32, f32),
    pub spine_drag_start_positions: Vec<Vec3>,
    /// Which gizmo handle is being dragged (None = free drag on camera plane)
    pub spine_drag_handle: Option<GizmoHandle>,
    /// Hovered gizmo handle (for highlighting)
    pub gizmo_hover_handle: Option<GizmoHandle>,

    // Snap/quantization settings
    pub snap_settings: SnapSettings,

    // Viewport mouse state
    pub viewport_last_mouse: (f32, f32),
    pub viewport_mouse_captured: bool,
}

impl ModelerState {
    pub fn new() -> Self {
        // Orbit camera setup
        let orbit_target = Vec3::new(0.0, 50.0, 0.0); // Center of scene, slightly elevated
        let orbit_distance = 400.0;
        let orbit_azimuth = 0.8;      // ~45 degrees
        let orbit_elevation = 0.3;    // ~17 degrees up

        let mut camera = Camera::new();
        Self::update_camera_from_orbit(&mut camera, orbit_target, orbit_distance, orbit_azimuth, orbit_elevation);

        Self {
            model: Model::test_humanoid(),
            current_file: None,

            // Start with a test spine model
            spine_model: Some(SpineModel::test_humanoid()),
            spine_mesh_dirty: true,

            view: ModelerView::Model,
            select_mode: SelectMode::Bone,
            tool: TransformTool::Select,
            selection: ModelerSelection::None,

            camera,
            raster_settings: RasterSettings::default(),
            orbit_target,
            orbit_distance,
            orbit_azimuth,
            orbit_elevation,

            uv_zoom: 1.0,
            uv_offset: Vec2::default(),
            uv_selection: Vec::new(),

            paint_color: Color::WHITE,
            brush_size: 4.0,
            paint_mode: PaintMode::Texture,

            hierarchy_expanded: Vec::new(),

            current_animation: 0,
            current_frame: 0,
            playing: false,
            playback_time: 0.0,
            selected_keyframes: Vec::new(),

            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            dirty: false,
            status_message: None,

            transform_active: false,
            transform_start_mouse: (0.0, 0.0),
            transform_start_positions: Vec::new(),
            transform_start_rotations: Vec::new(),
            axis_lock: None,

            spine_drag_active: false,
            spine_drag_start_mouse: (0.0, 0.0),
            spine_drag_start_positions: Vec::new(),
            spine_drag_handle: None,
            gizmo_hover_handle: None,

            snap_settings: SnapSettings::default(),

            viewport_last_mouse: (0.0, 0.0),
            viewport_mouse_captured: false,
        }
    }

    /// Update camera position and orientation from orbit parameters
    fn update_camera_from_orbit(camera: &mut Camera, target: Vec3, distance: f32, azimuth: f32, elevation: f32) {
        // Match camera's basis calculation from render.rs:
        // basis_z.x = cos(rotation_x) * sin(rotation_y)
        // basis_z.y = -sin(rotation_x)
        // basis_z.z = cos(rotation_x) * cos(rotation_y)
        //
        // Camera looks along +basis_z, so position = target - basis_z * distance
        // For orbit: rotation_x = elevation (pitch), rotation_y = azimuth (yaw)

        let pitch = elevation;
        let yaw = azimuth;

        // Forward direction (what camera looks at)
        let forward = Vec3::new(
            pitch.cos() * yaw.sin(),
            -pitch.sin(),
            pitch.cos() * yaw.cos(),
        );

        // Camera sits behind the target along the forward direction
        camera.position = target - forward * distance;
        camera.rotation_x = pitch;
        camera.rotation_y = yaw;
        camera.update_basis();
    }

    /// Update the camera from current orbit state
    pub fn sync_camera_from_orbit(&mut self) {
        Self::update_camera_from_orbit(
            &mut self.camera,
            self.orbit_target,
            self.orbit_distance,
            self.orbit_azimuth,
            self.orbit_elevation,
        );
    }

    /// Set a status message that will be displayed for a duration
    pub fn set_status(&mut self, message: &str, duration_secs: f64) {
        let expiry = macroquad::time::get_time() + duration_secs;
        self.status_message = Some((message.to_string(), expiry));
    }

    /// Get current status message if not expired
    pub fn get_status(&self) -> Option<&str> {
        if let Some((msg, expiry)) = &self.status_message {
            if macroquad::time::get_time() < *expiry {
                return Some(msg);
            }
        }
        None
    }

    /// Save current state for undo
    pub fn save_undo(&mut self) {
        self.undo_stack.push(self.model.clone());
        self.redo_stack.clear();
        self.dirty = true;

        // Limit undo stack size
        if self.undo_stack.len() > 50 {
            self.undo_stack.remove(0);
        }
    }

    /// Undo last action
    pub fn undo(&mut self) {
        if let Some(prev) = self.undo_stack.pop() {
            self.redo_stack.push(self.model.clone());
            self.model = prev;
            self.set_status("Undo", 1.0);
        }
    }

    /// Redo last undone action
    pub fn redo(&mut self) {
        if let Some(next) = self.redo_stack.pop() {
            self.undo_stack.push(self.model.clone());
            self.model = next;
            self.set_status("Redo", 1.0);
        }
    }

    /// Get the current animation being edited
    pub fn current_animation(&self) -> Option<&super::model::Animation> {
        self.model.animations.get(self.current_animation)
    }

    /// Get the current animation mutably
    pub fn current_animation_mut(&mut self) -> Option<&mut super::model::Animation> {
        self.model.animations.get_mut(self.current_animation)
    }

    /// Interpolate pose at current frame
    pub fn get_current_pose(&self) -> Vec<PartTransform> {
        let num_parts = self.model.parts.len();

        let anim = match self.current_animation() {
            Some(a) if !a.keyframes.is_empty() => a,
            _ => return vec![PartTransform::default(); num_parts],
        };

        // Find surrounding keyframes
        let frame = self.current_frame;
        let mut prev_kf = &anim.keyframes[0];
        let mut next_kf = &anim.keyframes[0];

        for kf in &anim.keyframes {
            if kf.frame <= frame {
                prev_kf = kf;
            }
            if kf.frame >= frame && next_kf.frame <= frame {
                next_kf = kf;
            }
        }

        // If same keyframe, no interpolation needed
        if prev_kf.frame == next_kf.frame {
            return prev_kf.transforms.clone();
        }

        // Interpolate
        let t = (frame - prev_kf.frame) as f32 / (next_kf.frame - prev_kf.frame) as f32;

        prev_kf.transforms
            .iter()
            .zip(next_kf.transforms.iter())
            .map(|(a, b)| a.lerp(b, t))
            .collect()
    }

    /// Toggle playback
    pub fn toggle_playback(&mut self) {
        self.playing = !self.playing;
        if self.playing {
            self.playback_time = 0.0;
        }
    }

    /// Stop playback and return to frame 0
    pub fn stop_playback(&mut self) {
        self.playing = false;
        self.current_frame = 0;
        self.playback_time = 0.0;
    }

    /// Update animation playback
    pub fn update_playback(&mut self, delta: f64) {
        if !self.playing {
            return;
        }

        let anim = match self.current_animation() {
            Some(a) => a,
            None => {
                self.playing = false;
                return;
            }
        };

        let fps = anim.fps as f64;
        let last_frame = anim.last_frame();
        let looping = anim.looping;

        self.playback_time += delta;
        let frame_duration = 1.0 / fps;

        while self.playback_time >= frame_duration {
            self.playback_time -= frame_duration;
            self.current_frame += 1;

            if self.current_frame > last_frame {
                if looping {
                    self.current_frame = 0;
                } else {
                    self.current_frame = last_frame;
                    self.playing = false;
                    break;
                }
            }
        }
    }

    /// Insert keyframe at current frame for all parts
    pub fn insert_keyframe(&mut self) {
        let frame = self.current_frame;
        let pose = self.get_current_pose();

        // Ensure we have at least one animation
        if self.model.animations.is_empty() {
            self.model.animations.push(super::model::Animation::new("default"));
        }

        let anim = &mut self.model.animations[self.current_animation];
        anim.set_keyframe(super::model::Keyframe {
            frame,
            transforms: pose,
        });

        self.dirty = true;
        self.set_status(&format!("Keyframe inserted at frame {}", frame), 1.5);
    }

    /// Delete keyframe at current frame
    pub fn delete_keyframe(&mut self) {
        let frame = self.current_frame;

        if let Some(anim) = self.current_animation_mut() {
            anim.remove_keyframe(frame);
            self.dirty = true;
            self.set_status(&format!("Keyframe deleted at frame {}", frame), 1.5);
        }
    }

    /// Cycle to next view mode
    pub fn next_view(&mut self) {
        let next = (self.view.index() + 1) % ModelerView::ALL.len();
        self.view = ModelerView::from_index(next).unwrap();
        self.set_status(&format!("Mode: {}", self.view.label()), 1.0);
    }

    /// Save spine model to file (uses RON format like levels)
    pub fn save_spine_model(&mut self, path: &std::path::Path) -> Result<(), String> {
        if let Some(spine_model) = &self.spine_model {
            spine_model.save_to_file(path).map_err(|e| e.to_string())?;
            self.current_file = Some(path.to_path_buf());
            self.dirty = false;
            self.set_status(&format!("Saved: {}", path.display()), 2.0);
            Ok(())
        } else {
            Err("No spine model to save".to_string())
        }
    }

    /// Load spine model from file (uses RON format like levels)
    pub fn load_spine_model(&mut self, path: &std::path::Path) -> Result<(), String> {
        let model = SpineModel::load_from_file(path).map_err(|e| e.to_string())?;
        self.spine_model = Some(model);
        self.current_file = Some(path.to_path_buf());
        self.selection = ModelerSelection::None;
        self.dirty = false;
        self.set_status(&format!("Loaded: {}", path.display()), 2.0);
        Ok(())
    }

    /// Create a new empty spine model
    pub fn new_spine_model(&mut self) {
        self.spine_model = Some(SpineModel::new_empty("untitled"));
        self.current_file = None;
        self.selection = ModelerSelection::SpineJoints(vec![(0, 0)]);
        self.dirty = false;
        self.set_status("New model created", 1.5);
    }

    /// Get current spine model file path (for save)
    pub fn current_spine_file(&self) -> Option<&std::path::Path> {
        self.current_file.as_deref()
    }
}

impl Default for ModelerState {
    fn default() -> Self {
        Self::new()
    }
}
