//! Modeler editor state
//!
//! PicoCAD-inspired design:
//! - 4-panel viewport layout (3D perspective + top/front/side ortho views)
//! - Face-centric workflow with grid snapping
//! - Simple keyboard shortcuts (E=extrude, R/T=rotate, V=toggle view mode)

use std::path::PathBuf;
use serde::{Deserialize, Serialize};
use crate::rasterizer::{Camera, Vec2, Vec3, Color, RasterSettings, BlendMode};
use super::mesh_editor::{EditableMesh, MeshProject, MeshObject, TextureAtlas};
use super::model::Animation;
use super::drag::DragManager;

// ============================================================================
// PicoCAD-Style Viewport System
// ============================================================================

/// Which viewport panel (for 4-panel layout)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ViewportId {
    /// Top-left: 3D perspective view
    #[default]
    Perspective,
    /// Top-right: Top-down view (XZ plane, looking down Y)
    Top,
    /// Bottom-left: Front view (XY plane, looking down Z)
    Front,
    /// Bottom-right: Side view (YZ plane, looking down X)
    Side,
}

impl ViewportId {
    pub const ALL: [ViewportId; 4] = [
        ViewportId::Perspective,
        ViewportId::Top,
        ViewportId::Front,
        ViewportId::Side,
    ];

    pub fn label(&self) -> &'static str {
        match self {
            ViewportId::Perspective => "3D",
            ViewportId::Top => "Top",
            ViewportId::Front => "Front",
            ViewportId::Side => "Side",
        }
    }

    pub fn is_ortho(&self) -> bool {
        !matches!(self, ViewportId::Perspective)
    }
}

/// Camera state for an orthographic viewport
#[derive(Debug, Clone)]
pub struct OrthoCamera {
    /// Zoom level (pixels per world unit)
    pub zoom: f32,
    /// Pan offset in world units (center of view)
    pub center: Vec2,
}

impl Default for OrthoCamera {
    fn default() -> Self {
        Self {
            zoom: 2.0,
            center: Vec2::new(0.0, 50.0), // Centered, slightly elevated
        }
    }
}

/// Build mode vs Texture mode (V key to toggle, like PicoCAD)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ViewMode {
    /// Edit geometry (vertices, faces, extrude)
    #[default]
    Build,
    /// Edit UVs and textures
    Texture,
}

impl ViewMode {
    pub fn label(&self) -> &'static str {
        match self {
            ViewMode::Build => "Build",
            ViewMode::Texture => "Texture",
        }
    }

    pub fn toggle(&self) -> Self {
        match self {
            ViewMode::Build => ViewMode::Texture,
            ViewMode::Texture => ViewMode::Build,
        }
    }
}

// ============================================================================
// Edit Mode (simplified from Blender-style)
// ============================================================================

/// Object vs Edit mode (Tab to toggle)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum InteractionMode {
    /// Select whole meshes
    Object,
    /// Edit vertices/faces
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

/// Sub-mode for rigging/animation (future use)
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
// Selection Modes (PicoCAD-style: Vertex/Edge/Face)
// ============================================================================

/// Selection mode - what type of element to select
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SelectMode {
    Vertex,
    Edge,
    #[default]
    Face,  // PicoCAD default: face-centric workflow
}

impl SelectMode {
    pub fn label(&self) -> &'static str {
        match self {
            SelectMode::Vertex => "Vertex",
            SelectMode::Edge => "Edge",
            SelectMode::Face => "Face",
        }
    }

    /// Get valid select modes for interaction mode
    pub fn valid_modes(interaction: InteractionMode) -> Vec<SelectMode> {
        match interaction {
            InteractionMode::Object => vec![], // Whole mesh selected
            InteractionMode::Edit => vec![SelectMode::Vertex, SelectMode::Edge, SelectMode::Face],
        }
    }
}

/// Current selection in the modeler
#[derive(Debug, Clone, Default)]
pub enum ModelerSelection {
    #[default]
    None,
    /// Object mode: whole mesh selected
    Mesh,
    /// Edit mode: selected vertices
    Vertices(Vec<usize>),
    /// Edit mode: selected edges (v0_index, v1_index)
    Edges(Vec<(usize, usize)>),
    /// Edit mode: selected faces
    Faces(Vec<usize>),
}

