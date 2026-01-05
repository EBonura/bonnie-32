# Bonnie Engine Backlog

This document tracks planned features, known issues, and future improvements.

---

## Backlog

### Rendering / PS1 Authenticity
- [ ] **15-bit texture palette conversion**: All imported textures should be quantized to 15-bit color (5 bits per channel). Should be toggleable like other PS1 effects. Consider keeping original textures and generating converted copies on-demand to balance memory usage vs. authenticity.
- [ ] **Face transparency modes**: In properties panel, allow setting PS1 semi-transparency blend modes (Average, Add, Subtract, AddQuarter) per face

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

---

### World Editor - UI/UX

- [ ] **Refine skybox section**: The skybox settings need UI polish and better controls.
- [ ] **2D view auto-center on current room**: The 2D grid view should center on the currently selected room. Also auto-center in 3D editor when switching rooms.
- [ ] **Extended multi-selection**: Current multi-selection is limited. Should support larger selections and more operations.
- [ ] **Shift+click for range select, Ctrl+click for toggle select**: Standard selection behavior - Shift extends selection range, Ctrl toggles individual items.
- [ ] **Add deselect functionality**: Currently there's no way to deselect all. Add Escape or click-on-empty to deselect.
- [ ] **Single click deselects multi-selection**: When you have multiple items selected, clicking on a single cell should deselect everything and select only that cell.
- [ ] **Gradient fills across cells**: Support linear and spherical gradient fills across multiple selected cells for vertex colors.
- [ ] **Auto-select room vs room lock**: Automatically select the room the cursor is in, rather than requiring manual room locking.
- [ ] **Color slider lock + double-click reset**: Add a lock toggle to color sliders to prevent accidental changes. Double-click should reset to default value. Should work for both face colors and single vertex colors.
- [ ] **Fix vertex color removal**: The "remove color" option doesn't work properly for vertex colors.
- [ ] **Load textures on new level**: When creating a new level, textures aren't loaded automatically. Should load default texture pack.
- [ ] **Increase texture cache time**: Textures are being unloaded too quickly from cache. Increase the retention time to avoid reloading.

---

### Music Tracker

#### UI/UX
- [ ] Per-note vs channel FX toggle
- [ ] **Piano roll quick tool**: Add a piano roll that slides up from the bottom as a quick entry tool. Keep the pattern editor as the main detailed editing view.

#### Future
- [ ] Custom instrument editor: Tab for building custom instruments beyond SF2 soundfonts
- [ ] **Waveform visualizer**: Add a waveform visualizer somewhere in the UI for visual feedback during playback

---

### Assets (Modeler)

#### Known Issues
- [ ] Drag-to-select box only works in 3D view (not in 2D view)
- [ ] Selection box overflows from 2D view into 3D viewport when dragging near boundary
- [ ] Scale and Rotate transform modes not implemented - need to add Move mode first with proper gizmos for each mode
- [ ] Overview panel is still a stub
- [ ] Fix transform tool icons: Select/Move/Rotate/Scale all show the same select icon

#### Future
- [ ] VRAM usage counter: Display usage with warning when exceeded
