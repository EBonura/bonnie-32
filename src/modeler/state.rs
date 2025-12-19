//! Modeler editor state
//!
//! Blender-style mode hierarchy:
//! - InteractionMode: Object vs Edit (Tab to toggle)
//! - DataContext: Spine, Mesh, or Rig (1/2/3 keys)
//! - RigSubMode: Skeleton, Parts, Animate (Shift+1/2/3 in Rig context)

use std::path::PathBuf;
use serde::{Deserialize, Serialize};
use crate::rasterizer::{Camera, Vec2, Vec3, Color, RasterSettings};
use super::spine::SpineModel;
use super::mesh_editor::EditableMesh;
use super::model::{Animation, Keyframe, BoneTransform};

// ============================================================================
// Mode Hierarchy (Blender-style)
// ============================================================================

/// Object vs Edit mode (like Blender's Tab toggle)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum InteractionMode {
    /// Work with whole objects (parts, bones, spine segments)
    Object,
    /// Edit internal structure (vertices, joints)
    #[default]
    Edit,
}

impl InteractionMode {
    pub fn label(&self) -> &'static str {
        match self {
            InteractionMode::Object => "Object",
            InteractionMode::Edit => "Edit",
        }
    }

    pub fn toggle(&self) -> Self {
        match self {
            InteractionMode::Object => InteractionMode::Edit,
            InteractionMode::Edit => InteractionMode::Object,
        }
    }
}

/// What data context we're working in
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DataContext {
    /// Procedural mesh from spine joints
    Spine,
    /// Imported/created mesh geometry (default - start with cube)
    #[default]
    Mesh,
    /// Rigged model with skeleton
    Rig,
}

impl DataContext {
    pub const ALL: [DataContext; 3] = [
        DataContext::Spine,
        DataContext::Mesh,
        DataContext::Rig,
    ];

    pub fn label(&self) -> &'static str {
        match self {
            DataContext::Spine => "Spine",
            DataContext::Mesh => "Mesh",
            DataContext::Rig => "Rig",
        }
    }

    pub fn index(&self) -> usize {
        match self {
            DataContext::Spine => 0,
            DataContext::Mesh => 1,
            DataContext::Rig => 2,
        }
    }

    pub fn from_index(i: usize) -> Option<Self> {
        Self::ALL.get(i).copied()
    }
}

/// Sub-mode when in Rig context
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RigSubMode {
    /// Create/edit bones
    #[default]
    Skeleton,
    /// Split mesh, assign parts to bones
    Parts,
    /// Pose and keyframe animation
    Animate,
}

impl RigSubMode {
    pub const ALL: [RigSubMode; 3] = [
        RigSubMode::Skeleton,
        RigSubMode::Parts,
        RigSubMode::Animate,
    ];

    pub fn label(&self) -> &'static str {
        match self {
            RigSubMode::Skeleton => "Skeleton",
            RigSubMode::Parts => "Parts",
            RigSubMode::Animate => "Animate",
        }
    }

    pub fn index(&self) -> usize {
        match self {
            RigSubMode::Skeleton => 0,
            RigSubMode::Parts => 1,
            RigSubMode::Animate => 2,
        }
    }

    pub fn from_index(i: usize) -> Option<Self> {
        Self::ALL.get(i).copied()
    }
}

// ============================================================================
// PS1-Style Rigged Model
// ============================================================================

/// A complete rigged model ready for animation
/// PS1-style: each part is rigidly attached to one bone (no vertex weights)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiggedModel {
    pub name: String,
    pub skeleton: Vec<RigBone>,
    pub parts: Vec<MeshPart>,
    pub animations: Vec<Animation>,
}

