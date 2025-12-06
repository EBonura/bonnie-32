# Bonnie-Engine Modeler Tool Design

## Overview

A low-poly 3D modeler with segmented/hierarchy animation - the authentic PS1 approach used in Metal Gear Solid, Resident Evil, and Final Fantasy VII.

**Key Principle**: Each model part IS its own bone. No weight painting, no GPU skinning. Just hierarchical transforms.

---

## Data Structures

### Core Model

```rust
/// A segmented 3D model (PS1-style hierarchy animation)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Model {
    pub name: String,
    pub parts: Vec<ModelPart>,          // Flat array, hierarchy via parent index
    pub animations: Vec<Animation>,
    pub atlas: TextureAtlas,
}

/// A single part of the model (its own mesh + transform)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelPart {
    pub name: String,                   // "torso", "head", "arm_l", etc.
    pub parent: Option<usize>,          // Index into parts array (None = root)
    pub pivot: Vec3,                    // Joint/pivot point (local space)
    pub vertices: Vec<ModelVertex>,     // Vertices relative to pivot
    pub faces: Vec<ModelFace>,
    pub visible: bool,                  // For editor visibility toggle
}

/// Vertex data (no bone weights needed!)
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ModelVertex {
    pub position: Vec3,                 // Relative to part's pivot
    pub uv: Vec2,                       // UV coordinates into atlas
    pub color: Color,                   // Vertex color (PS1-style lighting)
}

/// Triangle face
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ModelFace {
    pub indices: [usize; 3],            // Indices into part's vertices
    pub double_sided: bool,
}

/// Texture atlas (single texture per model)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextureAtlas {
    pub size: AtlasSize,
    pub pixels: Vec<u8>,                // RGBA data
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum AtlasSize {
    S64 = 64,
    S128 = 128,
    S256 = 256,
    S512 = 512,
}
```

### Animation

```rust
/// Named animation clip
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Animation {
    pub name: String,                   // "idle", "walk", "attack"
    pub fps: u8,                        // Frames per second (typically 15-30)
    pub looping: bool,
    pub keyframes: Vec<Keyframe>,
}

/// Single keyframe (stores transform for each part)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Keyframe {
    pub frame: u32,                     // Frame number (0, 5, 10, etc.)
    pub transforms: Vec<PartTransform>, // One per ModelPart, indexed same
}

/// Local transform for a part at a keyframe
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct PartTransform {
    pub position: Vec3,                 // Offset from rest pose
    pub rotation: Vec3,                 // Euler angles (degrees) - simpler than quaternions
}

impl Default for PartTransform {
    fn default() -> Self {
        Self {
            position: Vec3::ZERO,
            rotation: Vec3::ZERO,
        }
    }
}
```

---

## Editor State

