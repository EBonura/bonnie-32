//! Modeler editor state
//!
//! PicoCAD-inspired design:
//! - 4-panel viewport layout (3D perspective + top/front/side ortho views)
//! - Face-centric workflow with grid snapping
//! - Simple keyboard shortcuts (E=extrude, R/T=rotate, V=toggle view mode)

use std::path::PathBuf;
use serde::{Deserialize, Serialize};
use crate::rasterizer::{Camera, Vec2, Vec3, Color, RasterSettings, BlendMode, Color15, Clut, ClutId};
use crate::texture::{TextureLibrary, TextureEditorState, UserTexture};
use crate::asset::Asset;
use crate::ui::{TextInputState, DropdownState};
use super::mesh_editor::{
    EditableMesh, MeshPart, IndexedAtlas, EditFace, TextureRef, ClutPool,
    checkerboard_atlas, checkerboard_clut,
};
use super::model::Animation;
use super::drag::DragManager;
use super::tools::ModelerToolBox;

// ============================================================================
// Math Helpers
// ============================================================================

/// Rotate a vector by euler angles (X rotation = pitch, Z rotation = yaw)
/// Matches spherical coordinate convention used in get_bone_tip_position:
/// direction = (sin(z)*cos(x), cos(z)*cos(x), -sin(x))
/// Order: X rotation (pitch) first, then Z rotation (yaw)
pub fn rotate_by_euler(v: Vec3, rotation: Vec3) -> Vec3 {
    if rotation.x.abs() < 0.001 && rotation.z.abs() < 0.001 {
        return v;  // No rotation
    }

    let rad_x = rotation.x.to_radians();
    let rad_z = rotation.z.to_radians();

    let cos_x = rad_x.cos();
    let sin_x = rad_x.sin();
    let cos_z = rad_z.cos();
    let sin_z = rad_z.sin();

    // Apply X rotation (pitch) FIRST - tilts Y toward negative Z
    let x1 = v.x;
    let y1 = v.y * cos_x + v.z * sin_x;
    let z1 = -v.y * sin_x + v.z * cos_x;

    // Apply Z rotation (yaw) SECOND - turns Y toward positive X
    let x2 = x1 * cos_z + y1 * sin_z;
    let y2 = -x1 * sin_z + y1 * cos_z;
    let z2 = z1;

    Vec3::new(x2, y2, z2)
}

/// Inverse rotation by euler angles (undoes rotate_by_euler)
/// Order: inverse of X-then-Z is (-Z)-then-(-X)
pub fn inverse_rotate_by_euler(v: Vec3, rotation: Vec3) -> Vec3 {
    if rotation.x.abs() < 0.001 && rotation.z.abs() < 0.001 {
        return v;  // No rotation
    }

    let rad_x = rotation.x.to_radians();
    let rad_z = rotation.z.to_radians();

    let cos_x = rad_x.cos();
    let sin_x = rad_x.sin();
    let cos_z = rad_z.cos();
    let sin_z = rad_z.sin();

    // Apply inverse Z rotation FIRST (undo the second rotation)
    let x1 = v.x * cos_z - v.y * sin_z;
    let y1 = v.x * sin_z + v.y * cos_z;
    let z1 = v.z;

    // Apply inverse X rotation SECOND (undo the first rotation)
    let x2 = x1;
    let y2 = y1 * cos_x - z1 * sin_x;
    let z2 = y1 * sin_x + z1 * cos_x;

    Vec3::new(x2, y2, z2)
}

// ============================================================================
// Resolved Texture
// ============================================================================

/// Resolved texture data from a TextureRef
///
/// This enum provides access to the actual texture data after resolving
/// a TextureRef through the texture library or embedded data.
#[derive(Debug)]
pub enum ResolvedTexture<'a> {
    /// Static code-generated texture (checkerboard)
    Static {
        atlas: &'static IndexedAtlas,
        clut: &'static Clut,
    },
    /// User texture from the library
    UserTexture(&'a UserTexture),
    /// Embedded texture data (from OBJ imports)
    Embedded {
        atlas: &'a IndexedAtlas,
        clut: &'a Clut,
    },
}

// ============================================================================
// Camera Mode
// ============================================================================

/// Camera mode for 3D viewport
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CameraMode {
    #[default]
    Free,   // WASD + mouse look (FPS style)
    Orbit,  // Rotate around target point
}

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

/// Which panel has keyboard focus (for routing shortcuts)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ActivePanel {
    #[default]
    Viewport,       // One of the 4 viewports (WASD camera, selection shortcuts)
    TextureEditor,  // Texture/UV editor panel
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
            // Scale: 1024 units = 1 meter
            zoom: 0.1, // Zoomed out more for larger scale
            center: Vec2::new(0.0, 1024.0), // Centered at 1 meter height
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
    pub parts: Vec<RigPart>,
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
            parts: vec![RigPart {
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
    /// Visual width of bone at its widest point (0 = auto from length)
    #[serde(default)]
    pub width: f32,
}

impl RigBone {
    /// Default visual width for a bone
    pub const DEFAULT_WIDTH: f32 = 40.0;

    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            parent: None,
            local_position: Vec3::ZERO,
            local_rotation: Vec3::ZERO,
            length: 20.0,
            width: Self::DEFAULT_WIDTH,
        }
    }

    pub fn with_parent(name: &str, parent: usize) -> Self {
        Self {
            name: name.to_string(),
            parent: Some(parent),
            local_position: Vec3::ZERO,
            local_rotation: Vec3::ZERO,
            length: 20.0,
            width: Self::DEFAULT_WIDTH,
        }
    }

    /// Get the display width for this bone.
    /// If width is 0 (legacy/auto), falls back to length-proportional calculation.
    pub fn display_width(&self) -> f32 {
        if self.width > 0.0 {
            self.width
        } else {
            (self.length * 0.15).clamp(20.0, 200.0)
        }
    }
}

/// A mesh piece that moves 100% with its bone (PS1-style rigid binding)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RigPart {
    pub name: String,
    /// Which bone this part follows (None = unassigned)
    pub bone_index: Option<usize>,
    /// The geometry (in bone-local space when assigned)
    pub mesh: EditableMesh,
    /// Local pivot point
    pub pivot: Vec3,
}

/// State for creating a new bone via click-drag in the viewport
#[derive(Debug, Clone)]
pub struct BoneCreationState {
    /// Parent bone index (None = creating root bone)
    pub parent: Option<usize>,
    /// World position where the drag started (bone base)
    pub start_pos: Vec3,
    /// Current world position of the drag (bone tip)
    pub end_pos: Vec3,
    /// Offset from mouse world position to actual tip (tip_pos - initial_world_pos)
    /// Used to prevent snapping when starting a drag
    pub drag_offset: Vec3,
}

/// State for dragging the component opacity slider
#[derive(Debug, Clone, Copy)]
pub struct OpacityDrag {
    /// Which component is being adjusted
    pub component_idx: usize,
    /// Starting mouse Y position
    pub start_y: f32,
    /// Opacity value when drag started
    pub start_opacity: u8,
    /// X position to draw the vertical slider popup
    pub popup_x: f32,
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
#[derive(Debug, Clone, Default, PartialEq)]
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
    /// Skeleton mode: selected bone bases (G moves whole bone)
    Bones(Vec<usize>),
    /// Skeleton mode: selected bone tips (G changes direction/length)
    BoneTips(Vec<usize>),
}

impl ModelerSelection {
    pub fn is_empty(&self) -> bool {
        match self {
            ModelerSelection::None => true,
            ModelerSelection::Mesh => false,
            ModelerSelection::Vertices(v) => v.is_empty(),
            ModelerSelection::Edges(v) => v.is_empty(),
            ModelerSelection::Faces(v) => v.is_empty(),
            ModelerSelection::Bones(v) => v.is_empty(),
            ModelerSelection::BoneTips(v) => v.is_empty(),
        }
    }

    /// Get selected bone bases if any
    pub fn bones(&self) -> Option<&[usize]> {
        match self {
            ModelerSelection::Bones(bones) => Some(bones),
            _ => None,
        }
    }

    /// Get selected bone tips if any
    pub fn bone_tips(&self) -> Option<&[usize]> {
        match self {
            ModelerSelection::BoneTips(tips) => Some(tips),
            _ => None,
        }
    }

