# Bonnie Engine

[![Version](https://img.shields.io/badge/version-0.1.0-blue.svg)](https://github.com/ebonura/bonnie-engine/releases)
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
- **Low resolution** - Native 320x240 rendering
- **No perspective correction** - True to PS1 hardware limitations
- **Z-sorted room boundaries** - Depth-tested wireframe overlays

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
  - Camera controls (WASD + Q/E for height)
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
- **Texture Painting** - Click faces to apply selected texture
- **Vertex Linking** - Move coincident vertices together or independently
- **Face Deletion** - Delete/Backspace removes selected floors, ceilings, and walls

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
- **Portal**: (WIP) Connect rooms
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
- **Z to /**: Piano keys (bottom row, C to E)
- **Q to ]**: Piano keys (top row, F to C - continues from bottom)
- **Numpad +/-**: Octave up/down
- **Space**: Play/Pause
- **Esc**: Stop playback
- **F9/F10**: Edit step down/up
- **Apostrophe (`)**: Note off
- **Arrow keys**: Navigate pattern
- **Home/End**: Jump to start/end of pattern

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

### Overall / Meta

- [ ] Remove AI/Claude mentions from git history (use `git filter-branch` or BFG Repo Cleaner - backup first!)
- [ ] Built-in example browser: open browser window showing bundled maps/models with 3D preview (list left, 3D view right, info bottom)

---

### Rendering Pipeline

- [x] **Per-vertex colors with texture modulation**: RGB color per vertex that multiplies texture: `final = (texel * vertex_color) / 128`. Enables Wipeout 2097-style track tinting. Values > 128 brighten, < 128 darken. UI shows 4-vertex swatch grid with preset colors.
- [ ] **Dynamic lighting support**: Recalculate affected vertex colors per frame for point lights (like Tomb Raider flares). CPU calculates distance/angle to light, updates vertex colors, GPU interpolates via Gouraud.
- [ ] Add aspect ratio toggle: Currently locked to PS1 4:3 (320x240), add icon to toggle full available space (affects World Editor and Modeler)

---

### World Editor - 3D Viewport

#### Bugs/Polish
- [ ] Context-sensitive bottom bar: Show left/right click actions; when right-clicking show WASD/QE bindings

#### Major Features
- [ ] **Implement portals**: Create and visualize room connections (Portal struct exists in geometry.rs)

#### Future
- [ ] Entity system design: Research TrenchBroom and Tomb Raider Level Editor for spawn points, interactables (ladders, chests, doors), triggers, lights. Each entity type configurable in properties panel

---

### Music Editor

#### Piano Keyboard
- **3-octave continuous layout** across two keyboard rows
- **Bottom row (Z to /)**: C to E (~1.5 octaves)
- **Top row (Q to ])**: F to C (continues seamlessly from bottom row)
- **Octave control**: Numpad +/- only (regular keys are piano keys)
- **Note-off**: Apostrophe key (`)

#### High Priority (Bugs)
- [ ] Fix left-side knob reset bug: Knobs reset to max value when dragged on left side
- [ ] Thicken knob perimeter: Circle outline too thin for easy reading
- [ ] Configurable pattern length: Currently hardcoded to 64 rows - should be adjustable
- [ ] Fix Pause icon: Currently shows pen icon instead of pause
- [ ] Fix Arrangement icon: Shows lock instead of arrangement-related icon
- [ ] Fix Instruments icon: Shows rotating arrow - need better icon (piano/synth?)

#### Medium Priority
- [ ] Add per-note vs channel FX toggle: At top of each channel, toggle between per-note FX or channel-wide FX (with greyed-out section when not active)
- [ ] Add bottom status bar: Same as World Editor, context-sensitive shortcuts/suggestions

#### Future
- [ ] Custom instrument editor: Tab for building custom instruments beyond SF2 soundfonts

---

### Assets

#### High Priority
- [ ] Fix transform tool icons: Select/Move/Rotate/Scale all show the same select icon

#### Medium Priority
- [ ] Add PS1 aspect ratio toggle: Same feature as World Editor

#### Future
- [ ] Pixel art painting tools: Built-in tools specific for texture painting
- [ ] PS1 color depth constraints: Limit to PS1 palette (toggleable)
- [ ] VRAM usage counter: PS1 had 1MB VRAM (~700-900KB available after screen buffer). Display usage with warning when exceeded
- [ ] Polygon count indicator: Green/yellow/red based on PS1-realistic counts
- [ ] Multi-object animation: Design system for animations where 2 objects interact (e.g., player pulling lever)

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
- [ ] Portal creation and room connectivity
- [ ] Multi-room support
- [ ] Slope/ramp tools
- [ ] Collision detection and physics
- [ ] Character controller (movement, jumping)
- [ ] Camera system (third-person, lock-on)

### UI & Settings
- [ ] Options menu in-game (resolution, PS1 effects toggles)
- [ ] Editor toolbar: PS1 effects toggles (vertex jitter, affine mapping, etc.)
- [ ] Resolution selector (240p, 480p, native)
- [ ] HUD system (health, stamina bars)

### Rendering & Effects
- [ ] Sprite/billboard rendering (classic PS1 technique for enemies, items)
- [ ] Particle system (dust, sparks, blood splatter)
- [ ] Fog system (distance-based fade)

### Core Systems
- [ ] Entity system (enemies, items, spawn points)
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
- **Icon Font**: [Lucide](https://lucide.dev/) for toolbar icons
- **Level Format**: RON (Rust Object Notation)
- **Resolution**: 320x240 (4:3 aspect ratio)
- **Coordinate System**: Y-up, right-handed
- **Sector Size**: 1024 units (TRLE standard)

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
