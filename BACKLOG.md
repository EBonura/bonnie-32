# Bonnie Engine Backlog

This document tracks planned features, known issues, and future improvements.

---

## Backlog

### Architecture

- [ ] **Unified render pipeline**: Game and editor currently have separate render paths with duplicated code. Should create a shared scene renderer that both use, with hooks for editor-specific overlays (grid, selection, gizmos) and game-specific features (player, particles). This becomes critical as we add meshes, skeletal animation, particle systems, etc. Benefits: any optimization applies to both, WYSIWYG editing, single source of truth for rendering.


---

### World Editor - Geometry

- [ ] Smarter floor/ceiling placement tool, if there's already a floor nearby, use that height, ideally if the neighbour floor is slanted, new floor should have the same slant
- [ ] We need a way to place rooms on top of each other, maybe the top view can be toggled with a side view?
- [ ] **Hold button for continuous placement**: When holding the mouse button in floor/wall/ceiling mode, should continuously place geometry as the mouse moves.
- [ ] **Batch slope/height editing**: When multiple faces are selected, changing slope or height should apply to all selected faces proportionally.
- [ ] **Better floor placement highlight**: Improve the visual feedback when placing floors to make it clearer where the floor will be placed.

---

### World Editor - 3D Viewport

- [ ] Context-sensitive bottom bar: Show left/right click actions; when right-clicking show WASD/QE bindings
- [ ] **Camera floor limit**: Add a minimum camera height so users can't accidentally go below the floor level. Should be slightly below the camera's current position.
- [ ] **Scroll wheel for camera dolly**: Use mouse scroll wheel for forward/backward camera movement (dolly) in addition to zoom.
- [ ] **Object movement like cells**: Move objects by dragging on Z/X axis, with Shift for Y axis movement (same as cell movement)
- [ ] **Larger light indicators**: Lights are hard to see and click - make them at least 3x bigger

---

### World Editor - UI/UX

- [ ] **Scissor clipping uses unsafe**: The texture editor and other panels use `unsafe { get_internal_gl().quad_gl.scissor(...) }` for clipping. Consider creating a safe wrapper function in the UI module to encapsulate this pattern.
- [ ] **Refine skybox section**: The skybox settings need UI polish and better controls.
- [ ] **Gradient fills across cells**: Support linear and spherical gradient fills across multiple selected cells for vertex colors.
- [ ] **Auto-select room vs room lock**: Automatically select the room the cursor is in, rather than requiring manual room locking.
- [ ] **Color slider lock**: Add a lock toggle to color sliders to prevent accidental changes.
- [ ] **Increase texture cache time**: Textures are being uwnloaded too quickly from cache. Increase the retention time to avoid reloading.
- [ ] **Environment/Objects switch**: Add a switch between environment and objects editing mode
- [ ] **Smarter 2D diagonal display**: Only show diagonal lines in sectors when it matters (different textures or heights) for a cleaner 2D view
- [ ] **Better fog defaults**: Default fog values should be color (5,5,5), start 8192, falloff 30k+, cull 9k. Sliders should work in sectors (1 sector = 1024)
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
- [ ] Selection box overflows from 2D view into 3D viewport when dragging near boundary
- [ ] Fix transform tool icons: Select/Move/Rotate/Scale all show the same select icon
- [ ] **Rotation mouse movement unintuitive**: Currently only responds to left/right movement. Should follow the rotation axis precisely

#### UX Improvements
- [ ] **Uniform scale from gizmo center**: Click center of gizmo to scale uniformly across all axes
- [ ] **Configurable grid snap granularity**: Allow changing the Snap to Grid step size
- [ ] **Rotation pivot from selection**: Allow selecting an edge as rotation pivot (secondary selection mode) - similar to Blender's 3D cursor but simpler

#### Future
- [ ] **Light support**: Asset editor should support lights. Lights in world editor become objects with only a light component (no geometry)