    /// Check if this is a bone base or tip selection
    pub fn is_bone_selection(&self) -> bool {
        matches!(self, ModelerSelection::Bones(_) | ModelerSelection::BoneTips(_))
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
    /// For bones/tips, returns empty (bones don't affect mesh vertices)
    pub fn get_affected_vertex_indices(&self, mesh: &EditableMesh) -> Vec<usize> {
        match self {
            ModelerSelection::None | ModelerSelection::Mesh | ModelerSelection::Bones(_) | ModelerSelection::BoneTips(_) => Vec::new(),
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
                    .flat_map(|face| face.vertices.clone())
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

/// Transform orientation mode (like Blender's orientation modes)
/// Determines how the gizmo is oriented and how movements are applied
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TransformOrientation {
    /// Gizmo aligned to world XYZ axes
    #[default]
    Global,
    /// Gizmo aligned to selection normal (face normal, bone direction, etc.)
    Local,
}

impl TransformOrientation {
    pub fn toggle(&self) -> Self {
        match self {
            TransformOrientation::Global => TransformOrientation::Local,
            TransformOrientation::Local => TransformOrientation::Global,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            TransformOrientation::Global => "Global",
            TransformOrientation::Local => "Local",
        }
    }

    pub fn icon(&self) -> char {
        match self {
            TransformOrientation::Global => '🌐',  // Globe for global
            TransformOrientation::Local => '📐',   // Triangle ruler for local
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

/// UV modal transform mode (G/S/R for UV editing)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum UvModalTransform {
    #[default]
    None,
    Grab,   // G key - move UV selection
    Scale,  // S key - scale UV selection
    Rotate, // R key - rotate UV selection
}

impl UvModalTransform {
    pub fn label(&self) -> &'static str {
        match self {
            UvModalTransform::None => "",
            UvModalTransform::Grab => "UV Grab",
            UvModalTransform::Scale => "UV Scale",
            UvModalTransform::Rotate => "UV Rotate",
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

// Note: AtlasEditMode removed - replaced with collapsible paint section
// and tab-based mode switching in TextureEditorState

/// Axis constraint for transforms
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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
            grid_size: 128.0,  // 128 units = 1/8 of SECTOR_SIZE (1024)
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

/// Mirror editing settings
/// When enabled, only one side of the mesh is editable; the other side is auto-generated.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct MirrorSettings {
    pub enabled: bool,
    pub axis: Axis,
    /// Vertices within this distance of the mirror plane are considered "center" vertices
    /// and will be constrained to the plane during editing
    pub threshold: f32,
}

impl Default for MirrorSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            axis: Axis::X,
            threshold: 1.0,  // 1 world unit
        }
    }
}

impl MirrorSettings {
    /// Check if a position is on the editable side (positive side of the axis)
    pub fn is_editable_side(&self, pos: Vec3) -> bool {
        if !self.enabled {
            return true;
        }
        match self.axis {
            Axis::X => pos.x >= -self.threshold,
            Axis::Y => pos.y >= -self.threshold,
            Axis::Z => pos.z >= -self.threshold,
        }
    }

    /// Check if a position is on the mirror plane (center vertex)
    pub fn is_on_plane(&self, pos: Vec3) -> bool {
        match self.axis {
            Axis::X => pos.x.abs() <= self.threshold,
            Axis::Y => pos.y.abs() <= self.threshold,
            Axis::Z => pos.z.abs() <= self.threshold,
        }
    }

    /// Constrain a position to the mirror plane if it's a center vertex
    pub fn constrain_to_plane(&self, pos: Vec3) -> Vec3 {
        if !self.enabled {
            return pos;
        }
        if self.is_on_plane(pos) {
            match self.axis {
                Axis::X => Vec3::new(0.0, pos.y, pos.z),
                Axis::Y => Vec3::new(pos.x, 0.0, pos.z),
                Axis::Z => Vec3::new(pos.x, pos.y, 0.0),
            }
        } else {
            pos
        }
    }

    /// Get the mirrored position across the axis
    pub fn mirror_position(&self, pos: Vec3) -> Vec3 {
        match self.axis {
            Axis::X => Vec3::new(-pos.x, pos.y, pos.z),
            Axis::Y => Vec3::new(pos.x, -pos.y, pos.z),
            Axis::Z => Vec3::new(pos.x, pos.y, -pos.z),
        }
    }

    /// Get the mirrored normal across the axis
    pub fn mirror_normal(&self, normal: Vec3) -> Vec3 {
        match self.axis {
            Axis::X => Vec3::new(-normal.x, normal.y, normal.z),
            Axis::Y => Vec3::new(normal.x, -normal.y, normal.z),
            Axis::Z => Vec3::new(normal.x, normal.y, -normal.z),
        }
    }
}

/// Clipboard for copy/paste operations
/// Stores geometry that can be pasted as a new object
#[derive(Clone, Debug, Default)]
pub struct Clipboard {
    /// Copied mesh geometry (centered at origin for easier placement)
    pub mesh: Option<EditableMesh>,
    /// Original center position (for relative paste)
    pub center: Vec3,
}

impl Clipboard {
    /// Copy selected faces from a mesh
    pub fn copy_faces(&mut self, mesh: &EditableMesh, face_indices: &[usize]) {
        use std::collections::{HashMap, HashSet};

        if face_indices.is_empty() {
            self.mesh = None;
            return;
        }

        // Collect all vertices used by selected faces
        let mut used_vertices: HashSet<usize> = HashSet::new();
        for &fi in face_indices {
            if let Some(face) = mesh.faces.get(fi) {
                for &vi in &face.vertices {
                    used_vertices.insert(vi);
                }
            }
        }

        // Build old->new vertex index mapping
        let mut vertex_map: HashMap<usize, usize> = HashMap::new();
        let mut new_vertices: Vec<crate::rasterizer::Vertex> = Vec::new();
        let mut sorted_verts: Vec<usize> = used_vertices.into_iter().collect();
        sorted_verts.sort();

        for old_idx in sorted_verts {
            if let Some(vert) = mesh.vertices.get(old_idx) {
                vertex_map.insert(old_idx, new_vertices.len());
                new_vertices.push(vert.clone());
            }
        }

        // Copy faces with remapped indices
        let mut new_faces: Vec<EditFace> = Vec::new();
        for &fi in face_indices {
            if let Some(face) = mesh.faces.get(fi) {
                let new_verts: Vec<usize> = face.vertices.iter()
                    .filter_map(|&vi| vertex_map.get(&vi).copied())
                    .collect();
                if new_verts.len() == face.vertices.len() {
                    new_faces.push(EditFace {
                        vertices: new_verts,
                        texture_id: face.texture_id,
                        black_transparent: face.black_transparent,
                        blend_mode: face.blend_mode,
                    });
                }
            }
        }

        // Calculate center for the copied geometry
        let mut center = Vec3::ZERO;
        if !new_vertices.is_empty() {
            for v in &new_vertices {
                center = center + v.pos;
            }
            center = center * (1.0 / new_vertices.len() as f32);
        }

        // Center the geometry at origin
        for v in &mut new_vertices {
            v.pos = v.pos - center;
        }

        self.center = center;
        self.mesh = Some(EditableMesh::from_parts(new_vertices, new_faces));
    }

    /// Copy entire mesh (for whole object copy)
    pub fn copy_mesh(&mut self, mesh: &EditableMesh) {
        // Calculate center
        let mut center = Vec3::ZERO;
        if !mesh.vertices.is_empty() {
            for v in &mesh.vertices {
                center = center + v.pos;
            }
            center = center * (1.0 / mesh.vertices.len() as f32);
        }

        // Clone and center at origin
        let mut clone = mesh.clone();
        for v in &mut clone.vertices {
            v.pos = v.pos - center;
        }

        self.center = center;
        self.mesh = Some(clone);
    }

    /// Check if clipboard has content
    pub fn has_content(&self) -> bool {
        self.mesh.is_some()
    }
}

/// Main modeler state
pub struct ModelerState {
    // Edit mode
    pub interaction_mode: InteractionMode,

    // Asset with embedded mesh + components
    // This is the single source of truth for mesh data - access via objects() and objects_mut()
    pub asset: Asset,

    // Selection and CLUT state (moved from MeshProject, not serialized with asset)
    /// Currently selected object index within the mesh component
    pub selected_object: Option<usize>,
    /// Global CLUT pool (shared across all textures)
    pub clut_pool: ClutPool,
    /// Preview CLUT override (for testing palette swaps without changing default)
    pub preview_clut: Option<ClutId>,

    // File state
    pub current_file: Option<PathBuf>,

    // View/edit state
    pub select_mode: SelectMode,
    pub selection: ModelerSelection,
    /// Transform gizmo orientation (Global = world axes, Local = selection normal)
    pub transform_orientation: TransformOrientation,

    // Camera (free or orbit mode for perspective view)
    pub camera: Camera,
    pub camera_mode: CameraMode,
    pub raster_settings: RasterSettings,
    pub orbit_target: Vec3,      // Point the camera orbits around (orbit mode)
    pub orbit_distance: f32,     // Distance from target (orbit mode)
    pub orbit_azimuth: f32,      // Horizontal angle in radians (orbit mode)
    pub orbit_elevation: f32,    // Vertical angle in radians (orbit mode)

    // PicoCAD-style 4-panel viewport system
    pub active_panel: ActivePanel,     // Which panel has keyboard focus
    pub active_viewport: ViewportId,   // Which viewport has focus (within viewport panel)
    pub fullscreen_viewport: Option<ViewportId>,  // Space to toggle fullscreen
    pub ortho_top: OrthoCamera,        // Top view camera (XZ plane)
    pub ortho_front: OrthoCamera,      // Front view camera (XY plane)
    pub ortho_side: OrthoCamera,       // Side view camera (YZ plane)

    // Resizable panel splits (0.0-1.0 ratios)
    pub viewport_h_split: f32,         // Horizontal divider (left/right, default 0.5)
    pub viewport_v_split: f32,         // Vertical divider (top/bottom, default 0.5)
    pub dragging_h_divider: bool,      // True while dragging horizontal divider
    pub dragging_v_divider: bool,      // True while dragging vertical divider

    // Paint state
    pub paint_color: Color,
    pub paint_blend_mode: BlendMode,
    pub brush_size: f32,
    pub brush_type: BrushType,
    pub paint_mode: PaintMode,
    pub color_picker_slider: Option<usize>, // Active slider in color picker (0=R, 1=G, 2=B)
    pub brush_size_slider_active: bool, // True while dragging brush size slider
    pub paint_stroke_active: bool, // True while painting (for undo grouping)

    // Collapsible panel sections
    pub paint_section_expanded: bool, // Paint/texture editor section
    pub paint_texture_scroll: f32,    // Scroll position in paint texture browser
    pub paint_samples_collapsed: bool,  // SAMPLES section collapsed state
    pub paint_user_collapsed: bool,     // MY TEXTURES section collapsed state

    // Component management UI
    pub selected_component: Option<usize>,      // Index in asset.components for editing
    pub components_section_expanded: bool,      // Whether Components section is expanded
    pub properties_section_expanded: bool,      // Whether Properties section is expanded
    pub lights_section_expanded: bool,          // Whether Lights section is expanded
    /// Unified dropdown menu state (replaces individual menu_open/btn_rect fields)
    pub dropdown: DropdownState,
    /// Component opacity levels (0 = fully visible, 7 = hidden)
    /// Auto-grows as components are added
    /// This is the *displayed* opacity — may include auto-dimming for non-selected components
    pub component_opacity: Vec<u8>,
    /// User-intended opacity for each component (before auto-focus dimming)
    /// When auto-focus is active, non-selected components are dimmed beyond their base opacity
    pub base_component_opacity: Vec<u8>,
    /// Active opacity drag (vertical slider interaction)
    pub opacity_drag: Option<OpacityDrag>,
    pub delete_component_dialog: Option<usize>, // Component index pending deletion confirmation

    // Component gizmo dragging (for Light offset, etc.)
    pub component_gizmo_drag_axis: Option<Axis>,      // Which axis is being dragged
    pub component_gizmo_drag_start: Option<(f32, f32)>, // Starting mouse position
    pub component_gizmo_start_offset: [f32; 3],       // Starting offset value
    pub component_gizmo_drag_viewport: Option<ViewportId>, // Which viewport owns the drag

    // CLUT editing state
    pub selected_clut: Option<crate::rasterizer::ClutId>, // Currently selected CLUT in pool
    pub selected_clut_entry: usize,                       // Selected palette index (0-15 or 0-255)
    pub active_palette_index: u8,                         // Palette index for indexed painting
    pub clut_preview_active: bool,                        // Live palette swap preview in viewport
    pub clut_color_slider: Option<usize>,                 // Active slider in CLUT color editor

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

    // Skeleton editing state (TR-style: bones define topology, not animation)
    // Skeleton bones are stored in AssetComponent::Skeleton, accessed via asset.skeleton()
    /// Currently selected bone index
    pub selected_bone: Option<usize>,
    /// Bone base under cursor (for hover highlighting)
    pub hovered_bone: Option<usize>,
    /// Bone tip under cursor (for hover highlighting)
    pub hovered_bone_tip: Option<usize>,
    /// Show bones in viewport (when Skeleton component selected)
    pub show_bones: bool,
    /// Active bone creation drag (start position, parent bone)
    pub bone_creation: Option<BoneCreationState>,
    /// Bone rename mode active
    pub bone_rename_active: bool,
    /// Bone rename text buffer
    pub bone_rename_buffer: String,
    /// Target mesh index for bone picker (dropdown id: "bone_picker")
    pub bone_picker_target_mesh: Option<usize>,

    // Edit state (undo/redo stores context-specific snapshots)
    pub dirty: bool,
    pub status_message: Option<(String, f64)>,

    // Undo/Redo system
    pub undo_stack: Vec<UndoEvent>,
    pub redo_stack: Vec<UndoEvent>,
    pub max_undo_levels: usize,

    // Snap/quantization settings
    pub snap_settings: SnapSettings,

    // Note: Mirror editing is now per-part (in MeshPart.mirror)

    /// Clipboard for copy/paste operations
    pub clipboard: Clipboard,

    /// X-ray mode: see and select through geometry (backface selection enabled)
    pub xray_mode: bool,

    // Viewport mouse state
    pub viewport_last_mouse: (f32, f32),
    pub viewport_mouse_captured: bool,
    /// Track if modifier was held last frame (for macOS stuck key workaround)
    /// When a modifier is released, we can't trust WASD state due to macOS not sending key-up events
    pub modifier_was_held: bool,
    /// Track which movement keys we trust (macOS workaround)
    /// When modifier is released, keys become untrusted until freshly pressed
    pub trusted_movement_keys: [bool; 6], // W, A, S, D, Q, E
    /// Which ortho viewport is currently panning (if any)
    pub ortho_pan_viewport: Option<ViewportId>,
    /// Last mouse position for ortho panning (separate from perspective view)
    pub ortho_last_mouse: (f32, f32),

    // Pending drag start positions (for detecting drag vs click)
    // Actual drag state is in DragManager
    pub box_select_pending_start: Option<(f32, f32)>,
    /// Which viewport started the box select (for unified viewport handling)
    pub box_select_viewport: Option<ViewportId>,
    pub free_drag_pending_start: Option<(f32, f32)>,
    pub ortho_drag_pending_start: Option<(f32, f32)>,
    /// Pending start for ortho box selection (clicked on empty space)
    pub ortho_box_select_pending_start: Option<(f32, f32)>,
    /// Which ortho viewport started box selection
    pub ortho_box_select_viewport: Option<ViewportId>,
    /// Which ortho viewport initiated the current drag (if any)
    pub ortho_drag_viewport: Option<ViewportId>,
    /// Zoom level of the ortho viewport when drag started
    pub ortho_drag_zoom: f32,
    /// Axis constraint captured when gizmo was clicked (for ortho drag)
    pub ortho_drag_axis: Option<crate::ui::drag_tracker::Axis>,

    // Hover state (like world editor - auto-detect element under cursor)
    /// Hovered vertex index (highest priority)
    pub hovered_vertex: Option<usize>,
    /// Hovered edge (v0, v1 indices)
    pub hovered_edge: Option<(usize, usize)>,
    /// Hovered face index (lowest priority)
    pub hovered_face: Option<usize>,

    // Gizmo hover state
    /// Which gizmo axis is being hovered (for highlighting) - perspective view
    pub gizmo_hovered_axis: Option<Axis>,
    /// Which gizmo axis is being hovered in ortho views
    pub ortho_gizmo_hovered_axis: Option<Axis>,
    /// True when gizmo is dragging bone bases (moves local_position)
    pub gizmo_bone_drag: bool,
    /// True when gizmo is dragging bone tips (changes rotation/length)
    pub gizmo_bone_tip_drag: bool,

    // Modal transform state (G/S/R keys) - now uses DragManager for actual transform
    pub modal_transform: ModalTransform,

    // Context menu state (legacy)
    pub context_menu: Option<ContextMenu>,

    // Radial menu state (new hold-to-show menu)
    pub radial_menu: super::radial_menu::RadialMenuState,

    // Unified drag manager (new system - replaces scattered gizmo_drag_* fields)
    pub drag_manager: DragManager,

    // Tool system (TrenchBroom-inspired)
    pub tool_box: ModelerToolBox,

    // Texture editor state
    pub texture_editor: TextureEditorState,

    // User texture library (shared with world editor)
    pub user_textures: TextureLibrary,

    // True when editing the indexed atlas with the texture editor
    pub editing_indexed_atlas: bool,

    // Temporary UserTexture for editing the indexed atlas
    // Synced back to IndexedAtlas on close
    pub editing_texture: Option<crate::texture::UserTexture>,

    // Currently selected user texture name (for single-click selection before editing)
    pub selected_user_texture: Option<String>,

    // Texture pending deletion (shows confirmation dialog when Some)
    pub texture_pending_delete: Option<String>,

    // Thumbnail size for paint texture grid (32, 48, 64, 96)
    pub paint_thumb_size: f32,

    // Pending async user texture list (cloud discovery)
    pub pending_user_texture_list: Option<crate::storage::PendingList>,
    // Queue of user textures to load from cloud
    pub pending_texture_loads: Vec<(String, crate::storage::PendingLoad)>,
    // Flag to trigger user texture refresh from main loop
    pub pending_texture_refresh: bool,

    // Object rename dialog state (object index, text input state)
    pub rename_dialog: Option<(usize, TextInputState)>,

    // Object delete confirmation dialog (object index)
    pub delete_dialog: Option<usize>,

    // Unsaved texture changes - pending object switch (shows save/discard dialog)
    pub unsaved_texture_pending_switch: Option<usize>,

    // Ambient light slider dragging state
    pub ambient_slider_active: bool,

    // Light component RGB slider being dragged (0=R, 1=G, 2=B)
    pub light_color_slider: Option<usize>,
}

/// Type of context menu being displayed (auto-detected from selection)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContextMenuType {
    /// Default menu for adding primitives (no selection or object mode)
    Primitives,
    /// Vertex operations (assign to bone, etc.)
    VertexOps,
    /// Face operations (materials, etc.) - future
    FaceOps,
    /// Edge operations - future
    EdgeOps,
}

#[derive(Clone)]
pub struct ContextMenu {
    /// Screen position of menu
    pub x: f32,
    pub y: f32,
    /// World position where clicked (for placing primitives)
    pub world_pos: Vec3,
    /// Which viewport the menu was opened in
    pub viewport: ViewportId,
    /// Type of context menu (auto-detected from selection)
    pub menu_type: ContextMenuType,
    /// Hovered bone index for highlighting (when in bone assignment submenu)
    pub hovered_bone: Option<usize>,
}

impl ContextMenu {
    /// Create context menu at position, auto-detecting type from selection
    pub fn at_cursor(x: f32, y: f32, world_pos: Vec3, viewport: ViewportId, selection: &ModelerSelection) -> Self {
        let menu_type = match selection {
            ModelerSelection::Vertices(v) if !v.is_empty() => ContextMenuType::VertexOps,
            ModelerSelection::Faces(f) if !f.is_empty() => ContextMenuType::FaceOps,
            ModelerSelection::Edges(e) if !e.is_empty() => ContextMenuType::EdgeOps,
            _ => ContextMenuType::Primitives,
        };
        Self { x, y, world_pos, viewport, menu_type, hovered_bone: None }
    }

    /// Legacy constructor for primitives menu
    pub fn new(x: f32, y: f32, world_pos: Vec3, viewport: ViewportId) -> Self {
        Self { x, y, world_pos, viewport, menu_type: ContextMenuType::Primitives, hovered_bone: None }
    }

    pub fn vertex_ops(x: f32, y: f32) -> Self {
        Self {
            x, y,
            world_pos: Vec3::ZERO,
            viewport: ViewportId::Perspective,
            menu_type: ContextMenuType::VertexOps,
            hovered_bone: None,
        }
    }
}

/// Unified undo event - mesh change, selection change, texture change, or skeleton change
#[derive(Debug, Clone)]
pub enum UndoEvent {
    /// Mesh edit (geometry, UVs, etc.)
    Mesh {
        object_index: Option<usize>,
        mesh: EditableMesh,
        atlas: Option<IndexedAtlas>,
        description: String,
    },
    /// Selection change only
    Selection(ModelerSelection),
    /// Texture paint edit (pixel indices, palette)
    Texture {
        indices: Vec<u8>,
        palette: Vec<Color15>,
    },
    /// Skeleton edit (bone transforms)
    Skeleton {
        bones: Vec<RigBone>,
        description: String,
    },
}

impl ModelerState {
    pub fn new() -> Self {
        // Camera setup
        // Scale: 1024 units = 1 meter (SECTOR_SIZE)
        // Default to free camera mode (like the world editor)
        let orbit_target = Vec3::new(0.0, 1024.0, 0.0); // Center at 1 meter height
        let orbit_distance = 4096.0; // 4 meters back
        let orbit_azimuth = 0.8;      // ~45 degrees
        let orbit_elevation = 0.3;    // ~17 degrees up

        let mut camera = Camera::new();
        // Initialize camera position for free mode (similar starting view)
        camera.position = Vec3::new(-2048.0, 2048.0, -2048.0);
        camera.rotation_x = 0.3;  // Looking slightly down
        camera.rotation_y = 0.8;  // Looking toward origin
        camera.update_basis();

        // Load user textures first so we can apply one to the default cube
        let user_textures = {
            let mut lib = TextureLibrary::new();
            if let Err(e) = lib.discover() {
                eprintln!("Failed to discover user textures: {}", e);
            }
            lib
        };

        // Asset is the single source of truth for mesh data
        // Asset::new() creates a default cube mesh
        let asset = Asset::new("untitled");

        // Initialize CLUT pool with a default grayscale palette
        let clut_pool = ClutPool::default();

        // Select first user texture for editing (but don't apply to default cube)
        let (editing_texture, selected_user_texture) = if let Some((name, tex)) = user_textures.iter().next() {
            (Some(tex.clone()), Some(name.to_string()))
        } else {
            (None, None)
        };

        Self {
            interaction_mode: InteractionMode::Edit,

            // Asset (single source of truth for geometry)
            asset,
            selected_object: Some(0),
            clut_pool,
            preview_clut: None,

            current_file: None,

            select_mode: SelectMode::Face, // PicoCAD: face-centric
            selection: ModelerSelection::None,
            transform_orientation: TransformOrientation::Global,

            camera,
            camera_mode: CameraMode::Free, // Default to free camera (like world editor)
            raster_settings: RasterSettings::modeler(), // Ambient lighting, placed lights shine through
            orbit_target,
            orbit_distance,
            orbit_azimuth,
            orbit_elevation,

            // PicoCAD-style viewports
            active_panel: ActivePanel::Viewport,
            active_viewport: ViewportId::Perspective,
            fullscreen_viewport: None,
            ortho_top: OrthoCamera::default(),
            ortho_front: OrthoCamera::default(),
            ortho_side: OrthoCamera::default(),

            // Resizable panels (default 50/50 splits)
            viewport_h_split: 0.5,
            viewport_v_split: 0.5,
            dragging_h_divider: false,
            dragging_v_divider: false,

            paint_color: Color::WHITE,
            paint_blend_mode: BlendMode::Opaque,
            brush_size: 4.0,
            brush_type: BrushType::Square,
            paint_mode: PaintMode::Texture,
            color_picker_slider: None,
            brush_size_slider_active: false,
            paint_stroke_active: false,

            // Collapsible sections
            paint_section_expanded: true,
            paint_texture_scroll: 0.0,
            paint_samples_collapsed: false,
            paint_user_collapsed: false,

            // Component management UI
            selected_component: None,
            components_section_expanded: true,
            properties_section_expanded: true,
            lights_section_expanded: true,
            dropdown: DropdownState::new(),
            component_opacity: Vec::new(),
            base_component_opacity: Vec::new(),
            opacity_drag: None,
            delete_component_dialog: None,

            // Component gizmo dragging
            component_gizmo_drag_axis: None,
            component_gizmo_drag_start: None,
            component_gizmo_start_offset: [0.0, 0.0, 0.0],
            component_gizmo_drag_viewport: None,

            // CLUT editing defaults
            selected_clut: None,
            selected_clut_entry: 1, // Default to index 1 (index 0 is transparent)
            active_palette_index: 1,
            clut_preview_active: false,
            clut_color_slider: None,

            vertex_linking: true, // Default on: move coincident vertices together

            hierarchy_expanded: Vec::new(),

            current_animation: 0,
            current_frame: 0,
            playing: false,
            playback_time: 0.0,
            selected_keyframes: Vec::new(),

            // Skeleton editing state (TR-style)
            // Bones stored in asset.skeleton(), not in ModelerState
            selected_bone: None,
            hovered_bone: None,
            hovered_bone_tip: None,
            show_bones: true,  // Default: show bones when they exist
            bone_creation: None,
            bone_rename_active: false,
            bone_rename_buffer: String::new(),
            bone_picker_target_mesh: None,

            dirty: false,
            status_message: None,

            // Undo/Redo system
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            max_undo_levels: 50,

            snap_settings: SnapSettings::default(),
            clipboard: Clipboard::default(),
            xray_mode: false,

            viewport_last_mouse: (0.0, 0.0),
            viewport_mouse_captured: false,
            modifier_was_held: false,
            trusted_movement_keys: [true; 6], // All trusted initially
            ortho_pan_viewport: None,
            ortho_last_mouse: (0.0, 0.0),

            box_select_pending_start: None,
            box_select_viewport: None,
            free_drag_pending_start: None,
            ortho_drag_pending_start: None,
            ortho_box_select_pending_start: None,
            ortho_box_select_viewport: None,
            ortho_drag_viewport: None,
            ortho_drag_zoom: 1.0,
            ortho_drag_axis: None,

            hovered_vertex: None,
            hovered_edge: None,
            hovered_face: None,

            gizmo_hovered_axis: None,
            ortho_gizmo_hovered_axis: None,
            gizmo_bone_drag: false,
            gizmo_bone_tip_drag: false,

            modal_transform: ModalTransform::None,

            context_menu: None,
            radial_menu: super::radial_menu::RadialMenuState::new(),

            drag_manager: DragManager::new(),

            tool_box: ModelerToolBox::new(),

            texture_editor: TextureEditorState::new(),
            user_textures,

            editing_indexed_atlas: false,
            editing_texture,
            selected_user_texture,
            texture_pending_delete: None,
            paint_thumb_size: 64.0,  // Default thumbnail size
            pending_user_texture_list: None,
            pending_texture_loads: Vec::new(),
            pending_texture_refresh: false,

            rename_dialog: None,
            delete_dialog: None,
            unsaved_texture_pending_switch: None,
            ambient_slider_active: false,
            light_color_slider: None,
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
    // Component Opacity (0 = fully visible, 7 = hidden)
    // ========================================================================

    /// Ensure opacity vecs are large enough for all components
    pub fn ensure_opacity_vec(&mut self) {
        let needed = self.asset.components.len();
        if self.component_opacity.len() < needed {
            self.component_opacity.resize(needed, 0); // Default to fully visible
        }
        if self.base_component_opacity.len() < needed {
            self.base_component_opacity.resize(needed, 0);
        }
    }

    /// Get opacity for a component (0 = visible, 7 = hidden)
    pub fn get_component_opacity(&self, idx: usize) -> u8 {
        self.component_opacity.get(idx).copied().unwrap_or(0)
    }

    /// Set the user-intended (base) opacity for a component, then reapply focus dimming
    pub fn set_component_opacity(&mut self, idx: usize, opacity: u8) {
        self.ensure_opacity_vec();
        let clamped = opacity.min(7);
        if let Some(val) = self.base_component_opacity.get_mut(idx) {
            *val = clamped;
        }
        if let Some(val) = self.component_opacity.get_mut(idx) {
            *val = clamped;
        }
    }

    /// Auto-dim non-selected components for visual focus.
    /// Selected component shows at its base opacity; others are dimmed to at least FOCUS_DIM_LEVEL.
    const FOCUS_DIM_LEVEL: u8 = 3;

    pub fn apply_focus_opacity(&mut self) {
        self.ensure_opacity_vec();
        let selected = self.selected_component;
        let count = self.asset.components.len();
        for i in 0..count {
            let base = self.base_component_opacity.get(i).copied().unwrap_or(0);
            if Some(i) == selected {
                // Selected component: show at user's intended opacity
                self.component_opacity[i] = base;
            } else {
                // Non-selected: dim to at least FOCUS_DIM_LEVEL (but respect if user set it higher)
                self.component_opacity[i] = base.max(Self::FOCUS_DIM_LEVEL);
            }
        }
    }

    /// Check if component is fully hidden (opacity >= 7)
    pub fn is_component_hidden(&self, idx: usize) -> bool {
        self.get_component_opacity(idx) >= 7
    }

    /// Convert opacity level (0-7) to alpha value (255-0)
    pub fn opacity_to_alpha(opacity: u8) -> u8 {
        match opacity {
            0 => 255,      // Fully visible
            1 => 220,
            2 => 180,
            3 => 140,
            4 => 100,
            5 => 60,
            6 => 30,
            _ => 0,        // Hidden (7+)
        }
    }

    // ========================================================================
    // Object/Mesh Access (asset is single source of truth)
    // ========================================================================

    /// Get all mesh objects as a slice
    pub fn objects(&self) -> &[MeshPart] {
        self.asset.mesh().map(|v| v.as_slice()).unwrap_or(&[])
    }

    /// Get mutable access to mesh objects
    pub fn objects_mut(&mut self) -> Option<&mut Vec<MeshPart>> {
        self.asset.mesh_mut()
    }

    /// Get a reference to the currently selected mesh (single source of truth)
    pub fn mesh(&self) -> &EditableMesh {
        static EMPTY: std::sync::OnceLock<EditableMesh> = std::sync::OnceLock::new();
        self.selected_object
            .and_then(|i| self.objects().get(i))
            .map(|obj| &obj.mesh)
            .unwrap_or_else(|| EMPTY.get_or_init(EditableMesh::new))
    }

    /// Get a mutable reference to the currently selected mesh (single source of truth)
    /// Returns None if no object is selected
    pub fn mesh_mut(&mut self) -> Option<&mut EditableMesh> {
        let idx = self.selected_object?;
        self.objects_mut()?.get_mut(idx).map(|obj| &mut obj.mesh)
    }

    /// Toggle interaction mode (Object <-> Edit)
    pub fn toggle_interaction_mode(&mut self) {
        self.interaction_mode = self.interaction_mode.toggle();
        self.selection.clear();
        self.set_status(&format!("{} Mode", self.interaction_mode.label()), 1.0);
    }

    /// Create a new mesh (replaces current)
    pub fn new_mesh(&mut self) {
        self.asset = Asset::new("untitled");
        self.selected_object = Some(0);
        self.clut_pool = ClutPool::default();
        self.preview_clut = None;
        self.current_file = None;
        self.selection.clear();
        self.dirty = false;
        self.set_status("New mesh", 1.0);
    }

    /// Save asset to file (includes mesh + components)
    #[cfg(not(target_arch = "wasm32"))]
    pub fn save_project(&mut self, path: &std::path::Path) -> Result<(), String> {
        // Asset is single source of truth - save it directly
        self.asset.save(path)
            .map_err(|e| format!("{}", e))?;
        self.current_file = Some(path.to_path_buf());
        self.dirty = false;
        self.set_status(&format!("Saved: {}", path.display()), 2.0);
        Ok(())
    }

    /// Load asset from file (includes mesh + components)
    #[cfg(not(target_arch = "wasm32"))]
    pub fn load_project(&mut self, path: &std::path::Path) -> Result<(), String> {
        let asset = Asset::load(path)
            .map_err(|e| format!("{}", e))?;
        self.asset = asset;
        self.selected_object = if self.objects().is_empty() { None } else { Some(0) };
        // Resolve ID-based texture references using the texture library
        self.resolve_all_texture_refs();
        self.current_file = Some(path.to_path_buf());
        self.selection.clear();
        self.dirty = false;
        self.set_status(&format!("Loaded: {}", path.display()), 2.0);
        Ok(())
    }

    /// Resolve all ID-based texture references in the asset
    ///
    /// For objects with TextureRef::Id, looks up the texture in the library
    /// and populates the runtime atlas with the texture data AND creates CLUTs.
    pub fn resolve_all_texture_refs(&mut self) {
        use crate::rasterizer::Clut;

        // Collect all ID -> texture data mappings we need
        // (to avoid borrow checker issues with self)
        let updates: Vec<(usize, String, usize, usize, crate::rasterizer::ClutDepth, Vec<u8>, Vec<Color15>)> = self.objects()
            .iter()
            .enumerate()
            .filter_map(|(idx, obj)| {
                if let TextureRef::Id(id) = &obj.texture_ref {
                    self.user_textures.get_by_id(*id).map(|tex| {
                        (idx, obj.name.clone(), tex.width, tex.height, tex.depth, tex.indices.clone(), tex.palette.clone())
                    })
                } else {
                    None
                }
            })
            .collect();

        // Now apply the updates - create CLUTs and set atlas data
        for (idx, obj_name, width, height, depth, indices, palette) in updates {
            // Create a CLUT for this object with the texture's palette
            let clut_name = format!("{}_clut", obj_name);
            let mut new_clut = Clut::new_4bit(&clut_name);
            new_clut.colors = palette;
            new_clut.depth = depth;
            let clut_id = self.clut_pool.add_clut(new_clut);

            // Update the object's atlas
            if let Some(objects) = self.objects_mut() {
                if let Some(obj) = objects.get_mut(idx) {
                    obj.atlas.width = width;
                    obj.atlas.height = height;
                    obj.atlas.depth = depth;
                    obj.atlas.indices = indices;
                    obj.atlas.default_clut = clut_id;
                }
            }
        }
    }

    // ========================================================================
    // Asset Helpers
    // ========================================================================

    /// Get the indexed texture atlas for the selected object
    /// Falls back to first object if no object is selected
    pub fn atlas(&self) -> &IndexedAtlas {
        static EMPTY: std::sync::OnceLock<IndexedAtlas> = std::sync::OnceLock::new();
        // Determine which object index to use
        let idx = self.selected_object
            .filter(|&i| i < self.objects().len())
            .unwrap_or(0);

        self.objects().get(idx)
            .map(|obj| &obj.atlas)
            .unwrap_or_else(|| EMPTY.get_or_init(|| IndexedAtlas::new_checkerboard(128, 128, crate::rasterizer::ClutDepth::Bpp4)))
    }

    /// Get mutable indexed texture atlas for the selected object
    /// Falls back to first object if no object is selected
    pub fn atlas_mut(&mut self) -> Option<&mut IndexedAtlas> {
        // Determine which object index to use
        let idx = self.selected_object
            .filter(|&i| i < self.objects().len())
            .unwrap_or(0);

        self.objects_mut()?.get_mut(idx).map(|obj| &mut obj.atlas)
    }

    /// Get the indexed texture atlas for a specific object
    pub fn object_atlas(&self, idx: usize) -> Option<&IndexedAtlas> {
        self.objects().get(idx).map(|o| &o.atlas)
    }

    /// Get mutable indexed texture atlas for a specific object
    pub fn object_atlas_mut(&mut self, idx: usize) -> Option<&mut IndexedAtlas> {
        self.objects_mut()?.get_mut(idx).map(|o| &mut o.atlas)
    }

    /// Resolve a TextureRef to actual texture data
    ///
    /// Returns None for TextureRef::None. For all other variants, returns
    /// the appropriate resolved texture with atlas and CLUT data.
    pub fn resolve_texture<'a>(&'a self, tex_ref: &'a TextureRef) -> Option<ResolvedTexture<'a>> {
        match tex_ref {
            TextureRef::None => None,
            TextureRef::Checkerboard => {
                Some(ResolvedTexture::Static {
                    atlas: checkerboard_atlas(),
                    clut: checkerboard_clut(),
                })
            }
            TextureRef::Id(id) => {
                self.user_textures.get_by_id(*id)
                    .map(ResolvedTexture::UserTexture)
            }
            TextureRef::Embedded(atlas) => {
                // Get CLUT from the CLUT pool
                self.clut_pool.get(atlas.default_clut)
                    .map(|clut| ResolvedTexture::Embedded {
                        atlas: atlas.as_ref(),
                        clut,
                    })
            }
        }
    }

    /// Resolve texture for a specific object index
    pub fn resolve_object_texture(&self, idx: usize) -> Option<ResolvedTexture<'_>> {
        self.objects().get(idx)
            .and_then(|obj| self.resolve_texture(&obj.texture_ref))
    }

    /// Get all visible mesh objects
    pub fn visible_objects(&self) -> impl Iterator<Item = (usize, &MeshPart)> {
        self.objects().iter().enumerate().filter(|(_, o)| o.visible)
    }

    /// Get the currently selected object index
    pub fn selected_object_index(&self) -> Option<usize> {
        self.selected_object
    }

    /// Select an object by index
    pub fn select_object(&mut self, index: usize) {
        if index < self.objects().len() {
            // Check for unsaved texture changes before switching
            if self.selected_object != Some(index) {
                if self.editing_indexed_atlas && self.texture_editor.dirty {
                    // Show save/discard dialog instead of switching immediately
                    self.unsaved_texture_pending_switch = Some(index);
                    return;
                }
                // Clear editing state when switching objects
                self.editing_indexed_atlas = false;
                self.editing_texture = None;
            }
            self.selected_object = Some(index);
            self.selection.clear();

            // Extract info before mutating self
            let (obj_name, tex_ref) = self.objects().get(index)
                .map(|obj| (obj.name.clone(), obj.texture_ref.clone()))
                .unwrap_or_default();

            self.set_status(&format!("Selected: {}", obj_name), 0.5);

            // Sync texture selection to match this object's texture reference
            match &tex_ref {
                TextureRef::Id(id) => {
                    // Look up texture name by ID
                    if let Some(name) = self.user_textures.get_name_by_id(*id) {
                        self.selected_user_texture = Some(name.to_string());
                    } else {
                        self.selected_user_texture = None;
                    }
                }
                TextureRef::Checkerboard | TextureRef::None => {
                    // Clear texture selection for built-in/no texture
                    self.selected_user_texture = None;
                }
                TextureRef::Embedded(_) => {
                    // Embedded textures don't have a library entry
                    self.selected_user_texture = None;
                }
            }
        }
    }

    /// Force object switch after unsaved texture dialog (bypasses dirty check)
    pub fn force_select_object(&mut self, index: usize) {
        if index < self.objects().len() {
            // Clear editing state
            self.editing_indexed_atlas = false;
            self.editing_texture = None;
            self.texture_editor.dirty = false;
            self.unsaved_texture_pending_switch = None;

            self.selected_object = Some(index);
            self.selection.clear();

            // Extract info before mutating self
            let (obj_name, tex_ref) = self.objects().get(index)
                .map(|obj| (obj.name.clone(), obj.texture_ref.clone()))
                .unwrap_or_default();

            self.set_status(&format!("Selected: {}", obj_name), 0.5);

            // Sync texture selection to match this object's texture reference
            match &tex_ref {
                TextureRef::Id(id) => {
                    if let Some(name) = self.user_textures.get_name_by_id(*id) {
                        self.selected_user_texture = Some(name.to_string());
                    } else {
                        self.selected_user_texture = None;
                    }
                }
                TextureRef::Checkerboard | TextureRef::None => {
                    self.selected_user_texture = None;
                }
                TextureRef::Embedded(_) => {
                    self.selected_user_texture = None;
                }
            }
        }
    }

    /// Get the currently selected mesh object
    pub fn selected_object(&self) -> Option<&MeshPart> {
        self.selected_object.and_then(|i| self.objects().get(i))
    }

    /// Get the currently selected mesh object mutably
    pub fn selected_object_mut(&mut self) -> Option<&mut MeshPart> {
        let idx = self.selected_object?;
        self.objects_mut()?.get_mut(idx)
    }

    /// Get the mirror settings for the currently selected object
    /// Returns MirrorSettings::default() if no object is selected or no mirror is set
    pub fn current_mirror_settings(&self) -> MirrorSettings {
        self.selected_object()
            .and_then(|obj| obj.mirror)
            .unwrap_or_default()
    }

    /// Add a new object to the asset
    pub fn add_object(&mut self, obj: MeshPart) -> usize {
        if let Some(objects) = self.objects_mut() {
            let idx = objects.len();
            objects.push(obj);
            self.selected_object = Some(idx);
            self.dirty = true;
            idx
        } else {
            // No mesh component - create one with this object
            use crate::asset::AssetComponent;
            self.asset.add_component(AssetComponent::Mesh { parts: vec![obj] });
            self.selected_object = Some(0);
            self.dirty = true;
            0
        }
    }

    /// Generate a unique object name with 2-digit suffix (e.g., "Cube.00", "Cube.01")
    pub fn generate_unique_object_name(&self, base_name: &str) -> String {
        let existing_names: std::collections::HashSet<&str> = self.objects()
            .iter()
            .map(|o| o.name.as_str())
            .collect();

        // Always use 2-digit suffix, starting from .00
        for i in 0..100 {
            let candidate = format!("{}.{:02}", base_name, i);
            if !existing_names.contains(candidate.as_str()) {
                return candidate;
            }
        }
        // Fallback for 100+ objects (unlikely)
        format!("{}.{}", base_name, self.objects().len())
    }

    /// Create a new asset (replaces current)
    pub fn new_project(&mut self) {
        self.asset = Asset::new("untitled");
        self.selected_object = Some(0);
        self.clut_pool = ClutPool::default();
        self.preview_clut = None;
        // Default cube is already created by Asset::new()
        self.current_file = None;
        self.selection.clear();
        self.dirty = false;
        self.set_status("New asset", 1.0);
    }

    // ========================================================================
    // Undo/Redo System (matches world editor pattern)
    // ========================================================================

    /// Save current mesh state for undo (before making geometry changes)
    pub fn save_undo(&mut self, description: &str) {
        self.undo_stack.push(UndoEvent::Mesh {
            object_index: self.selected_object,
            mesh: self.mesh().clone(),
            atlas: None,
            description: description.to_string(),
        });
        self.redo_stack.clear();
        self.dirty = true;

        // Limit undo stack size
        if self.undo_stack.len() > self.max_undo_levels {
            self.undo_stack.remove(0);
        }
    }

    /// Save current mesh state including texture atlas (for paint operations)
    pub fn save_undo_with_atlas(&mut self, description: &str) {
        self.undo_stack.push(UndoEvent::Mesh {
            object_index: self.selected_object,
            mesh: self.mesh().clone(),
            atlas: Some(self.atlas().clone()),
            description: description.to_string(),
        });
        self.redo_stack.clear();
        self.dirty = true;

        // Limit undo stack size
        if self.undo_stack.len() > self.max_undo_levels {
            self.undo_stack.remove(0);
        }
    }

    /// Save current selection state for undo
    pub fn save_selection_undo(&mut self) {
        // Don't save if selection hasn't changed from the last selection snapshot
        for event in self.undo_stack.iter().rev() {
            if let UndoEvent::Selection(last_sel) = event {
                if *last_sel == self.selection {
                    return; // No change from last selection snapshot
                }
                break; // Found a different selection snapshot
            }
        }

        self.undo_stack.push(UndoEvent::Selection(self.selection.clone()));
        self.redo_stack.clear();

        // Limit stack size
        if self.undo_stack.len() > self.max_undo_levels {
            self.undo_stack.remove(0);
        }
    }

    /// Save current texture state for undo (before making paint changes)
    pub fn save_texture_undo(&mut self) {
        if let Some(ref tex) = self.editing_texture {
            self.undo_stack.push(UndoEvent::Texture {
                indices: tex.indices.clone(),
                palette: tex.palette.clone(),
            });
            self.redo_stack.clear();
            self.texture_editor.dirty = true;

            // Limit stack size
            if self.undo_stack.len() > self.max_undo_levels {
                self.undo_stack.remove(0);
            }
        }
    }

    /// Save current skeleton state for undo (before making bone changes)
    pub fn save_undo_skeleton(&mut self, description: &str) {
        let bones = self.skeleton().to_vec();
        self.undo_stack.push(UndoEvent::Skeleton {
            bones,
            description: description.to_string(),
        });
        self.redo_stack.clear();
        self.dirty = true;

        // Limit stack size
        if self.undo_stack.len() > self.max_undo_levels {
            self.undo_stack.remove(0);
        }
    }

    /// Undo last action (mesh edit, selection, texture, or skeleton)
    pub fn undo(&mut self) -> bool {
        if let Some(event) = self.undo_stack.pop() {
            match event {
                UndoEvent::Mesh { object_index, mesh, atlas, description } => {
                    // Save current state to redo stack
                    let (current_mesh, current_atlas) = if let Some(idx) = object_index {
                        if let Some(obj) = self.objects().get(idx) {
                            (Some(obj.mesh.clone()), if atlas.is_some() { Some(obj.atlas.clone()) } else { None })
                        } else {
                            (None, None)
                        }
                    } else {
                        (None, None)
                    };
                    self.redo_stack.push(UndoEvent::Mesh {
                        object_index,
                        mesh: current_mesh.unwrap_or_else(EditableMesh::new),
                        atlas: current_atlas,
                        description: description.clone(),
                    });

                    // Restore the mesh and atlas to the correct object
                    if let Some(idx) = object_index {
                        if let Some(objects) = self.objects_mut() {
                            if let Some(obj) = objects.get_mut(idx) {
                                obj.mesh = mesh;
                                if let Some(a) = atlas {
                                    obj.atlas = a;
                                }
                            }
                        }
                        self.selected_object = Some(idx);
                    }
                    self.dirty = true;
                    self.set_status(&format!("Undo: {}", description), 1.0);
                }
                UndoEvent::Selection(prev_sel) => {
                    // Save current selection to redo stack
                    self.redo_stack.push(UndoEvent::Selection(self.selection.clone()));
                    self.selection = prev_sel;
                    self.set_status("Undo selection", 1.0);
                }
                UndoEvent::Texture { indices, palette } => {
                    // Save current state to redo stack
                    if let Some(ref tex) = self.editing_texture {
                        self.redo_stack.push(UndoEvent::Texture {
                            indices: tex.indices.clone(),
                            palette: tex.palette.clone(),
                        });
                    }
                    // Restore previous state
                    if let Some(ref mut tex) = self.editing_texture {
                        tex.indices = indices;
                        tex.palette = palette;
                    }
                    self.set_status("Undo paint", 1.0);
                }
                UndoEvent::Skeleton { bones, description } => {
                    // Save current skeleton to redo stack
                    self.redo_stack.push(UndoEvent::Skeleton {
                        bones: self.skeleton().to_vec(),
                        description: description.clone(),
                    });
                    // Restore previous skeleton
                    if let Some(skeleton_bones) = self.asset.skeleton_mut() {
                        *skeleton_bones = bones;
                    }
                    self.dirty = true;
                    self.set_status(&format!("Undo: {}", description), 1.0);
                }
            }
            true
        } else {
            self.set_status("Nothing to undo", 1.0);
            false
        }
    }

    /// Redo last undone action (mesh edit, selection, texture, or skeleton)
    pub fn redo(&mut self) -> bool {
        if let Some(event) = self.redo_stack.pop() {
            match event {
                UndoEvent::Mesh { object_index, mesh, atlas, description } => {
                    // Save current state to undo stack
                    let (current_mesh, current_atlas) = if let Some(idx) = object_index {
                        if let Some(obj) = self.objects().get(idx) {
                            (Some(obj.mesh.clone()), if atlas.is_some() { Some(obj.atlas.clone()) } else { None })
                        } else {
                            (None, None)
                        }
                    } else {
                        (None, None)
                    };
                    self.undo_stack.push(UndoEvent::Mesh {
                        object_index,
                        mesh: current_mesh.unwrap_or_else(EditableMesh::new),
                        atlas: current_atlas,
                        description: description.clone(),
                    });

                    // Restore the mesh and atlas to the correct object
                    if let Some(idx) = object_index {
                        if let Some(objects) = self.objects_mut() {
                            if let Some(obj) = objects.get_mut(idx) {
                                obj.mesh = mesh;
                                if let Some(a) = atlas {
                                    obj.atlas = a;
                                }
                            }
                        }
                        self.selected_object = Some(idx);
                    }
                    self.dirty = true;
                    self.set_status(&format!("Redo: {}", description), 1.0);
                }
                UndoEvent::Selection(next_sel) => {
                    // Save current selection to undo stack
                    self.undo_stack.push(UndoEvent::Selection(self.selection.clone()));
                    self.selection = next_sel;
                    self.set_status("Redo selection", 1.0);
                }
                UndoEvent::Texture { indices, palette } => {
                    // Save current state to undo stack
                    if let Some(ref tex) = self.editing_texture {
                        self.undo_stack.push(UndoEvent::Texture {
                            indices: tex.indices.clone(),
                            palette: tex.palette.clone(),
                        });
                    }
                    // Apply redo state
                    if let Some(ref mut tex) = self.editing_texture {
                        tex.indices = indices;
                        tex.palette = palette;
                    }
                    self.set_status("Redo paint", 1.0);
                }
                UndoEvent::Skeleton { bones, description } => {
                    // Save current skeleton to undo stack
                    self.undo_stack.push(UndoEvent::Skeleton {
                        bones: self.skeleton().to_vec(),
                        description: description.clone(),
                    });
                    // Apply redo state
                    if let Some(skeleton_bones) = self.asset.skeleton_mut() {
                        *skeleton_bones = bones;
                    }
                    self.dirty = true;
                    self.set_status(&format!("Redo: {}", description), 1.0);
                }
            }
            true
        } else {
            self.set_status("Nothing to redo", 1.0);
            false
        }
    }

    /// Backwards compatibility: alias for save_undo
    pub fn push_undo(&mut self, description: &str) {
        self.save_undo(description);
    }

    /// Backwards compatibility: alias for save_undo_with_atlas
    pub fn push_undo_with_atlas(&mut self, description: &str) {
        self.save_undo_with_atlas(description);
    }

    /// Check if undo is available
    pub fn can_undo(&self) -> bool {
        !self.undo_stack.is_empty()
    }

    /// Check if redo is available
    pub fn can_redo(&self) -> bool {
        !self.redo_stack.is_empty()
    }

    /// Set selection with undo support (saves previous selection to undo stack)
    pub fn set_selection(&mut self, new_selection: ModelerSelection) {
        if self.selection != new_selection {
            self.save_selection_undo();
            self.selection = new_selection;
        }
    }

    /// Clear selection with undo support
    pub fn clear_selection(&mut self) {
        self.set_selection(ModelerSelection::None);
    }

    // ========================================================================
    // Skeleton/Bone Operations (TR-style: fixed offsets, not animated)
    // Bones are stored in AssetComponent::Skeleton, accessed via asset.skeleton()
    // ========================================================================

    /// Get reference to skeleton bones (from first Skeleton component)
    pub fn skeleton(&self) -> &[RigBone] {
        self.asset.skeleton().map(|v| v.as_slice()).unwrap_or(&[])
    }

    /// Compute center of current selection (handles both mesh elements and bones)
    pub fn compute_selection_center(&self) -> Option<Vec3> {
        match &self.selection {
            ModelerSelection::Bones(bones) => {
                // Bone base selection - center at base positions
                if bones.is_empty() {
                    return None;
                }
                let skeleton = self.skeleton();
                let positions: Vec<Vec3> = bones.iter()
                    .filter(|&&idx| idx < skeleton.len())
                    .map(|&idx| {
                        let (base_pos, _) = self.get_bone_world_transform(idx);
                        base_pos
                    })
                    .collect();
                if positions.is_empty() {
                    return None;
                }
                let sum: Vec3 = positions.iter().fold(Vec3::ZERO, |acc, &p| acc + p);
                Some(sum * (1.0 / positions.len() as f32))
            }
            ModelerSelection::BoneTips(tips) => {
                // Bone tip selection - center at tip positions
                if tips.is_empty() {
                    return None;
                }
                let skeleton = self.skeleton();
                let positions: Vec<Vec3> = tips.iter()
                    .filter(|&&idx| idx < skeleton.len())
                    .map(|&idx| self.get_bone_tip_position(idx))
                    .collect();
                if positions.is_empty() {
                    return None;
                }
                let sum: Vec3 = positions.iter().fold(Vec3::ZERO, |acc, &p| acc + p);
                Some(sum * (1.0 / positions.len() as f32))
            }
            _ => {
                // Mesh element selection (vertices, edges, faces)
                // Apply bone transform if the selected object is bound to a bone
                let mesh = self.mesh();
                let indices = self.selection.get_affected_vertex_indices(mesh);
                if indices.is_empty() {
                    return None;
                }

                // Get bone transform if selected object is bound
                let bone_transform = self.selected_object()
                    .and_then(|obj| obj.default_bone_index)
                    .map(|bone_idx| self.get_bone_world_transform(bone_idx));

                let sum: Vec3 = indices.iter()
                    .filter_map(|&idx| mesh.vertices.get(idx))
                    .map(|v| {
                        if let Some((bone_pos, bone_rot)) = bone_transform {
                            rotate_by_euler(v.pos, bone_rot) + bone_pos
                        } else {
                            v.pos
                        }
                    })
                    .fold(Vec3::ZERO, |acc, pos| acc + pos);
                Some(sum * (1.0 / indices.len() as f32))
            }
        }
    }

    /// Compute the local orientation basis for transform gizmo
    /// Returns (x_axis, y_axis, z_axis) - three orthonormal vectors
    ///
    /// In Local mode:
    /// - For face selections: Uses face normal (transformed by bone if applicable)
    /// - For bone-bound meshes (non-face selection): Uses bone rotation
    /// - Otherwise: Falls back to world axes
    ///
    /// In Global mode: Always returns world axes
    pub fn compute_orientation_basis(&self) -> (Vec3, Vec3, Vec3) {
        let world_x = Vec3::new(1.0, 0.0, 0.0);
        let world_y = Vec3::new(0.0, 1.0, 0.0);
        let world_z = Vec3::new(0.0, 0.0, 1.0);

        if self.transform_orientation == TransformOrientation::Global {
            return (world_x, world_y, world_z);
        }

        // Local mode - try to compute local basis

        // For bone selections, use the selected bone's rotation
        if let ModelerSelection::Bones(bones) = &self.selection {
            if let Some(&bone_idx) = bones.first() {
                let (_, bone_rot) = self.get_bone_world_transform(bone_idx);
                let local_x = rotate_by_euler(world_x, bone_rot);
                let local_y = rotate_by_euler(world_y, bone_rot);
                let local_z = rotate_by_euler(world_z, bone_rot);
                return (local_x, local_y, local_z);
            }
        }

        // For bone tip selections, use the bone's rotation
        if let ModelerSelection::BoneTips(tips) = &self.selection {
            if let Some(&bone_idx) = tips.first() {
                let (_, bone_rot) = self.get_bone_world_transform(bone_idx);
                let local_x = rotate_by_euler(world_x, bone_rot);
                let local_y = rotate_by_euler(world_y, bone_rot);
                let local_z = rotate_by_euler(world_z, bone_rot);
                return (local_x, local_y, local_z);
            }
        }

        // Get bone transform if mesh is bone-bound
        let bone_transform = self.selected_object()
            .and_then(|obj| obj.default_bone_index)
            .map(|bone_idx| self.get_bone_world_transform(bone_idx));

        // For face selections, use face normal and edge directions
        if let ModelerSelection::Faces(faces) = &self.selection {
            if !faces.is_empty() {
                let mesh = self.mesh();
                let mut avg_normal = Vec3::ZERO;
                let mut first_edge = Vec3::ZERO;
                let mut count = 0;

                for &face_idx in faces {
                    if let Some(face) = mesh.faces.get(face_idx) {
                        // Get first edge direction (for tangent calculation)
                        if face.vertices.len() >= 2 && first_edge.len() < 0.001 {
                            if let (Some(v0), Some(v1)) = (
                                mesh.vertices.get(face.vertices[0]),
                                mesh.vertices.get(face.vertices[1])
                            ) {
                                first_edge = v1.pos - v0.pos;
                            }
                        }
                    }
                    if let Some(normal) = mesh.face_normal(face_idx) {
                        avg_normal = avg_normal + normal;
                        count += 1;
                    }
                }
                if count > 0 {
                    avg_normal = avg_normal * (1.0 / count as f32);
                    let len = (avg_normal.x * avg_normal.x + avg_normal.y * avg_normal.y + avg_normal.z * avg_normal.z).sqrt();
                    if len > 0.001 {
                        avg_normal = avg_normal * (1.0 / len);

                        // If bone-bound, transform normal AND edge to world space
                        if let Some((_, br)) = bone_transform {
                            avg_normal = rotate_by_euler(avg_normal, br);
                            first_edge = rotate_by_euler(first_edge, br);
                        }

                        // Normalize the edge to get X axis (tangent along first edge)
                        let edge_len = first_edge.len();
                        let local_x = if edge_len > 0.001 {
                            first_edge * (1.0 / edge_len)
                        } else {
                            // Fallback: use cross product with up vector
                            let up = if avg_normal.y.abs() < 0.9 { world_y } else { world_x };
                            let cross = Vec3::new(
                                up.y * avg_normal.z - up.z * avg_normal.y,
                                up.z * avg_normal.x - up.x * avg_normal.z,
                                up.x * avg_normal.y - up.y * avg_normal.x,
                            );
                            let cross_len = cross.len();
                            if cross_len > 0.001 { cross * (1.0 / cross_len) } else { world_x }
                        };

                        // Z = X × Y (where Y is normal) for right-handed system
                        let local_z = Vec3::new(
                            local_x.y * avg_normal.z - local_x.z * avg_normal.y,
                            local_x.z * avg_normal.x - local_x.x * avg_normal.z,
                            local_x.x * avg_normal.y - local_x.y * avg_normal.x,
                        );

                        // Y = face normal
                        return (local_x, avg_normal, local_z);
                    }
                }
            }
        }

        // For bone-bound meshes (non-face selection), use bone rotation
        if let Some((_, bone_rot)) = bone_transform {
            let local_x = rotate_by_euler(world_x, bone_rot);
            let local_y = rotate_by_euler(world_y, bone_rot);
            let local_z = rotate_by_euler(world_z, bone_rot);
            return (local_x, local_y, local_z);
        }

        // Fallback to world axes
        (world_x, world_y, world_z)
    }

    /// Transform a world-space delta to local space for the current orientation
    /// Used when applying transforms in Global mode on bone-bound meshes
    pub fn world_to_local_delta(&self, delta: Vec3) -> Vec3 {
        // If bone-bound, we need to inverse-rotate the delta
        if let Some(obj) = self.selected_object() {
            if let Some(bone_idx) = obj.default_bone_index {
                let (_, bone_rot) = self.get_bone_world_transform(bone_idx);
                return inverse_rotate_by_euler(delta, bone_rot);
            }
        }
        delta
    }

    /// Transform a local-space delta to world space
    pub fn local_to_world_delta(&self, delta: Vec3) -> Vec3 {
        if let Some(obj) = self.selected_object() {
            if let Some(bone_idx) = obj.default_bone_index {
                let (_, bone_rot) = self.get_bone_world_transform(bone_idx);
                return rotate_by_euler(delta, bone_rot);
            }
        }
        delta
    }

    /// Add a bone to the skeleton and return its index
    pub fn add_bone(&mut self, bone: RigBone) -> Option<usize> {
        if let Some(bones) = self.asset.skeleton_mut() {
            let idx = bones.len();
            bones.push(bone);
            self.dirty = true;
            Some(idx)
        } else {
            None // No Skeleton component
        }
    }

    /// Remove a bone from the skeleton
    /// Children are reparented to the deleted bone's parent
    pub fn remove_bone(&mut self, bone_idx: usize) {
        let skeleton = match self.asset.skeleton_mut() {
            Some(s) => s,
            None => return,
        };

        if bone_idx >= skeleton.len() {
            return;
        }

        // Get the parent of the bone being removed
        let parent = skeleton[bone_idx].parent;

        // Reparent all children to the deleted bone's parent
        for bone in skeleton.iter_mut() {
            if bone.parent == Some(bone_idx) {
                bone.parent = parent;
            }
            // Adjust indices for bones after the removed one
            if let Some(p) = bone.parent {
                if p > bone_idx {
                    bone.parent = Some(p - 1);
                }
            }
        }

        // Remove the bone
        skeleton.remove(bone_idx);

        // Clear selection if it was the removed bone
        if self.selected_bone == Some(bone_idx) {
            self.selected_bone = None;
        } else if let Some(sel) = self.selected_bone {
            if sel > bone_idx {
                self.selected_bone = Some(sel - 1);
            }
        }

        self.dirty = true;
    }

    /// Get the world transform for a bone by walking up the hierarchy
    /// Returns (position, rotation) in world space
    pub fn get_bone_world_transform(&self, bone_idx: usize) -> (Vec3, Vec3) {
        let skeleton = self.skeleton();
        if bone_idx >= skeleton.len() {
            return (Vec3::ZERO, Vec3::ZERO);
        }

        let mut position = Vec3::ZERO;
        let mut rotation = Vec3::ZERO;

        // Walk up the hierarchy, accumulating transforms
        let mut current = Some(bone_idx);
        let mut chain = Vec::new();

        // Build chain from root to this bone
        while let Some(idx) = current {
            chain.push(idx);
            current = skeleton[idx].parent;
        }

        // Apply transforms from root to leaf
        for idx in chain.into_iter().rev() {
            let bone = &skeleton[idx];
            // Rotate the local_position by the accumulated rotation, then add
            let rotated_pos = rotate_by_euler(bone.local_position, rotation);
            position = position + rotated_pos;
            rotation = rotation + bone.local_rotation;
        }

        (position, rotation)
    }

    /// Get the world position of a bone's tip (base + direction * length)
    pub fn get_bone_tip_position(&self, bone_idx: usize) -> Vec3 {
        let skeleton = self.skeleton();
        if bone_idx >= skeleton.len() {
            return Vec3::ZERO;
        }

        let (base_pos, rotation) = self.get_bone_world_transform(bone_idx);
        let bone = &skeleton[bone_idx];

        // Calculate direction from rotation (inverse of direction_to_rotation)
        // rot_x = pitch (tilt forward/back), rot_z = yaw (turn left/right)
        // Default direction is Y-up (0, 1, 0)
        let rad_x = rotation.x.to_radians();
        let rad_z = rotation.z.to_radians();
        let cos_x = rad_x.cos();
        let direction = Vec3::new(
            rad_z.sin() * cos_x,   // X: horizontal component scaled by cos(pitch)
            rad_z.cos() * cos_x,   // Y: forward component scaled by cos(pitch)
            -rad_x.sin(),          // Z: vertical tilt from pitch
        ).normalize();

        base_pos + direction * bone.length
    }

    /// Get indices of root bones (no parent)
    pub fn root_bones(&self) -> Vec<usize> {
        self.skeleton()
            .iter()
            .enumerate()
            .filter(|(_, b)| b.parent.is_none())
            .map(|(i, _)| i)
            .collect()
    }

    /// Get indices of children for a bone
    pub fn bone_children(&self, parent_idx: usize) -> Vec<usize> {
        self.skeleton()
            .iter()
            .enumerate()
            .filter(|(_, b)| b.parent == Some(parent_idx))
            .map(|(i, _)| i)
            .collect()
    }

    /// Generate a unique bone name
    pub fn generate_bone_name(&self) -> String {
        let skeleton = self.skeleton();
        let existing: std::collections::HashSet<&str> = skeleton
            .iter()
            .map(|b| b.name.as_str())
            .collect();

        for i in 0..100 {
            let name = format!("Bone.{:02}", i);
            if !existing.contains(name.as_str()) {
                return name;
            }
        }
        format!("Bone.{}", skeleton.len())
    }

    // ========================================================================
    // Per-Vertex Bone Assignment (Rigid Skinning)
    // ========================================================================

    /// Assign currently selected vertices to a bone.
    /// Only works in vertex selection mode.
    pub fn assign_selected_vertices_to_bone(&mut self, bone_idx: usize) {
        // Verify bone exists
        if bone_idx >= self.skeleton().len() {
            return;
        }

        // Get vertex indices from current selection (vertices, faces, or edges)
        let vertex_indices: Vec<usize> = match &self.selection {
            ModelerSelection::Vertices(v) => v.clone(),
            ModelerSelection::Faces(faces) => {
                // Get all vertices from selected faces
                let face_indices = faces.clone();
                let mesh = self.mesh();
                let mut verts: std::collections::HashSet<usize> = std::collections::HashSet::new();
                for &face_idx in &face_indices {
                    if let Some(face) = mesh.faces.get(face_idx) {
                        verts.extend(face.vertices.iter().copied());
                    }
                }
                verts.into_iter().collect()
            }
            ModelerSelection::Edges(edges) => {
                // Get both vertices from each selected edge
                let mut verts: std::collections::HashSet<usize> = std::collections::HashSet::new();
                for &(a, b) in edges {
                    verts.insert(a);
                    verts.insert(b);
                }
                verts.into_iter().collect()
            }
            _ => {
                self.set_status("Select vertices, faces, or edges to assign", 2.0);
                return;
            }
        };

        if vertex_indices.is_empty() {
            self.set_status("No geometry selected", 2.0);
            return;
        }

        // Get bone name before mutable borrow
        let bone_name = self.skeleton().get(bone_idx).map(|b| b.name.clone()).unwrap_or_else(|| "bone".to_string());
        let obj_idx = self.selected_object.unwrap_or(0);

        // Save undo state
        self.save_undo("Assign Vertices to Bone");

        // Assign vertices to bone
        if let Some(objects) = self.objects_mut() {
            if let Some(obj) = objects.get_mut(obj_idx) {
                obj.mesh.assign_vertices_to_bone(&vertex_indices, Some(bone_idx));
            }
        }

        // Set status message
        self.set_status(&format!("Assigned {} vertices to '{}'", vertex_indices.len(), bone_name), 2.0);
    }

    /// Unassign currently selected vertices (they will use mesh's default_bone_index).
    /// Works with vertex, face, or edge selection.
    pub fn unassign_selected_vertices(&mut self) {
        // Get vertex indices from current selection (vertices, faces, or edges)
        let vertex_indices: Vec<usize> = match &self.selection {
            ModelerSelection::Vertices(v) => v.clone(),
            ModelerSelection::Faces(faces) => {
                let face_indices = faces.clone();
                let mesh = self.mesh();
                let mut verts: std::collections::HashSet<usize> = std::collections::HashSet::new();
                for &face_idx in &face_indices {
                    if let Some(face) = mesh.faces.get(face_idx) {
                        verts.extend(face.vertices.iter().copied());
                    }
                }
                verts.into_iter().collect()
            }
            ModelerSelection::Edges(edges) => {
                let mut verts: std::collections::HashSet<usize> = std::collections::HashSet::new();
                for &(a, b) in edges {
                    verts.insert(a);
                    verts.insert(b);
                }
                verts.into_iter().collect()
            }
            _ => {
                self.set_status("Select vertices, faces, or edges to unassign", 2.0);
                return;
            }
        };

        if vertex_indices.is_empty() {
            self.set_status("No geometry selected", 2.0);
            return;
        }

        let obj_idx = self.selected_object.unwrap_or(0);

        // Save undo state
        self.save_undo("Unassign Vertices from Bone");

        // Unassign vertices
        if let Some(objects) = self.objects_mut() {
            if let Some(obj) = objects.get_mut(obj_idx) {
                obj.mesh.assign_vertices_to_bone(&vertex_indices, None);
            }
        }

        self.set_status(&format!("Unassigned {} vertices from bone", vertex_indices.len()), 2.0);
    }

    /// Select all vertices assigned to a specific bone.
    /// Switches to vertex selection mode if not already.
    pub fn select_vertices_for_bone(&mut self, bone_idx: usize) {
        // Get bone name first
        let bone_name = self.skeleton().get(bone_idx).map(|b| b.name.clone()).unwrap_or_else(|| "bone".to_string());

        // Get current mesh
        let vertex_indices = if let Some(obj) = self.selected_object() {
            obj.mesh.get_vertices_for_bone(bone_idx)
        } else {
            return;
        };

        if vertex_indices.is_empty() {
            self.set_status(&format!("No vertices assigned to '{}'", bone_name), 1.5);
            return;
        }

        // Switch to edit mode vertex selection
        self.interaction_mode = InteractionMode::Edit;
        self.selection = ModelerSelection::Vertices(vertex_indices.clone());

        self.set_status(&format!("Selected {} vertices for '{}'", vertex_indices.len(), bone_name), 2.0);
    }

    /// Get count of vertices assigned to a specific bone for the selected mesh.
    pub fn count_vertices_for_bone(&self, bone_idx: usize) -> usize {
        if let Some(obj) = self.selected_object() {
            obj.mesh.get_vertices_for_bone(bone_idx).len()
        } else {
            0
        }
    }

}

impl Default for ModelerState {
    fn default() -> Self {
        Self::new()
    }
}