impl ModelerSelection {
    pub fn is_empty(&self) -> bool {
        match self {
            ModelerSelection::None => true,
            ModelerSelection::Mesh => false,
            ModelerSelection::Vertices(v) => v.is_empty(),
            ModelerSelection::Edges(v) => v.is_empty(),
            ModelerSelection::Faces(v) => v.is_empty(),
        }
    }

    pub fn clear(&mut self) {
        *self = ModelerSelection::None;
    }

    /// Get selected vertices if any
    pub fn vertices(&self) -> Option<&[usize]> {
        match self {
            ModelerSelection::Vertices(verts) => Some(verts),
            _ => None,
        }
    }

    /// Get selected edges if any
    pub fn edges(&self) -> Option<&[(usize, usize)]> {
        match self {
            ModelerSelection::Edges(edges) => Some(edges),
            _ => None,
        }
    }

    /// Get selected faces if any
    pub fn faces(&self) -> Option<&[usize]> {
        match self {
            ModelerSelection::Faces(faces) => Some(faces),
            _ => None,
        }
    }

    /// Get all unique vertex indices affected by this selection
    /// For edges, returns both vertices of each edge
    /// For faces, returns all vertices of each face
    pub fn get_affected_vertex_indices(&self, mesh: &EditableMesh) -> Vec<usize> {
        match self {
            ModelerSelection::None | ModelerSelection::Mesh => Vec::new(),
            ModelerSelection::Vertices(verts) => verts.clone(),
            ModelerSelection::Edges(edges) => {
                let mut indices: Vec<usize> = edges.iter()
                    .flat_map(|(v0, v1)| [*v0, *v1])
                    .collect();
                indices.sort();
                indices.dedup();
                indices
            }
            ModelerSelection::Faces(faces) => {
                let mut indices: Vec<usize> = faces.iter()
                    .filter_map(|&face_idx| mesh.faces.get(face_idx))
                    .flat_map(|face| [face.v0, face.v1, face.v2])
                    .collect();
                indices.sort();
                indices.dedup();
                indices
            }
        }
    }

