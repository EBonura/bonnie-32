# Bonnie Engine

[![Version](https://img.shields.io/badge/version-0.1.1-blue.svg)](https://github.com/ebonura/bonnie-engine/releases)
[![License](https://img.shields.io/badge/license-MIT-green.svg)](LICENSE)

**Created by [Emanuele Bonura](https://bonnie-games.itch.io/)**

**Live Demo:** [https://ebonura.github.io/bonnie-engine](https://ebonura.github.io/bonnie-engine)

[GitHub](https://github.com/EBonura/bonnie-engine) | [itch.io](https://bonnie-games.itch.io/) | [Buy Me a Coffee](https://buymeacoffee.com/bonniegames)

---

## Mission

Answer the question: **"How would a Souls-like have looked on a PS1?"**

## Core Pillars

1. **Unified Development Environment** - Every tool needed to create the game lives alongside the game itself. The editor, renderer, and game logic are one integrated package.

2. **Cross-Platform First** - Everything runs both in the browser (live demo) and locally (for planned Steam distribution). No compromises on either platform.

3. **Authentic PS1 Aesthetics** - Every feature serves the goal of recreating genuine PlayStation 1 hardware limitations and visual characteristics.

## Features

### Authentic PS1 Rendering
- **Affine texture mapping** - Characteristic warpy textures
- **Vertex snapping** - Jittery vertices at low precision
- **Gouraud shading** - Smooth per-vertex lighting interpolation
- **Per-vertex colors** - PS1-style texture modulation (Wipeout track tinting)
- **Low resolution** - Native 320x240 rendering (toggleable)
- **No perspective correction** - True to PS1 hardware limitations
- **Z-sorted room boundaries** - Depth-tested wireframe overlays
- **PS1 SPU reverb** - Authentic reverb emulation with 10 PsyQ SDK presets (Room, Studio, Hall, Space, Echo, etc.)
- **Aspect ratio toggle** - Switch between 4:3 and stretch-to-fill viewport

### TR1-Style Level System
- **Room-based architecture** - Levels divided into connected rooms
- **Portal culling** - Only render visible rooms through portals
- **TRLE sector grid** - 1024-unit sectors for precise alignment
- **Textured geometry** - Multiple texture pack support

### Modern Editor UI

The editor features a MuseScore-inspired interface design:

- **Tab-based navigation** - Fixed tabs for World Editor, Sound Designer, Tracker, and Game preview
- **Flat icon buttons** - Clean, minimal toolbar with [Lucide](https://lucide.dev/) icons
- **Cyan accent color** - Active state highlighting inspired by MuseScore 4
- **Unified toolbar** - All tools accessible in a single row
- **Tooltips** - Hover hints for all buttons

#### Dual Viewport System
- **3D Viewport** - Real-time preview with authentic PS1 rendering
  - Free camera (WASD + Q/E for height)
  - Orbit camera mode for focused editing
  - Vertex height editing (Y-axis only)
  - Face/edge/vertex selection with hover feedback

- **2D Grid View** - Top-down editing for precise layout
  - Sector-aligned floor/ceiling placement
  - Vertex position editing (X/Z plane)
  - Pan and zoom navigation

#### Editing Tools
- **Select Mode** - Pick and manipulate vertices, edges, and faces
- **Multi-select** - Shift+click to select multiple faces, edges, or vertices
- **Floor Tool** - Place 1024x1024 floor sectors with Shift+drag height adjustment
- **Ceiling Tool** - Place ceiling sectors with Shift+drag height adjustment
- **Wall Tool** - Click sector edges to create walls (auto-faces camera)
- **Edge Dragging** - Select and drag edges on floors, ceilings, and walls to adjust heights
- **Texture Painting** - Click faces to apply selected texture (applies to all multi-selected faces)
- **Vertex Linking** - Move coincident vertices together or independently
- **Face Deletion** - Delete/Backspace removes selected floors, ceilings, and walls
- **UV Mapping Controls** - Offset, scale, and rotation controls for texture alignment

#### Texture Management
- Browse multiple texture packs with chevron navigation
- ~800 textures across 4 included packs
- Auto-apply textures to new geometry
- Texture reference system (pack + name)
- WASM support via build-time manifest generation

#### Workflow Features
- **Undo/Redo** - Full history for all edits
- **Cross-platform save/load**
  - Desktop: Native file dialogs
  - Browser: Import/Export via download/upload
- **Level browser** - Browse and load example levels with modal overlay
- **Live preview** - Test levels with Play button
- **Status messages** - Contextual feedback for all operations

## Controls

### Editor Mode
- **Play button**: Test level in game mode
- **File menu**: Save, Load, Import, Export

#### 3D Viewport
- Right-click + drag: Rotate camera
- WASD: Move horizontally
- Q/E: Move up/down
- Left-click: Select geometry / Place walls on edges
- Shift + left-click: Add to multi-selection
- Drag vertices/edges: Adjust heights (floors, ceilings, walls)
- Shift + drag: Adjust placement height (Floor/Ceiling/Wall modes)
- Delete/Backspace: Remove selected face

#### 2D Grid View
- Left-click: Place floors/ceilings or select geometry
- Shift + left-click: Add sectors to multi-selection
- Right-click + drag: Pan view
- Scroll wheel: Zoom in/out
- Drag vertices: Reposition on X/Z plane

#### Toolbar
- **Select**: Choose and drag geometry
- **Floor**: Place floor sectors (Shift+drag to adjust height)
- **Wall**: Create walls on sector edges (faces toward camera)
- **Ceil**: Place ceiling sectors (Shift+drag to adjust height)
- **Link ON/OFF**: Toggle vertex linking mode
- **Delete/Backspace**: Remove selected faces

### Game Mode
- Press **Esc** to return to editor
- Right-click + drag: Look around
- WASD: Move camera
- Q/E: Move up/down
- **1/2/3**: Shading mode (None/Flat/Gouraud)
- **P**: Toggle perspective correction
- **J**: Toggle vertex jitter
- **Z**: Toggle Z-buffer

### Music Editor
- **37-key piano** - 3 octaves with full keyboard mapping
- **Z to /**: Piano keys (bottom row)
- **Q to ]**: Piano keys (top row, continues seamlessly)
- **Numpad +/-**: Octave up/down
- **Space**: Play/Pause
- **Esc**: Stop playback
- **F9/F10**: Edit step down/up
- **Apostrophe (`)**: Note off
- **Arrow keys**: Navigate pattern
- **Home/End**: Jump to start/end of pattern
- **PS1 reverb knob** - 10 authentic PsyQ SDK presets with wet/dry mix control

## Building

```bash
cargo run --release
```

## Web Build

```bash
# Build for web
cargo build --release --target wasm32-unknown-unknown

# Serve locally
python3 -m http.server 8000
```

## Texture Credits

This project uses the following free texture packs:

- **Retro Texture Pack** by Little Martian
  https://little-martian.itch.io/retro-textures-pack

- **Low Poly 64x64 Textures** by PhobicPaul
  https://phobicpaul.itch.io/low-poly-64x64-textures

- **Quake-Like Texture Pack** by Level Eleven Games
  https://level-eleven-games.itch.io/quake-like-texture-pack

- **Dark Fantasy Townhouse 64x64 Texture Pack** by Level Eleven Games
  https://level-eleven-games.itch.io/dark-fantasy-townhouse-64x64-texture-pack

## Backlog

### Rendering / PS1 Authenticity

- [ ] **15-bit texture palette conversion**: All imported textures should be quantized to 15-bit color (5 bits per channel). Should be toggleable like other PS1 effects. Consider keeping original textures and generating converted copies on-demand to balance memory usage vs. authenticity.
- [ ] **Face transparency modes**: In properties panel, allow setting PS1 semi-transparency blend modes (Average, Add, Subtract, AddQuarter) per face
- [ ] **Face normal flipping**: In properties panel, allow swapping/flipping face normals

---

### World Editor - UI/UX

- [x] **Fix link icon**: Now uses `link-2` and `link-2-off` from Lucide
- [x] **Rename linking feature**: Renamed to "Geometry Linked/Independent" (works for vertices, edges, and faces)
- [x] **Per-room ambient light**: Each room now uses its own ambient setting when rendering

---

### World Editor - Geometry

- [x] **Cross-room boundary linking**: Moving vertices/edges/faces now finds and moves coincident vertices across all rooms
- [ ] **Multiple walls per sector edge**: Currently limited to 1 wall per side. The `Sector` struct already supports multiple walls via `Vec<VerticalFace>`. Implement smart placement where new walls stack on top/bottom of existing walls with configurable start/end heights.
- [ ] smarter floow/ceiling placement tool, if there's already a floor nearby, use that height, ideally if the neighbour floor is slanted, new floor should have the same slant

---

### Overall / Meta

- [ ] Remove AI/Claude mentions from git history (use `git filter-branch` or BFG Repo Cleaner - backup first!)

---

### Rendering Pipeline

- [ ] **Dynamic lighting support**: Recalculate affected vertex colors per frame for point lights

---

### World Editor - 3D Viewport

#### Bugs/Polish
- [ ] Context-sensitive bottom bar: Show left/right click actions; when right-clicking show WASD/QE bindings

#### Major Features
- [x] **Implement portals**: Auto-generated portals between adjacent rooms (supports infinite height for open-air sectors)

#### Future
- [x] Entity system design: Tile-based objects (PlayerStart, Light) with properties panel editing

---

### Music Editor

#### Remaining
- [ ] Configurable pattern length: Currently hardcoded to 64 rows - should be adjustable
- [ ] Per-note vs channel FX toggle
- [ ] Bottom status bar with context-sensitive shortcuts

#### Future
- [ ] Custom instrument editor: Tab for building custom instruments beyond SF2 soundfonts

---

### Assets (Modeler)

#### Remaining
- [ ] Fix transform tool icons: Select/Move/Rotate/Scale all show the same select icon

#### Future
- [ ] Pixel art painting tools: Built-in tools specific for texture painting
- [ ] PS1 color depth constraints: Limit to PS1 palette (toggleable)
- [ ] VRAM usage counter: Display usage with warning when exceeded
- [ ] Polygon count indicator: Green/yellow/red based on PS1-realistic counts

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
- [x] Portal creation and room connectivity (auto-generated portals between adjacent rooms)
- [x] Multi-room support
- [ ] Slope/ramp tools
- [x] Collision detection and physics (TR-style cylinder collision)
- [x] Character controller (movement, jumping)
- [x] Camera system (third-person follow, orbit preview)

### UI & Settings
- [x] Editor toolbar: PS1 effects toggles (vertex jitter, affine mapping, dithering, etc.)
- [x] Level browser with example levels
- [ ] Options menu in-game (resolution, PS1 effects toggles)
- [ ] Resolution selector (240p, 480p, native)
- [ ] HUD system (health, stamina bars)

### Rendering & Effects
- [ ] Sprite/billboard rendering (classic PS1 technique for enemies, items)
- [ ] Particle system (dust, sparks, blood splatter)
- [ ] Fog system (distance-based fade)

### Core Systems
- [x] Entity system (tile-based objects: PlayerStart, Light, with properties panel)
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
- [x] Multi-selection (Shift+click)
- [x] Texture applies to all multi-selected faces
- [x] UV mapping controls (offset, scale, rotation)
- [x] Orbit camera mode
- [ ] Copy/paste sectors
- [ ] Grid snapping toggles
- [ ] Vertex welding/merging tool
- [ ] Face splitting/subdividing
- [ ] Selection box (drag to select multiple)

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

## Technical Details

- **Engine**: Custom software rasterizer in Rust
- **UI Framework**: Macroquad for windowing and input
- **Audio**: rustysynth for SF2 soundfont playback, cpal for native audio output
- **Icon Font**: [Lucide](https://lucide.dev/) for toolbar icons
- **Level Format**: RON (Rust Object Notation)
- **Resolution**: 320x240 (4:3 aspect ratio) or stretch-to-fill
- **Coordinate System**: Y-up, right-handed
- **Sector Size**: 1024 units (TRLE standard)
- **Reverb**: PS1 SPU emulation based on nocash PSX specifications

### WASM Texture Loading

Since WebAssembly can't enumerate directories at runtime, textures are loaded via a manifest system:

1. `build.rs` scans `assets/textures/` at compile time
2. Generates `assets/textures/manifest.txt` listing all packs and files
3. WASM runtime loads textures async from the manifest
4. Native builds still use direct filesystem enumeration

## Acknowledgments

The software rasterizer is based on [tipsy](https://github.com/nkanaev/tipsy), a minimal PS1-style software renderer written in C99 by nkanaev.

## License

MIT