```rust
/// Modeler view modes
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ModelerView {
    Model,      // Edit mesh geometry
    UV,         // Edit UV mapping
    Paint,      // Texture + vertex color painting
    Hierarchy,  // Edit part hierarchy + pivots
    Animate,    // Timeline + keyframe animation
}

/// Selection modes for modeling
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SelectMode {
    Part,       // Select whole parts
    Vertex,
    Edge,
    Face,
}

/// Current selection in the modeler
#[derive(Debug, Clone)]
pub enum ModelerSelection {
    None,
    Parts(Vec<usize>),                          // Selected part indices
    Vertices { part: usize, verts: Vec<usize> },
    Edges { part: usize, edges: Vec<(usize, usize)> },
    Faces { part: usize, faces: Vec<usize> },
}

/// Active transform tool
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TransformTool {
    Select,
    Move,       // G key
    Rotate,     // R key
    Scale,      // S key
    Extrude,    // E key
}

/// Main modeler state
pub struct ModelerState {
    // Model data
    pub model: Model,
    pub current_file: Option<PathBuf>,

    // View state
    pub view: ModelerView,
    pub select_mode: SelectMode,
    pub tool: TransformTool,
    pub selection: ModelerSelection,

    // Camera
    pub camera: Camera,
    pub raster_settings: RasterSettings,

    // UV Editor state
    pub uv_zoom: f32,
    pub uv_offset: Vec2,
    pub uv_selection: Vec<usize>,           // Selected UV vertices

    // Paint state
    pub paint_color: Color,
    pub brush_size: f32,
    pub paint_mode: PaintMode,              // Texture or VertexColor

    // Hierarchy state
    pub selected_part: Option<usize>,
    pub hierarchy_expanded: Vec<bool>,      // Which parts are expanded in tree

    // Animation state
    pub current_animation: usize,
    pub current_frame: u32,
    pub playing: bool,
    pub playback_time: f64,
    pub selected_keyframes: Vec<usize>,     // Selected keyframe indices

    // Edit state
    pub undo_stack: Vec<Model>,
    pub redo_stack: Vec<Model>,
    pub dirty: bool,
    pub status_message: Option<(String, f64)>,

    // Transform state (for mouse drag)
    pub transform_start: Option<Vec3>,
    pub axis_lock: Option<Axis>,            // X, Y, Z constraint
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PaintMode {
    Texture,
    VertexColor,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Axis {
    X,
    Y,
    Z,
}
```

---

## UI Layout

Following the World Editor's pattern:

```
┌─────────────────────────────────────────────────────────────────────────────┐
│ [New][Open][Save] | [Undo][Redo] | [Select][Move][Rotate][Scale][Extrude]   │
│ | [Mode: Model ▼] | [Part][Vert][Edge][Face] | [PS1 toggles] | file.bon*   │
├────────────────┬────────────────────────────────────────────┬───────────────┤
│                │                                            │               │
│   Hierarchy    │                                            │    Atlas      │
│   ──────────   │                                            │  ┌─────────┐  │
│   ▼ Model      │                                            │  │         │  │
│     ▼ torso    │           3D Viewport                      │  │  128²   │  │
│       └ head   │                                            │  │         │  │
│       └ arm_l  │      (software rasterizer)                 │  └─────────┘  │
│         └ hand_l                                            │ ────────────  │
│       └ arm_r  │                                            │  Properties   │
│       └ leg_l  │                                            │  - Position   │
│       └ leg_r  │                                            │  - Rotation   │
│                │                                            │  - UV coords  │
│ ────────────── │                                            │  - Vtx color  │
│   UV Editor    │                                            │               │
│  (2D atlas)    │                                            │               │
│                │                                            │               │
├────────────────┴────────────────────────────────────────────┴───────────────┤
│ [◀◀][▶][■] Frame: 012/060 |████░░░░░░░░░░░| Anim: idle | [+Key][-Key]       │
└─────────────────────────────────────────────────────────────────────────────┘
```

### Panel Structure

```rust
pub struct ModelerLayout {
    /// Main horizontal split (left panels | center viewport | right panels)
    pub main_split: SplitPanel,         // Ratio ~0.20 (left 20%)
    pub right_split: SplitPanel,        // Ratio ~0.80 of remainder (viewport 80%)

    /// Left vertical split (hierarchy | UV editor)
    pub left_split: SplitPanel,         // Ratio ~0.5

    /// Right vertical split (atlas | properties)
    pub right_panel_split: SplitPanel,  // Ratio ~0.4

    /// Timeline height (fixed at bottom, ~80px)
    pub timeline_height: f32,
}
```

---

## Mode-Specific Behavior

### Model Mode (Geometry Editing)

**Selection:**
- Click to select (Part/Vertex/Edge/Face based on mode)
- Shift+click to add to selection
- B for box select
- A to toggle select all

**Transforms:**
- G = Grab/Move (then X/Y/Z to lock axis)
- R = Rotate (then X/Y/Z to lock axis)
- S = Scale (then X/Y/Z to lock axis)
- Mouse move to transform, click to confirm, Escape to cancel