impl RiggedModel {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            skeleton: Vec::new(),
            parts: Vec::new(),
            animations: vec![Animation::new("Action")],
        }
    }

    /// Create from an editable mesh (single part, no bones yet)
    pub fn from_mesh(name: &str, mesh: EditableMesh) -> Self {
        Self {
            name: name.to_string(),
            skeleton: Vec::new(),
            parts: vec![MeshPart {
                name: "root".to_string(),
                bone_index: None,
                mesh,
                pivot: Vec3::ZERO,
            }],
            animations: vec![Animation::new("Action")],
        }
    }

    /// Add a bone and return its index
    pub fn add_bone(&mut self, bone: RigBone) -> usize {
        let idx = self.skeleton.len();
        self.skeleton.push(bone);
        idx
    }

    /// Get root bones (no parent)
    pub fn root_bones(&self) -> Vec<usize> {
        self.skeleton
            .iter()
            .enumerate()
            .filter(|(_, b)| b.parent.is_none())
            .map(|(i, _)| i)
            .collect()
    }

    /// Get children of a bone
    pub fn bone_children(&self, parent_index: usize) -> Vec<usize> {
        self.skeleton
            .iter()
            .enumerate()
            .filter(|(_, b)| b.parent == Some(parent_index))
            .map(|(i, _)| i)
            .collect()
    }
}

/// A single bone in the hierarchy
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RigBone {
    pub name: String,
    /// Parent bone index (None = root bone)
    pub parent: Option<usize>,
    /// Local position relative to parent (bind pose)
    pub local_position: Vec3,
    /// Local rotation in degrees (bind pose)
    pub local_rotation: Vec3,
    /// Length of bone for visualization
    pub length: f32,
}

impl RigBone {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            parent: None,
            local_position: Vec3::ZERO,
            local_rotation: Vec3::ZERO,
            length: 20.0,
        }
    }

    pub fn with_parent(name: &str, parent: usize) -> Self {
        Self {
            name: name.to_string(),
            parent: Some(parent),
            local_position: Vec3::ZERO,
            local_rotation: Vec3::ZERO,
            length: 20.0,
        }
    }
}

/// A mesh piece that moves 100% with its bone (PS1-style rigid binding)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeshPart {
    pub name: String,
    /// Which bone this part follows (None = unassigned)
    pub bone_index: Option<usize>,
    /// The geometry (in bone-local space when assigned)
    pub mesh: EditableMesh,
    /// Local pivot point
    pub pivot: Vec3,
}

// ============================================================================
// Selection Modes (simplified, context-aware)
// ============================================================================

/// Selection mode - what type of element to select
/// Constrained by current DataContext and InteractionMode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectMode {
    // Spine context
    Segment,  // Object mode: select whole segments
    Joint,    // Edit mode: select joints
    SpineBone,// Edit mode: select bones (joint pairs)

    // Mesh context
    Vertex,
    Edge,
    Face,

    // Rig context
    Part,     // Select mesh parts
    RigBone,  // Select bones
}

impl SelectMode {
    pub fn label(&self) -> &'static str {
        match self {
            SelectMode::Segment => "Segment",
            SelectMode::Joint => "Joint",
            SelectMode::SpineBone => "Bone",
            SelectMode::Vertex => "Vertex",
            SelectMode::Edge => "Edge",
            SelectMode::Face => "Face",
            SelectMode::Part => "Part",
            SelectMode::RigBone => "Bone",
        }
    }

    /// Get valid select modes for given context and interaction mode
    pub fn valid_modes(context: DataContext, interaction: InteractionMode) -> Vec<SelectMode> {
        match (context, interaction) {
            (DataContext::Spine, InteractionMode::Object) => vec![SelectMode::Segment],
            (DataContext::Spine, InteractionMode::Edit) => vec![SelectMode::Joint, SelectMode::SpineBone],
            (DataContext::Mesh, InteractionMode::Object) => vec![], // Whole mesh selected
            (DataContext::Mesh, InteractionMode::Edit) => vec![SelectMode::Vertex, SelectMode::Edge, SelectMode::Face],
            (DataContext::Rig, InteractionMode::Object) => vec![SelectMode::Part, SelectMode::RigBone],
            (DataContext::Rig, InteractionMode::Edit) => vec![SelectMode::Vertex, SelectMode::Edge, SelectMode::Face],
        }
    }
}

/// Current selection in the modeler (simplified, context-aware)
#[derive(Debug, Clone)]
pub enum ModelerSelection {
    None,

    // Spine context
    /// Object mode: selected spine segments
    SpineSegments(Vec<usize>),
    /// Edit mode: selected joints (segment_index, joint_index)
    SpineJoints(Vec<(usize, usize)>),
    /// Edit mode: selected bones (segment_index, bone_index)
    SpineBones(Vec<(usize, usize)>),

