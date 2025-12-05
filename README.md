# bonnie-rs

**Mission:** Answer the question "How would a Souls-like have looked on a PS1?"

A PS1-style software rasterizer engine with an integrated level editor. Everything in this project is tooling to achieve authentic PlayStation 1 aesthetics for a Souls-like experience.

## Features

### Authentic PS1 Rendering
- **Affine texture mapping** - Characteristic warpy textures
- **Vertex snapping** - Jittery vertices at low precision
- **Gouraud shading** - Smooth per-vertex lighting
- **Low resolution** - Native 320x240 rendering
- **No perspective correction** - True to PS1 hardware limitations

### TR1-Style Level System
- **Room-based architecture** - Levels divided into connected rooms
- **Portal culling** - Only render visible rooms through portals
- **TRLE sector grid** - 1024-unit sectors for precise alignment
- **Textured geometry** - Multiple texture pack support

### Integrated Level Editor

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
- **Floor Tool** - Place 1024x1024 floor sectors
- **Ceiling Tool** - Place ceiling sectors at standard height
- **Wall Tool** - (Planned) Connect vertices to create walls
- **Texture Painting** - Click faces to apply selected texture
- **Vertex Linking** - Move coincident vertices together or independently

#### Texture Management
- Browse multiple texture packs
- Search and filter textures
- Auto-apply textures to new geometry
- Texture reference system (pack + name)

#### Workflow Features
- **Undo/Redo** - Full history for all edits
- **Cross-platform save/load**
  - Desktop: Native file dialogs
  - Browser: Import/Export JSON
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
- Left-click: Select geometry (Select mode only)
- Drag: Move vertex heights

#### 2D Grid View
- Left-click: Place floors/ceilings or select geometry
- Right-click + drag: Pan view
- Scroll wheel: Zoom in/out
- Drag vertices: Reposition on X/Z plane

#### Toolbar
- **Select**: Choose and drag geometry
- **Floor**: Place floor sectors
- **Wall**: (WIP) Create walls
- **Ceil**: Place ceiling sectors
- **Portal**: (WIP) Connect rooms
- **Link ON/OFF**: Toggle vertex linking mode

### Game Mode
- Press **Esc** to return to editor
- Right-click + drag: Look around
- WASD: Move camera
- Q/E: Move up/down
- **1/2/3**: Shading mode (None/Flat/Gouraud)
- **P**: Toggle perspective correction
- **J**: Toggle vertex jitter
- **Z**: Toggle Z-buffer

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

Live demo: [https://ebonura.github.io/bonnie-engine](https://ebonura.github.io/bonnie-engine)

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

## Roadmap

### Core Engine
- [ ] Sprite/billboard rendering (classic PS1 technique for enemies, items)
- [ ] Particle system (dust, sparks, blood splatter)
- [ ] Audio system with sequencer/tracker
- [ ] Collision detection and physics
- [ ] Character controller

### Editor
- [ ] Wall tool implementation
- [ ] Portal creation and room connectivity
- [ ] Multi-room support
- [ ] Slope/ramp tools
- [ ] Lighting system
- [ ] Entity placement (enemies, items, spawn points)

### Souls-like Features
- [ ] Lock-on targeting
- [ ] Stamina-based combat
- [ ] Bonfire checkpoints
- [ ] Death/corpse run mechanics
- [ ] Boss arenas and encounters

## Technical Details

- **Engine**: Custom software rasterizer in Rust
- **UI Framework**: Macroquad for windowing and input
- **Level Format**: RON (Rust Object Notation)
- **Resolution**: 320x240 (4:3 aspect ratio)
- **Coordinate System**: Y-up, right-handed
- **Sector Size**: 1024 units (TRLE standard)

## Acknowledgments

The software rasterizer is based on [tipsy](https://github.com/nkanaev/tipsy), a minimal PS1-style software renderer written in C99 by nkanaev.

## License

MIT
