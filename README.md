# Bonnie Engine

[![Version](https://img.shields.io/badge/version-0.1.3-blue.svg)](https://github.com/ebonura/bonnie-engine/releases)
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

### Controller Support
- **Gamepad input** - Full controller support (gilrs on native, Web Gamepad API on WASM)
- **Elden Ring controls** - Familiar Souls-like button layout
- **Unified input** - Seamlessly switch between keyboard and controller

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

#### Controller (Elden Ring style)
- **Left Stick**: Move (relative to camera)
- **Right Stick**: Rotate camera
- **A**: Jump
- **B (hold)**: Sprint
- **Start**: Options menu

#### Keyboard/Mouse
- **WASD**: Move
- **Space**: Jump
- **Shift (hold)**: Sprint
- **Right-click + drag**: Rotate camera
- **Esc/Start**: Options menu

### Music Tracker
- **37-key piano** - 3 octaves with full keyboard mapping
- **Z to /**: Piano keys (bottom row)
- **Q to ]**: Piano keys (top row, continues seamlessly)
- **Numpad +/-**: Octave up/down
- **Space**: Play/Pause
- **Esc**: Stop playback
- **Apostrophe (`)**: Note off
- **Arrow keys**: Navigate pattern
- **Home/End**: Jump to start/end of pattern
- **Per-channel audio settings** - Each channel has its own sample rate, reverb type, wet level, pan, mod, and expression
- **PS1 SPU sample rates** - OFF, 44kHz, 22kHz, 11kHz, 5kHz per channel
- **PS1 reverb presets** - 10 authentic PsyQ SDK presets per channel (Off, Room, StudioS, StudioM, StudioL, Hall, HalfEcho, SpaceEcho, Chaos, Delay)
- **Effect buttons** - Quick insert effects (Arp, SlideUp, SlideDown, Porta, Vib, VolSlide, Vol, Expr, Mod, Pan) with configurable amount

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

- **Tiny Texture Pack 1, 2, 3** by Screaming Brain Studios
  https://screamingbrainstudios.itch.io/tiny-texture-pack
  https://screamingbrainstudios.itch.io/tiny-texture-pack-2
  https://screamingbrainstudios.itch.io/tiny-texture-pack-3

## Backlog & Roadmap

See [BACKLOG.md](BACKLOG.md) for planned features, known issues, and future improvements.

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