    // Mesh context
    /// Object mode: whole mesh selected
    Mesh,
    /// Edit mode: selected vertices
    MeshVertices(Vec<usize>),
    /// Edit mode: selected edges (v0_index, v1_index)
    MeshEdges(Vec<(usize, usize)>),
    /// Edit mode: selected faces
    MeshFaces(Vec<usize>),

    // Rig context
    /// Object mode: selected mesh parts
    RigParts(Vec<usize>),
    /// Object mode: selected bones
    RigBones(Vec<usize>),
}

impl ModelerSelection {
    pub fn is_empty(&self) -> bool {
        match self {
            ModelerSelection::None => true,
            ModelerSelection::SpineSegments(v) => v.is_empty(),
            ModelerSelection::SpineJoints(v) => v.is_empty(),
            ModelerSelection::SpineBones(v) => v.is_empty(),
            ModelerSelection::Mesh => false, // Whole mesh is selected
            ModelerSelection::MeshVertices(v) => v.is_empty(),
            ModelerSelection::MeshEdges(v) => v.is_empty(),
            ModelerSelection::MeshFaces(v) => v.is_empty(),
            ModelerSelection::RigParts(v) => v.is_empty(),
            ModelerSelection::RigBones(v) => v.is_empty(),
        }
    }

    pub fn clear(&mut self) {
        *self = ModelerSelection::None;
    }

