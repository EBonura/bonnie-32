# Bonnie Engine Backlog & Roadmap

This document tracks planned features, known issues, and future improvements.

---

## Backlog

### Rendering / PS1 Authenticity
- Controls page should have the "joystick" icon, currently there's just a sad face
- [ ] **15-bit texture palette conversion**: All imported textures should be quantized to 15-bit color (5 bits per channel). Should be toggleable like other PS1 effects. Consider keeping original textures and generating converted copies on-demand to balance memory usage vs. authenticity.
- [ ] **Face transparency modes**: In properties panel, allow setting PS1 semi-transparency blend modes (Average, Add, Subtract, AddQuarter) per face
- [ ] **Face normal flipping**: In properties panel, allow swapping/flipping face normals

---

### World Editor - Geometry

- [ ] Smarter floor/ceiling placement tool, if there's already a floor nearby, use that height, ideally if the neighbour floor is slanted, new floor should have the same slant
- [ ] We need a way to place rooms on top of each other, maybe the top view can be toggled with a side view?
- [ ] bigger effort: Tomb raider 3 introduced diagonals which are indeed supported by Open Lara, we'll need those as well

---

### Rendering Pipeline

- [ ] **Dynamic lighting support**: Recalculate affected vertex colors per frame for point lights

---

### World Editor - 3D Viewport

- [ ] Context-sensitive bottom bar: Show left/right click actions; when right-clicking show WASD/QE bindings

---

### Music Tracker

#### UI/UX
- [ ] **Text too small**: Everything is very small text-wise. The world editor has better scaling - study that and make text bigger where it makes sense
- [ ] Per-note vs channel FX toggle

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

---

### PS1 Technical Reference

For implementing authentic PS1 constraints:

**VRAM:**
- Total: 1MB (1,048,576 bytes)
- Screen buffer (320x240x16bit): ~153,600 bytes
- Double buffer: ~307,200 bytes
- Available for textures: ~700-900KB
- Textures typically 4-bit or 8-bit indexed with CLUTs

**Dithering (Bayer 4x4 matrix):**
```
 0/16   8/16   2/16  10/16
12/16   4/16  14/16   6/16
 3/16  11/16   1/16   9/16
15/16   7/16  13/16   5/16
```

---

## Roadmap

### Priority: Map Creation & Basic Gameplay
- [ ] Fix 2D grid placement precision (sectors not aligning to clicks)
- [ ] Slope/ramp tools

### UI & Settings
- [ ] Options menu in-game (resolution, PS1 effects toggles)
- [ ] Resolution selector (240p, 480p, native)
- [ ] HUD system (health, stamina bars)

### Rendering & Effects
- [ ] Sprite/billboard rendering (classic PS1 technique for enemies, items)
- [ ] Particle system (dust, sparks, blood splatter)
- [ ] Fog system (distance-based fade)

### Core Systems
- [ ] Inventory system
- [ ] Save/load game state

### Souls-like Mechanics
- [ ] Lock-on targeting
- [ ] Stamina-based combat (attacks, dodges, blocks)
- [ ] Bonfire checkpoints (rest, respawn, level up)
- [ ] Death/corpse run mechanics
- [ ] Boss arenas and encounters
- [ ] Weapon system (durability, movesets)
- [ ] Estus flask / healing system

### Editor QoL
- [ ] Copy/paste sectors
- [ ] Grid snapping toggles
- [ ] Vertex welding/merging tool
- [ ] Face splitting/subdividing
- [ ] Selection box (drag to select multiple)

---

### Feedback Session (January 2026)

#### Music Tracker Layout
- [ ] **Piano roll quick tool**: Add a piano roll that slides up from the bottom as a quick entry tool. Keep the pattern editor as the main detailed editing view.

#### World Editor - UI/UX
- [ ] **Preserve existing textures when changing texture pack**: Currently switching texture packs deletes loaded textures. Should add new textures while keeping existing ones.
- [ ] **Refine skybox section**: The skybox settings need UI polish and better controls.
- [ ] **Fix texture selection visibility**: Textures that aren't fully visible in the browser can't be selected. Fix click detection for partially visible textures.
- [ ] **Camera floor limit**: Add a minimum camera height so users can't accidentally go below the floor level. Should be slightly below the camera's current position.
- [ ] **2D view auto-center on current room**: The 2D grid view should center on the currently selected room. Also auto-center in 3D editor when switching rooms.
- [ ] **Extended multi-selection**: Current multi-selection is limited. Should support larger selections and more operations.
- [ ] **Scroll wheel for camera dolly**: Use mouse scroll wheel for forward/backward camera movement (dolly) in addition to zoom.
- [ ] **Shift+click for range select, Ctrl+click for toggle select**: Standard selection behavior - Shift extends selection range, Ctrl toggles individual items.
- [ ] **Add deselect functionality**: Currently there's no way to deselect all. Add Escape or click-on-empty to deselect.
- [ ] **Single click deselects multi-selection**: When you have multiple items selected, clicking on a single cell should deselect everything and select only that cell.
- [ ] **Gradient fills across cells**: Support linear and spherical gradient fills across multiple selected cells for vertex colors.
- [ ] **Auto-select room vs room lock**: Automatically select the room the cursor is in, rather than requiring manual room locking.
- [ ] **Color slider lock + double-click reset**: Add a lock toggle to color sliders to prevent accidental changes. Double-click should reset to default value. Should work for both face colors and single vertex colors.
- [ ] **Fix vertex color removal**: The "remove color" option doesn't work properly for vertex colors.
- [ ] **Load textures on new level**: When creating a new level, textures aren't loaded automatically. Should load default texture pack.
- [ ] **Hold button for continuous placement**: When holding the mouse button in floor/wall/ceiling mode, should continuously place geometry as the mouse moves.
- [ ] **Batch slope/height editing**: When multiple faces are selected, changing slope or height should apply to all selected faces proportionally.
- [ ] **Better floor placement highlight**: Improve the visual feedback when placing floors to make it clearer where the floor will be placed.
- [ ] **Increase texture cache time**: Textures are being unloaded too quickly from cache. Increase the retention time to avoid reloading.

### Level Design Features
- [ ] Water/liquid volumes (with different rendering)
- [ ] Trigger volumes (for events, cutscenes)
- [ ] Ladder/climbing surfaces
- [ ] Moving platforms
- [ ] Destructible geometry
- [ ] Skyboxes (PS1-style low-poly or texture-based)

### Enemy/NPC Systems
- [ ] AI pathfinding
- [ ] Aggro/detection radius
- [ ] Attack patterns
- [ ] Animation state machine

### Performance
- [ ] Frustum culling optimization
- [ ] Occlusion culling (beyond portals)
- [ ] Level streaming for large worlds

### Future Tools (Maybe)
- [ ] Texture editor integration
- [ ] Animation tool (for entities/bosses)
- [ ] Cutscene editor