**Modeling Operations:**
- E = Extrude selection
- F = Fill face (from selected verts/edges)
- X = Delete
- M = Merge vertices
- P = Separate selection to new part

**Primitives (Shift+A menu):**
- Plane
- Cube
- Cylinder (with segment count)
- Cone
- UV Sphere (low-poly)

### UV Mode

**Left Panel (UV Editor):**
- Shows 2D view of atlas with UV islands
- Same selection tools as 3D (G/R/S work in 2D)
- Pixel-snap toggle (essential for PS1 crisp textures)

**Operations:**
- U = Unwrap menu (Box, Cylinder, Sphere, Project from View)
- L = Select linked UVs
- Ctrl+E = Mark seam (for unwrapping)

### Paint Mode

**Brush Settings (right panel):**
- Brush size slider
- Color picker (with palette)
- Mode toggle: Texture / Vertex Color

**In Viewport:**
- Click+drag to paint
- Shift+click = eyedropper (pick color)
- [ / ] = decrease/increase brush size

**Vertex Color specifics:**
- Colors stored per-vertex
- Great for PS1 baked lighting
- Blends across faces automatically

### Hierarchy Mode

**Left Panel (Part Tree):**
- Drag to reparent parts
- Right-click context menu: Rename, Delete, Duplicate

**Viewport:**
- Click part to select
- Pivot gizmo for adjusting joint position
- Arrow to show parent-child relationships

### Animate Mode

**Timeline (bottom):**
- Horizontal strip showing frames
- Diamond markers for keyframes
- Playhead (vertical line)
- Drag to scrub

**Dopesheet (replaces UV editor in left panel):**
- Rows = parts
- Columns = frames
- Click cell to select keyframe
- Drag to move keyframes

**Operations:**
- I = Insert keyframe (for selected parts)
- K = Remove keyframe
- Space = Play/Pause
- Shift+Left/Right = Previous/Next keyframe
- Home/End = First/Last frame

---

## Hotkey Reference

### Global (all modes)
| Key | Action |
|-----|--------|
| Tab | Cycle through views (Model→UV→Paint→Hierarchy→Animate) |
| 1 | Part select mode |
| 2 | Vertex select mode |
| 3 | Edge select mode |
| 4 | Face select mode |
| Ctrl+Z | Undo |
| Ctrl+Shift+Z | Redo |
| Ctrl+S | Save |
| Ctrl+O | Open |
| Ctrl+N | New |

### Transform
| Key | Action |
|-----|--------|
| G | Grab/Move |
| R | Rotate |
| S | Scale |
| X (during transform) | Lock to X axis |
| Y (during transform) | Lock to Y axis |
| Z (during transform) | Lock to Z axis |
| Escape | Cancel transform |
| Enter/Click | Confirm transform |

### Model Mode
| Key | Action |
|-----|--------|
| E | Extrude |
| F | Fill face |
| M | Merge |
| P | Separate to new part |
| X | Delete |
| A | Select all/none |
| B | Box select |
| L | Select linked |
| Shift+A | Add primitive menu |

### Animate Mode
| Key | Action |
|-----|--------|
| I | Insert keyframe |
| K | Delete keyframe |
| Space | Play/Pause |
| Left/Right | Previous/Next frame |
| Shift+Left/Right | Previous/Next keyframe |
| Home | First frame |
| End | Last frame |

---

## File Formats

### Native Format (.bon)

RON serialization (matches your existing level format):

```ron
(
    name: "character",
    parts: [
        (
            name: "torso",
            parent: None,
            pivot: (x: 0.0, y: 0.0, z: 0.0),
            vertices: [
                (position: (x: -10, y: 0, z: -10), uv: (x: 0.0, y: 0.0), color: (r: 255, g: 255, b: 255, a: 255)),
                // ...
            ],
            faces: [
                (indices: [0, 1, 2], double_sided: false),
                // ...
            ],
            visible: true,
        ),
        // more parts...
    ],
    animations: [
        (
            name: "idle",
            fps: 15,
            looping: true,
            keyframes: [
                (
                    frame: 0,
                    transforms: [
                        (position: (x: 0, y: 0, z: 0), rotation: (x: 0, y: 0, z: 0)),
                        // one per part
                    ]
                ),
                // more keyframes...
            ]
        )
    ],
    atlas: (
        size: S128,
        pixels: [...],  // Base64 encoded or raw bytes
    ),
)
```

