# BONNIE-32 Backlog

This document tracks planned features, known issues, and future improvements.

---

## Backlog

### Architecture

- [ ] **Unified render pipeline**: Game and editor currently have separate render paths with duplicated code. Should create a shared scene renderer that both use, with hooks for editor-specific overlays (grid, selection, gizmos) and game-specific features (player, particles). This becomes critical as we add meshes, skeletal animation, particle systems, etc. Benefits: any optimization applies to both, WYSIWYG editing, single source of truth for rendering.


---

### World Editor - Geometry

- [ ] We need a way to place rooms on top of each other, maybe the top view can be toggled with a side view?
- [ ] **Batch slope/height editing**: When multiple faces are selected, changing slope or height should apply to all selected faces proportionally.
- [ ] **Better floor placement highlight**: Improve the visual feedback when placing floors to make it clearer where the floor will be placed.

---

### World Editor - 3D Viewport

- [ ] Context-sensitive bottom bar: Show left/right click actions; when right-clicking show WASD/QE bindings
- [ ] **Camera floor limit**: Add a minimum camera height so users can't accidentally go below the floor level. Should be slightly below the camera's current position.
- [ ] **Object movement like cells**: Move objects by dragging on Z/X axis, with Shift for Y axis movement (same as cell movement)

---

### World Editor - UI/UX

- [ ] **Refine skybox section**: The skybox settings need UI polish and better controls.
- [ ] **Gradient fills across cells**: Support linear and spherical gradient fills across multiple selected cells for vertex colors.
- [ ] **Auto-select room vs room lock**: Automatically select the room the cursor is in, rather than requiring manual room locking.
- [ ] **Color slider lock**: Add a lock toggle to color sliders to prevent accidental changes.
- [ ] **Convert texture-packs to .ron textures**: Migrate from texture-packs format to individual .ron texture files (like assets/samples/textures). Remove assets/samples/texture-packs entirely.
- [ ] **Environment/Objects switch**: Add a switch between environment and objects editing mode

---

### Music Tracker

#### UI/UX
- [ ] Per-note vs channel FX toggle
- [ ] **Piano roll quick tool**: Add a piano roll that slides up from the bottom as a quick entry tool. Keep the pattern editor as the main detailed editing view.
- [ ] **Track notes persistence**: If a track has notes and user reduces track count, notes should be preserved (track just hidden). Must also save hidden track data.

#### Future
- [ ] **Waveform visualizer**: Add a waveform visualizer somewhere in the UI for visual feedback during playback

---

### Paint Editor

#### Canvas Operations
- [ ] **Non-destructive resize**: Canvas resize should be non-destructive until user saves (different from track editor behavior)
- [ ] **Color adjustments**: Add contrast/saturation/hue/brightness controls
- [ ] **Multiple layers**: Full support for multiple layers
- [ ] **Multiple frames**: Full support for animation frames

---

### Rendering / PS1 Authenticity

- [ ] **Texture wobble effect**: Per-texture property for water/wobble distortion effect

---

### Assets (Modeler)

#### Known Issues
- [ ] **Rotation mouse movement unintuitive**: Currently only responds to left/right movement. Should follow the rotation axis precisely

#### UX Improvements
- [ ] **Full undo support**: Ctrl-Z should extend to palette changes, component creation/deletion, and generally every action in the asset editor
- [ ] **Uniform scale from gizmo center**: Click center of gizmo to scale uniformly across all axes
- [ ] **Configurable grid snap granularity**: Allow changing the Snap to Grid step size
- [ ] **Rotation pivot from selection**: Allow selecting an edge as rotation pivot (secondary selection mode) - similar to Blender's 3D cursor but simpler

#### Future
- [ ] 2D editor for designing menus and UIs