    /// Compute the center point of the selection (average of all affected vertex positions)
    pub fn compute_center(&self, mesh: &EditableMesh) -> Option<Vec3> {
        let indices = self.get_affected_vertex_indices(mesh);
        if indices.is_empty() {
            return None;
        }
        let sum: Vec3 = indices.iter()
            .filter_map(|&idx| mesh.vertices.get(idx).map(|v| v.pos))
            .fold(Vec3::ZERO, |acc, pos| acc + pos);
        Some(sum * (1.0 / indices.len() as f32))
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

/// Brush type for texture painting
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BrushType {
    Square,
    Fill,
}

/// Atlas editing mode - UV vertex editing vs painting
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AtlasEditMode {
    /// Move UV vertices
    #[default]
    Uv,
    /// Paint on the texture atlas
    Paint,
}

impl AtlasEditMode {
    pub fn toggle(&self) -> Self {
        match self {
            AtlasEditMode::Uv => AtlasEditMode::Paint,
            AtlasEditMode::Paint => AtlasEditMode::Uv,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            AtlasEditMode::Uv => "UV",
            AtlasEditMode::Paint => "Paint",
        }
    }
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
    // Edit mode
    pub interaction_mode: InteractionMode,

    // The mesh being edited (legacy single mesh mode)
    pub mesh: EditableMesh,

    // PicoCAD-style project with multiple objects and texture atlas
    pub project: MeshProject,

    // File state
    pub current_file: Option<PathBuf>,

    // View/edit state
    pub select_mode: SelectMode,
    pub tool: TransformTool,
    pub selection: ModelerSelection,

    // Camera (orbit mode for perspective view)
    pub camera: Camera,
    pub raster_settings: RasterSettings,
    pub orbit_target: Vec3,      // Point the camera orbits around
    pub orbit_distance: f32,     // Distance from target
    pub orbit_azimuth: f32,      // Horizontal angle (radians)
    pub orbit_elevation: f32,    // Vertical angle (radians)

    // PicoCAD-style 4-panel viewport system
    pub view_mode: ViewMode,           // Build vs Texture (V to toggle)
    pub active_viewport: ViewportId,   // Which viewport has focus
    pub fullscreen_viewport: Option<ViewportId>,  // Space to toggle fullscreen
    pub ortho_top: OrthoCamera,        // Top view camera (XZ plane)
    pub ortho_front: OrthoCamera,      // Front view camera (XY plane)
    pub ortho_side: OrthoCamera,       // Side view camera (YZ plane)

    // Resizable panel splits (0.0-1.0 ratios)
    pub viewport_h_split: f32,         // Horizontal divider (left/right, default 0.5)
    pub viewport_v_split: f32,         // Vertical divider (top/bottom, default 0.5)

    // UV Editor state
    pub uv_zoom: f32,
    pub uv_offset: Vec2,
    pub uv_selection: Vec<usize>,
    pub uv_drag_active: bool,
    pub uv_drag_start: (f32, f32),
    pub uv_drag_start_uvs: Vec<(usize, usize, Vec2)>, // (object_idx, vertex_idx, original_uv)

    // Paint state
    pub paint_color: Color,
    pub paint_blend_mode: BlendMode,
    pub brush_size: f32,
    pub brush_type: BrushType,
    pub paint_mode: PaintMode,
    pub atlas_edit_mode: AtlasEditMode, // UV editing vs painting (V to toggle)
    pub color_picker_slider: Option<usize>, // Active slider in color picker (0=R, 1=G, 2=B)
    pub brush_size_slider_active: bool, // True while dragging brush size slider
    pub paint_stroke_active: bool, // True while painting (for undo grouping)

    // Vertex linking: when true, move coincident vertices together
    pub vertex_linking: bool,

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

    // Undo/Redo system
    pub undo_stack: Vec<UndoState>,
    pub redo_stack: Vec<UndoState>,
    pub max_undo_levels: usize,

    // Transform state (for mouse drag)
    pub transform_active: bool,
    pub transform_start_mouse: (f32, f32),
    pub transform_start_positions: Vec<Vec3>,
    pub transform_start_rotations: Vec<Vec3>,
    pub axis_lock: Option<Axis>,

    // Snap/quantization settings
    pub snap_settings: SnapSettings,

    // Viewport mouse state
    pub viewport_last_mouse: (f32, f32),
    pub viewport_mouse_captured: bool,
    /// Which ortho viewport is currently panning (if any)
    pub ortho_pan_viewport: Option<ViewportId>,
    /// Last mouse position for ortho panning (separate from perspective view)
    pub ortho_last_mouse: (f32, f32),

    // Box selection state
    pub box_select_active: bool,
    pub box_select_start: (f32, f32),

    // Hover state (like world editor - auto-detect element under cursor)
    /// Hovered vertex index (highest priority)
    pub hovered_vertex: Option<usize>,
    /// Hovered edge (v0, v1 indices)
    pub hovered_edge: Option<(usize, usize)>,
    /// Hovered face index (lowest priority)
    pub hovered_face: Option<usize>,

    // Gizmo drag state (for move gizmo)
    /// Which gizmo axis is being hovered (for highlighting)
    pub gizmo_hovered_axis: Option<Axis>,
    /// True if currently dragging the gizmo
    pub gizmo_dragging: bool,
    /// Which axis is being dragged
    pub gizmo_drag_axis: Option<Axis>,
    /// Start mouse position when gizmo drag began
    pub gizmo_drag_start_mouse: (f32, f32),
    /// Initial positions of vertices being dragged
    pub gizmo_drag_start_positions: Vec<(usize, Vec3)>,
    /// Selection center when drag started
    pub gizmo_drag_center: Vec3,

    // Modal transform state (G/S/R keys)
    pub modal_transform: ModalTransform,
    pub modal_transform_start_mouse: (f32, f32),
    pub modal_transform_start_positions: Vec<Vec3>,
    pub modal_transform_center: Vec3,

    // Context menu state
    pub context_menu: Option<ContextMenu>,

    // Unified drag manager (new system - replaces scattered gizmo_drag_* fields)
    pub drag_manager: DragManager,
}

/// Context menu for right-click actions
#[derive(Debug, Clone)]
pub struct ContextMenu {
    /// Screen position of menu
    pub x: f32,
    pub y: f32,
    /// World position where clicked (for placing primitives)
    pub world_pos: Vec3,
    /// Which viewport the menu was opened in
    pub viewport: ViewportId,
}

impl ContextMenu {
    pub fn new(x: f32, y: f32, world_pos: Vec3, viewport: ViewportId) -> Self {
        Self { x, y, world_pos, viewport }
    }
}

/// Snapshot of mesh state for undo/redo
#[derive(Debug, Clone)]
pub struct UndoState {
    pub mesh: EditableMesh,
    pub selection: ModelerSelection,
    pub atlas: Option<TextureAtlas>, // Optional: only saved when atlas changes
    pub description: String,
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

        // Create project first, then derive mesh from it (single source of truth)
        let project = MeshProject::default();
        let mesh = project.selected().map(|o| o.mesh.clone()).unwrap_or_else(EditableMesh::new);

        Self {
            interaction_mode: InteractionMode::Edit,

            // Mesh derived from project's selected object
            mesh,

            // PicoCAD-style project (single source of truth for geometry)
            project,

            current_file: None,

            select_mode: SelectMode::Face, // PicoCAD: face-centric
            tool: TransformTool::Select,
            selection: ModelerSelection::None,

            camera,
            raster_settings: RasterSettings::game(), // Use game settings (no backface wireframe)
            orbit_target,
            orbit_distance,
            orbit_azimuth,
            orbit_elevation,

            // PicoCAD-style viewports
            view_mode: ViewMode::Build,
            active_viewport: ViewportId::Perspective,
            fullscreen_viewport: None,
            ortho_top: OrthoCamera::default(),
            ortho_front: OrthoCamera::default(),
            ortho_side: OrthoCamera::default(),

            // Resizable panels (default 50/50 splits)
            viewport_h_split: 0.5,
            viewport_v_split: 0.5,

            uv_zoom: 1.0,
            uv_offset: Vec2::default(),
            uv_selection: Vec::new(),
            uv_drag_active: false,
            uv_drag_start: (0.0, 0.0),
            uv_drag_start_uvs: Vec::new(),

            paint_color: Color::WHITE,
            paint_blend_mode: BlendMode::Opaque,
            brush_size: 4.0,
            brush_type: BrushType::Square,
            paint_mode: PaintMode::Texture,
            atlas_edit_mode: AtlasEditMode::default(),
            color_picker_slider: None,
            brush_size_slider_active: false,
            paint_stroke_active: false,

            vertex_linking: true, // Default on: move coincident vertices together

            hierarchy_expanded: Vec::new(),

            current_animation: 0,
            current_frame: 0,
            playing: false,
            playback_time: 0.0,
            selected_keyframes: Vec::new(),

            dirty: false,
            status_message: None,

            // Undo/Redo system
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            max_undo_levels: 50,

            transform_active: false,
            transform_start_mouse: (0.0, 0.0),
            transform_start_positions: Vec::new(),
            transform_start_rotations: Vec::new(),
            axis_lock: None,

            snap_settings: SnapSettings::default(),

            viewport_last_mouse: (0.0, 0.0),
            viewport_mouse_captured: false,
            ortho_pan_viewport: None,
            ortho_last_mouse: (0.0, 0.0),

            box_select_active: false,
            box_select_start: (0.0, 0.0),

            hovered_vertex: None,
            hovered_edge: None,
            hovered_face: None,

            gizmo_hovered_axis: None,
            gizmo_dragging: false,
            gizmo_drag_axis: None,
            gizmo_drag_start_mouse: (0.0, 0.0),
            gizmo_drag_start_positions: Vec::new(),
            gizmo_drag_center: Vec3::ZERO,

            modal_transform: ModalTransform::None,
            modal_transform_start_mouse: (0.0, 0.0),
            modal_transform_start_positions: Vec::new(),
            modal_transform_center: Vec3::ZERO,

            context_menu: None,

            drag_manager: DragManager::new(),
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

    // ========================================================================
    // PicoCAD-style viewport helpers
    // ========================================================================

    /// Toggle between Build and Texture view mode (V key)
    pub fn toggle_view_mode(&mut self) {
        self.view_mode = self.view_mode.toggle();
        self.set_status(&format!("{} Mode", self.view_mode.label()), 1.0);
    }

    /// Toggle fullscreen for the active viewport (Space key)
    pub fn toggle_fullscreen_viewport(&mut self) {
        if self.fullscreen_viewport.is_some() {
            self.fullscreen_viewport = None;
            self.set_status("Multi-view", 0.5);
        } else {
            self.fullscreen_viewport = Some(self.active_viewport);
            self.set_status(&format!("{} Fullscreen", self.active_viewport.label()), 0.5);
        }
    }

    /// Get the ortho camera for a viewport
    pub fn get_ortho_camera(&self, viewport: ViewportId) -> &OrthoCamera {
        match viewport {
            ViewportId::Top => &self.ortho_top,
            ViewportId::Front => &self.ortho_front,
            ViewportId::Side => &self.ortho_side,
            ViewportId::Perspective => &self.ortho_top, // Fallback (shouldn't happen)
        }
    }

    /// Get mutable ortho camera for a viewport
    pub fn get_ortho_camera_mut(&mut self, viewport: ViewportId) -> &mut OrthoCamera {
        match viewport {
            ViewportId::Top => &mut self.ortho_top,
            ViewportId::Front => &mut self.ortho_front,
            ViewportId::Side => &mut self.ortho_side,
            ViewportId::Perspective => &mut self.ortho_top, // Fallback (shouldn't happen)
        }
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

    // ========================================================================
    // Mesh/Project Sync (to keep state.mesh and state.project in sync)
    // ========================================================================

    /// Sync state.mesh FROM the currently selected project object.
    /// Call after UV edits or when switching objects.
    pub fn sync_mesh_from_project(&mut self) {
        if let Some(obj) = self.project.selected() {
            self.mesh = obj.mesh.clone();
        }
    }

    /// Sync state.mesh TO the currently selected project object.
    /// Call after geometry edits (extrude, move vertices, etc).
    pub fn sync_mesh_to_project(&mut self) {
        if let Some(obj) = self.project.selected_mut() {
            obj.mesh = self.mesh.clone();
        }
    }

    /// Toggle interaction mode (Object <-> Edit)
    pub fn toggle_interaction_mode(&mut self) {
        self.interaction_mode = self.interaction_mode.toggle();
        self.selection.clear();
        self.set_status(&format!("{} Mode", self.interaction_mode.label()), 1.0);
    }

    /// Create a new mesh (replaces current)
    pub fn new_mesh(&mut self) {
        self.project = MeshProject::default();
        self.sync_mesh_from_project(); // Derive mesh from project
        self.current_file = None;
        self.selection.clear();
        self.dirty = false;
        self.set_status("New mesh", 1.0);
    }

    /// Save project to file (includes mesh + texture atlas)
    #[cfg(not(target_arch = "wasm32"))]
    pub fn save_project(&mut self, path: &std::path::Path) -> Result<(), String> {
        // Sync current mesh to project before saving
        self.sync_mesh_to_project();

        self.project.save_to_file(path)
            .map_err(|e| format!("{}", e))?;
        self.current_file = Some(path.to_path_buf());
        self.dirty = false;
        self.set_status(&format!("Saved: {}", path.display()), 2.0);
        Ok(())
    }

    /// Load project from file (includes mesh + texture atlas)
    #[cfg(not(target_arch = "wasm32"))]
    pub fn load_project(&mut self, path: &std::path::Path) -> Result<(), String> {
        let project = MeshProject::load_from_file(path)
            .map_err(|e| format!("{}", e))?;
        self.project = project;
        self.sync_mesh_from_project();
        self.current_file = Some(path.to_path_buf());
        self.selection.clear();
        self.dirty = false;
        self.set_status(&format!("Loaded: {}", path.display()), 2.0);
        Ok(())
    }

    // ========================================================================
    // PicoCAD-style Project Helpers
    // ========================================================================

    /// Get the texture atlas
    pub fn atlas(&self) -> &TextureAtlas {
        &self.project.atlas
    }

    /// Get mutable texture atlas
    pub fn atlas_mut(&mut self) -> &mut TextureAtlas {
        &mut self.project.atlas
    }

    /// Get all visible mesh objects
    pub fn visible_objects(&self) -> impl Iterator<Item = (usize, &MeshObject)> {
        self.project.objects.iter().enumerate().filter(|(_, o)| o.visible)
    }

    /// Get the currently selected object index
    pub fn selected_object_index(&self) -> Option<usize> {
        self.project.selected_object
    }

    /// Select an object by index
    pub fn select_object(&mut self, index: usize) {
        if index < self.project.objects.len() {
            self.project.selected_object = Some(index);
            self.selection.clear();
            if let Some(obj) = self.project.objects.get(index) {
                self.set_status(&format!("Selected: {}", obj.name), 0.5);
            }
        }
    }

    /// Get the currently selected mesh object
    pub fn selected_object(&self) -> Option<&MeshObject> {
        self.project.selected()
    }

    /// Get the currently selected mesh object mutably
    pub fn selected_object_mut(&mut self) -> Option<&mut MeshObject> {
        self.project.selected_mut()
    }

    /// Add a new object to the project
    pub fn add_object(&mut self, obj: MeshObject) -> usize {
        let idx = self.project.add_object(obj);
        self.project.selected_object = Some(idx);
        self.dirty = true;
        idx
    }

    /// Create a new project (replaces current)
    pub fn new_project(&mut self) {
        self.project = MeshProject::default();
        self.mesh = EditableMesh::cube(50.0);
        self.current_file = None;
        self.selection.clear();
        self.dirty = false;
        self.set_status("New project", 1.0);
    }

    // ========================================================================
    // Undo/Redo System
    // ========================================================================

    /// Save current state to undo stack before making a change
    pub fn push_undo(&mut self, description: &str) {
        let state = UndoState {
            mesh: self.mesh.clone(),
            selection: self.selection.clone(),
            atlas: None, // Don't save atlas for mesh-only changes
            description: description.to_string(),
        };
        self.undo_stack.push(state);

        // Limit stack size
        while self.undo_stack.len() > self.max_undo_levels {
            self.undo_stack.remove(0);
        }

        // Clear redo stack when new action is performed
        self.redo_stack.clear();
    }

    /// Save current state including texture atlas to undo stack (for paint operations)
    pub fn push_undo_with_atlas(&mut self, description: &str) {
        let state = UndoState {
            mesh: self.mesh.clone(),
            selection: self.selection.clone(),
            atlas: Some(self.project.atlas.clone()),
            description: description.to_string(),
        };
        self.undo_stack.push(state);

        // Limit stack size
        while self.undo_stack.len() > self.max_undo_levels {
            self.undo_stack.remove(0);
        }

        // Clear redo stack when new action is performed
        self.redo_stack.clear();
    }

    /// Undo the last action (Ctrl+Z)
    pub fn undo(&mut self) -> bool {
        if let Some(undo_state) = self.undo_stack.pop() {
            // Save current state to redo stack (include atlas if the undo state had one)
            let redo_state = UndoState {
                mesh: self.mesh.clone(),
                selection: self.selection.clone(),
                atlas: if undo_state.atlas.is_some() { Some(self.project.atlas.clone()) } else { None },
                description: undo_state.description.clone(),
            };
            self.redo_stack.push(redo_state);

            // Restore the undo state
            self.mesh = undo_state.mesh;
            self.selection = undo_state.selection;
            if let Some(atlas) = undo_state.atlas {
                self.project.atlas = atlas;
            }
            self.sync_mesh_to_project(); // Keep project in sync
            self.dirty = true;
            self.set_status(&format!("Undo: {}", undo_state.description), 1.0);
            true
        } else {
            self.set_status("Nothing to undo", 1.0);
            false
        }
    }

    /// Redo the last undone action (Ctrl+Shift+Z or Ctrl+Y)
    pub fn redo(&mut self) -> bool {
        if let Some(redo_state) = self.redo_stack.pop() {
            // Save current state to undo stack (include atlas if the redo state had one)
            let undo_state = UndoState {
                mesh: self.mesh.clone(),
                selection: self.selection.clone(),
                atlas: if redo_state.atlas.is_some() { Some(self.project.atlas.clone()) } else { None },
                description: redo_state.description.clone(),
            };
            self.undo_stack.push(undo_state);

            // Restore the redo state
            self.mesh = redo_state.mesh;
            self.selection = redo_state.selection;
            if let Some(atlas) = redo_state.atlas {
                self.project.atlas = atlas;
            }
            self.sync_mesh_to_project(); // Keep project in sync
            self.dirty = true;
            self.set_status(&format!("Redo: {}", redo_state.description), 1.0);
            true
        } else {
            self.set_status("Nothing to redo", 1.0);
            false
        }
    }

    /// Check if undo is available
    pub fn can_undo(&self) -> bool {
        !self.undo_stack.is_empty()
    }

    /// Check if redo is available
    pub fn can_redo(&self) -> bool {
        !self.redo_stack.is_empty()
    }
}

impl Default for ModelerState {
    fn default() -> Self {
        Self::new()
    }
}