### OBJ Export (Static Mesh Only)

For interoperability - exports current pose, no animation:

```obj
# Bonnie-Engine OBJ Export
# Model: character

mtllib character.mtl

o torso
v -10.0 0.0 -10.0
v 10.0 0.0 -10.0
...
vt 0.0 0.0
vt 0.125 0.0
...
vn 0.0 1.0 0.0
...
f 1/1/1 2/2/1 3/3/1
...

o head
...
```

### OBJ Import

- Each `o` or `g` becomes a ModelPart
- Parent relationships need manual setup after import
- UVs imported directly
- Vertex colors not supported in OBJ (set to white)

---

## Rendering at Runtime

```rust
/// Render a model at a specific animation frame
pub fn render_model(
    fb: &mut Framebuffer,
    model: &Model,
    animation: &Animation,
    frame: f32,                     // Can be fractional for interpolation
    camera: &Camera,
    settings: &RasterSettings,
) {
    // 1. Compute interpolated pose
    let pose = interpolate_pose(animation, frame);

    // 2. Build world matrices hierarchically
    let mut world_matrices: Vec<Mat4> = Vec::with_capacity(model.parts.len());

    for (i, part) in model.parts.iter().enumerate() {
        // Get local transform for this part at this frame
        let local_transform = &pose.transforms[i];

        // Build local matrix: translate to pivot, rotate, translate back
        let local = Mat4::from_translation(part.pivot)
            * Mat4::from_euler_angles(
                local_transform.rotation.x.to_radians(),
                local_transform.rotation.y.to_radians(),
                local_transform.rotation.z.to_radians(),
            )
            * Mat4::from_translation(local_transform.position);

        // Multiply by parent's world matrix (or identity if root)
        let world = if let Some(parent_idx) = part.parent {
            world_matrices[parent_idx] * local
        } else {
            local
        };

        world_matrices.push(world);
    }

    // 3. Render each part
    for (i, part) in model.parts.iter().enumerate() {
        if !part.visible {
            continue;
        }

        let world_mat = &world_matrices[i];

        // Transform vertices
        let transformed_verts: Vec<Vertex> = part.vertices.iter()
            .map(|v| {
                let world_pos = *world_mat * v.position.extend(1.0);
                Vertex {
                    pos: world_pos.truncate(),
                    uv: v.uv,
                    // Apply vertex color as shading
                    ..Default::default()
                }
            })
            .collect();

        // Render faces
        for face in &part.faces {
            render_triangle(
                fb,
                &transformed_verts[face.indices[0]],
                &transformed_verts[face.indices[1]],
                &transformed_verts[face.indices[2]],
                &model.atlas,
                camera,
                settings,
            );
        }
    }
}

/// Interpolate between keyframes
fn interpolate_pose(animation: &Animation, frame: f32) -> Keyframe {
    // Find surrounding keyframes
    let frame_int = frame as u32;
    let t = frame.fract();

    let (kf_a, kf_b) = find_keyframes(animation, frame_int);

    // Lerp each part's transform
    let transforms = kf_a.transforms.iter()
        .zip(kf_b.transforms.iter())
        .map(|(a, b)| PartTransform {
            position: a.position.lerp(b.position, t),
            rotation: a.rotation.lerp(b.rotation, t), // Simple lerp for Euler
        })
        .collect();

    Keyframe {
        frame: frame_int,
        transforms,
    }
}
```

---

## Implementation Phases