    /// Get selected spine segments if any
    pub fn spine_segments(&self) -> Option<&[usize]> {
        match self {
            ModelerSelection::SpineSegments(segs) => Some(segs),
            _ => None,
        }
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

    /// Get selected mesh vertices if any
    pub fn mesh_vertices(&self) -> Option<&[usize]> {
        match self {
            ModelerSelection::MeshVertices(verts) => Some(verts),
            _ => None,
        }
    }

    /// Get selected mesh edges if any
    pub fn mesh_edges(&self) -> Option<&[(usize, usize)]> {
        match self {
            ModelerSelection::MeshEdges(edges) => Some(edges),
            _ => None,
        }
    }

    /// Get selected mesh faces if any
    pub fn mesh_faces(&self) -> Option<&[usize]> {
        match self {
            ModelerSelection::MeshFaces(faces) => Some(faces),
            _ => None,
        }
    }

    /// Get selected rig parts if any
    pub fn rig_parts(&self) -> Option<&[usize]> {
        match self {
            ModelerSelection::RigParts(parts) => Some(parts),
            _ => None,
        }
    }

    /// Get selected rig bones if any
    pub fn rig_bones(&self) -> Option<&[usize]> {
        match self {
            ModelerSelection::RigBones(bones) => Some(bones),
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

/// Modal transform mode (Blender-style G/S/R)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModalTransform {
    None,
    Grab,   // G key - move selection
    Scale,  // S key - scale selection
    Rotate, // R key - rotate selection
}

impl ModalTransform {
    pub fn label(&self) -> &'static str {
        match self {
            ModalTransform::None => "",
            ModalTransform::Grab => "Grab",
            ModalTransform::Scale => "Scale",
            ModalTransform::Rotate => "Rotate",
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
    // Mode hierarchy (Blender-style)
    pub interaction_mode: InteractionMode,
    pub data_context: DataContext,
    pub rig_sub_mode: RigSubMode,  // Only relevant when context == Rig

    // Data (only one active at a time based on context)
    pub spine_model: Option<SpineModel>,
    pub editable_mesh: Option<EditableMesh>,
    pub rigged_model: Option<RiggedModel>,

    /// Cached mesh from spine (regenerated when spine changes)
    pub spine_mesh_dirty: bool,

    // File state
    pub current_file: Option<PathBuf>,

    // View/edit state
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

    // Edit state (undo/redo stores context-specific snapshots)
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

    // Box selection state
    pub box_select_active: bool,
    pub box_select_start: (f32, f32),

    // Modal transform state (Blender-style G/S/R)
    pub modal_transform: ModalTransform,
    pub modal_transform_start_mouse: (f32, f32),
    pub modal_transform_start_positions: Vec<Vec3>,
    pub modal_transform_center: Vec3,  // Center point for scale/rotate

    // Rig bone rotation state (for Animate mode)
    pub rig_bone_rotating: bool,
    pub rig_bone_start_rotations: Vec<Vec3>,  // Original local_rotation values
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
            // Mode hierarchy - start in Mesh context, Edit mode (like Blender)
            interaction_mode: InteractionMode::Edit,
            data_context: DataContext::Mesh,
            rig_sub_mode: RigSubMode::Skeleton,

            // Start with a cube (standard starting shape like Blender)
            spine_model: None,
            editable_mesh: Some(EditableMesh::cube(50.0)),
            rigged_model: None,
            spine_mesh_dirty: false,

            current_file: None,

            select_mode: SelectMode::Vertex,
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

            box_select_active: false,
            box_select_start: (0.0, 0.0),

            modal_transform: ModalTransform::None,
            modal_transform_start_mouse: (0.0, 0.0),
            modal_transform_start_positions: Vec::new(),
            modal_transform_center: Vec3::ZERO,

            rig_bone_rotating: false,
            rig_bone_start_rotations: Vec::new(),
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

    /// Toggle interaction mode (Object <-> Edit)
    pub fn toggle_interaction_mode(&mut self) {
        self.interaction_mode = self.interaction_mode.toggle();
        self.selection.clear();
        self.set_status(&format!("{} Mode", self.interaction_mode.label()), 1.0);
    }

    /// Switch data context
    pub fn set_data_context(&mut self, context: DataContext) {
        if self.data_context != context {
            self.data_context = context;
            self.selection.clear();
            // Update select mode to a valid one for this context
            let valid_modes = SelectMode::valid_modes(context, self.interaction_mode);
            if !valid_modes.is_empty() {
                self.select_mode = valid_modes[0];
            }
            self.set_status(&format!("Context: {}", context.label()), 1.0);
        }
    }

    /// Switch rig sub-mode (only relevant in Rig context)
    pub fn set_rig_sub_mode(&mut self, sub_mode: RigSubMode) {
        if self.data_context == DataContext::Rig && self.rig_sub_mode != sub_mode {
            self.rig_sub_mode = sub_mode;
            self.selection.clear();
            self.set_status(&format!("Rig: {}", sub_mode.label()), 1.0);
        }
    }

    /// Cycle to next data context
    pub fn next_context(&mut self) {
        let next = (self.data_context.index() + 1) % DataContext::ALL.len();
        if let Some(context) = DataContext::from_index(next) {
            self.set_data_context(context);
        }
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

    // ========================================================================
    // Animation Keyframe Methods
    // ========================================================================

    /// Insert a keyframe at the current frame with current bone poses
    pub fn insert_keyframe(&mut self) {
        if let Some(rig) = &mut self.rigged_model {
            if rig.skeleton.is_empty() {
                self.set_status("No bones to keyframe", 1.0);
                return;
            }
            if self.current_animation >= rig.animations.len() {
                self.set_status("No animation selected", 1.0);
                return;
            }

            let num_bones = rig.skeleton.len();

            // Create keyframe from current bone transforms
            let mut kf = Keyframe::new(self.current_frame, num_bones);
            for (i, bone) in rig.skeleton.iter().enumerate() {
                kf.transforms[i] = BoneTransform::new(bone.local_position, bone.local_rotation);
            }

            rig.animations[self.current_animation].set_keyframe(kf);
            self.set_status(&format!("Keyframe inserted at frame {}", self.current_frame), 1.0);
            self.dirty = true;
        } else {
            self.set_status("No rigged model", 1.0);
        }
    }

    /// Delete keyframe at current frame
    pub fn delete_keyframe(&mut self) {
        if let Some(rig) = &mut self.rigged_model {
            if self.current_animation >= rig.animations.len() {
                return;
            }
            let anim = &mut rig.animations[self.current_animation];
            if anim.get_keyframe(self.current_frame).is_some() {
                anim.remove_keyframe(self.current_frame);
                self.set_status(&format!("Keyframe deleted at frame {}", self.current_frame), 1.0);
                self.dirty = true;
            } else {
                self.set_status("No keyframe at current frame", 1.0);
            }
        }
    }

    /// Apply interpolated pose from animation at current frame
    pub fn apply_animation_pose(&mut self) {
        // Need to get animation data first, then apply to skeleton
        let pose_data = if let Some(rig) = &self.rigged_model {
            if rig.animations.is_empty() || rig.skeleton.is_empty() {
                return;
            }
            if self.current_animation >= rig.animations.len() {
                return;
            }

            let anim = &rig.animations[self.current_animation];
            if anim.keyframes.is_empty() {
                return;
            }

            // Find surrounding keyframes
            let frame = self.current_frame;
            let mut prev_kf: Option<&Keyframe> = None;
            let mut next_kf: Option<&Keyframe> = None;

            for kf in &anim.keyframes {
                if kf.frame <= frame {
                    prev_kf = Some(kf);
                } else if next_kf.is_none() {
                    next_kf = Some(kf);
                    break;
                }
            }

            // Calculate interpolated transforms
            let num_bones = rig.skeleton.len();
            let mut result: Vec<Option<Vec3>> = vec![None; num_bones];

            match (prev_kf, next_kf) {
                (Some(prev), Some(next)) if next.frame > prev.frame => {
                    let t = (frame - prev.frame) as f32 / (next.frame - prev.frame) as f32;
                    for i in 0..num_bones {
                        if i < prev.transforms.len() && i < next.transforms.len() {
                            let interp = prev.transforms[i].lerp(&next.transforms[i], t);
                            result[i] = Some(interp.rotation);
                        }
                    }
                }
                (Some(kf), None) | (None, Some(kf)) => {
                    for i in 0..num_bones {
                        if i < kf.transforms.len() {
                            result[i] = Some(kf.transforms[i].rotation);
                        }
                    }
                }
                (Some(prev), Some(_next)) => {
                    // Same frame or invalid range, use prev
                    for i in 0..num_bones {
                        if i < prev.transforms.len() {
                            result[i] = Some(prev.transforms[i].rotation);
                        }
                    }
                }
                (None, None) => {}
            }

            Some(result)
        } else {
            None
        };

        // Apply the calculated pose
        if let (Some(pose), Some(rig)) = (pose_data, &mut self.rigged_model) {
            for (i, maybe_rot) in pose.into_iter().enumerate() {
                if let Some(rot) = maybe_rot {
                    if i < rig.skeleton.len() {
                        rig.skeleton[i].local_rotation = rot;
                    }
                }
            }
        }
    }

    /// Check if there's a keyframe at the current frame
    pub fn has_keyframe_at_current_frame(&self) -> bool {
        if let Some(rig) = &self.rigged_model {
            if self.current_animation < rig.animations.len() {
                return rig.animations[self.current_animation]
                    .get_keyframe(self.current_frame)
                    .is_some();
            }
        }
        false
    }

    /// Get keyframe frames for current animation (for timeline display)
    pub fn get_keyframe_frames(&self) -> Vec<u32> {
        if let Some(rig) = &self.rigged_model {
            if self.current_animation < rig.animations.len() {
                return rig.animations[self.current_animation]
                    .keyframes
                    .iter()
                    .map(|kf| kf.frame)
                    .collect();
            }
        }
        Vec::new()
    }

    /// Get last frame of current animation
    pub fn get_animation_last_frame(&self) -> u32 {
        if let Some(rig) = &self.rigged_model {
            if self.current_animation < rig.animations.len() {
                return rig.animations[self.current_animation].last_frame();
            }
        }
        60 // Default if no animation
    }

    /// Get FPS of current animation
    pub fn get_animation_fps(&self) -> u8 {
        if let Some(rig) = &self.rigged_model {
            if self.current_animation < rig.animations.len() {
                return rig.animations[self.current_animation].fps;
            }
        }
        15 // Default
    }

    /// Check if current animation loops
    pub fn is_animation_looping(&self) -> bool {
        if let Some(rig) = &self.rigged_model {
            if self.current_animation < rig.animations.len() {
                return rig.animations[self.current_animation].looping;
            }
        }
        true
    }
}

impl Default for ModelerState {
    fn default() -> Self {
        Self::new()
    }
}