### Phase 1: Foundation (1-2 weeks work)
- [ ] Add `modeler` module to project
- [ ] Implement data structures (Model, ModelPart, etc.)
- [ ] Add `Modeler` variant to `Tool` enum in app.rs
- [ ] Basic layout with panels (copy from editor)
- [ ] 3D viewport with camera controls (reuse from editor)
- [ ] Render static model parts

### Phase 2: Modeling Tools (2-3 weeks)
- [ ] Part/Vertex/Edge/Face selection
- [ ] Transform gizmos (Move, Rotate, Scale)
- [ ] Axis locking (X/Y/Z)
- [ ] Extrude operation
- [ ] Fill face (F key)
- [ ] Merge vertices
- [ ] Add primitive shapes
- [ ] Undo/redo

### Phase 3: UV & Texturing (1-2 weeks)
- [ ] UV Editor panel (2D view)
- [ ] Basic unwrap (box projection)
- [ ] UV selection and manipulation
- [ ] Pixel-snap mode
- [ ] Atlas size selection (64/128/256/512)
- [ ] Load texture into atlas

### Phase 4: Painting (1-2 weeks)
- [ ] Texture painting brush
- [ ] Color picker
- [ ] Vertex color painting
- [ ] Brush size control
- [ ] Eyedropper tool

### Phase 5: Hierarchy (1 week)
- [ ] Part tree view in left panel
- [ ] Drag-to-reparent
- [ ] Pivot point editing
- [ ] Separate selection to new part
- [ ] Join parts

### Phase 6: Animation (2-3 weeks)
- [ ] Timeline UI at bottom
- [ ] Playback (play/pause/stop)
- [ ] Keyframe insertion/deletion
- [ ] Dopesheet view
- [ ] Keyframe interpolation
- [ ] Animation list (multiple clips)
- [ ] Copy/paste keyframes

### Phase 7: Import/Export (1 week)
- [ ] Native .bon format save/load
- [ ] OBJ import (static)
- [ ] OBJ export (current pose)

### Phase 8: Polish
- [ ] Keyboard shortcuts reference (F1)
- [ ] Tooltips for all tools
- [ ] Status bar messages
- [ ] Error handling/recovery
- [ ] Performance optimization

---

## Icon Needs

Add to `ui/icons.rs`:

```rust
// Modeler tools
pub const CUBE: char = '\u{e061}';          // Box primitive (already have)
pub const SPHERE: char = '\u{...}';         // Sphere primitive
pub const CYLINDER: char = '\u{...}';       // Cylinder primitive
pub const VERTEX: char = '\u{...}';         // Vertex selection mode
pub const EDGE_MODE: char = '\u{...}';      // Edge selection mode
pub const FACE_MODE: char = '\u{...}';      // Face selection mode
pub const EXTRUDE: char = '\u{...}';        // Extrude tool
pub const MERGE: char = '\u{...}';          // Merge vertices
pub const BRUSH: char = '\u{e12e}';         // Paint brush
pub const EYEDROPPER: char = '\u{...}';     // Color picker
pub const BONE: char = '\u{e058}';          // Hierarchy/bone
pub const KEYFRAME: char = '\u{...}';       // Keyframe diamond
pub const TIMELINE: char = '\u{...}';       // Animation timeline
```

---

## Integration Points

### With World Editor
- Import models into levels as objects
- Reference models by name/path
- Place models at positions in rooms

### With Music Editor
- Eventually: sync animations to music beats
- Audio cues for animation events

### Export for Game Runtime
- Binary format for fast loading
- Pre-computed animation samples
- Compressed atlas textures

---

## Open Questions

1. **Max parts per model?** Suggest 64 (PS1-era characters rarely exceeded 20)
2. **Max vertices per part?** Suggest 256 (keeps things low-poly)
3. **Atlas format at runtime?** PNG for now, consider indexed palette later
4. **Morph targets?** Not in initial version, add later if needed
5. **IK constraints?** Not in initial version (FK only is more PS1-authentic)
