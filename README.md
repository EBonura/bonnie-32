# Bonnie Engine

[![Version](https://img.shields.io/badge/version-0.1.3-blue.svg)](https://github.com/ebonura/bonnie-engine/releases)
[![License](https://img.shields.io/badge/license-MIT-green.svg)](LICENSE)
[![Platform](https://img.shields.io/badge/platform-Windows%20%7C%20macOS%20%7C%20Linux%20%7C%20Web-lightgrey.svg)]()

A PS1-style game engine and level editor created with the goal of answering: **"How would a Souls-like have looked on a PS1?"**

**[Live Demo](https://ebonura.github.io/bonnie-engine)** | **[itch.io](https://bonnie-games.itch.io/)** | **[Buy Me a Coffee](https://buymeacoffee.com/bonniegames)**

## Description

Bonnie Engine is a unified development environment for creating games with authentic PlayStation 1 aesthetics. It includes a software rasterizer that faithfully recreates PS1 hardware limitations, a TR1-style level editor, a music tracker with PS1 SPU reverb emulation, and runs both natively and in the browser via WebAssembly.

### Key Features

- **Authentic PS1 Rendering** - Affine texture mapping, vertex snapping, Gouraud shading, 320x240 resolution, no perspective correction
- **Room-Based Level Editor** - TR1-style sector grid with dual 2D/3D viewports, texture painting, and portal culling
- **Music Tracker** - 8-channel tracker with 37-key piano, SF2 soundfont support, and 10 authentic PsyQ SDK reverb presets
- **Cross-Platform** - Runs natively on Windows/macOS/Linux and in browsers via WASM
- **Gamepad Support** - Full controller support with Elden Ring-style Souls-like controls

## Installation

### Requirements

- Rust 1.70+ (for building from source)
- Cargo

### Build from Source

```bash
git clone https://github.com/EBonura/bonnie-engine.git
cd bonnie-engine
cargo run --release
```

### Web Build

```bash
cargo build --release --target wasm32-unknown-unknown
python3 -m http.server 8000
# Open http://localhost:8000 in your browser
```

### Pre-built Binaries

Download from [itch.io](https://bonnie-games.itch.io/) or the [GitHub Releases](https://github.com/EBonura/bonnie-engine/releases) page.

## Usage

The engine is organized into tabs accessible from the top bar:

### Home
Introduction page with FAQ, motivation, and links. Explains the project's goals and how to get started with each tool.

### World Editor
TRLE-inspired room-based level editor with a sector grid system (1024 world units per sector, 256 units per height click). Features:
- **2D Grid View**: Top-down, front, or side projection for precise editing
- **3D Viewport**: Software-rendered preview with free camera or orbit mode
- **Tools**: Select, Draw Floor, Draw Wall, Draw Diagonal Wall, Draw Ceiling, Place Object
- **Panels**: Texture palette for painting faces, properties panel for vertex colors and face settings
- **Portals**: Connect rooms together for seamless traversal

### Assets
PicoCAD-inspired low-poly mesh modeler with a 4-panel viewport layout. Features:
- **Viewports**: Perspective, Top, Front, Side views (Space to toggle fullscreen)
- **Selection Modes**: Vertex (1), Edge (2), Face (3)
- **Transform Tools**: Move (G), Rotate (R), Scale (T) with Blender-style modal editing
- **Operations**: Extrude faces, OBJ import, shared texture atlas
- **View Modes**: Build mode for geometry, Texture mode (V) for UV editing

### Music Tracker
Pattern-based music tracker inspired by Picotron's design. Features:
- **8 Channels**: Each with instrument, pan, modulation, and expression controls
- **Pattern Editor**: Note, volume, and effect columns with keyboard input
- **Arrangement View**: Sequence patterns into a full song
- **SF2 Soundfonts**: Load and preview instruments via piano keyboard
- **PS1 SPU Reverb**: 10 authentic PsyQ SDK reverb presets (Room, Studio, Hall, etc.)
- **SPU Resampling**: Optional sample rate reduction for authentic lo-fi sound

### Game
Test your level in real-time with ECS-based game systems. Features:
- **Camera Modes**: Third-person character follow or free-fly spectator
- **Controls**: Keyboard/mouse or gamepad with auto-detection (Xbox/PlayStation layouts)
- **Debug Overlay**: Performance timings, player stats, render breakdown
- **FPS Limit**: 30 FPS (authentic), 60 FPS, or unlocked

## Roadmap

See [BACKLOG.md](BACKLOG.md) for planned features, known issues, and development priorities.

## Support

- **Issues**: [GitHub Issues](https://github.com/EBonura/bonnie-engine/issues)
- **Discussions**: [GitHub Discussions](https://github.com/EBonura/bonnie-engine/discussions)

## Contributing

Contributions are welcome! Please open an issue first to discuss what you'd like to change.

## Authors and Acknowledgments

**Created by [Emanuele Bonura](https://bonnie-games.itch.io/)**

The software rasterizer is based on [tipsy](https://github.com/nkanaev/tipsy) by nkanaev.

### Texture Credits

- [Retro Texture Pack](https://little-martian.itch.io/retro-textures-pack) by Little Martian
- [Low Poly 64x64 Textures](https://phobicpaul.itch.io/low-poly-64x64-textures) by PhobicPaul
- [Quake-Like Texture Pack](https://level-eleven-games.itch.io/quake-like-texture-pack) by Level Eleven Games
- [Dark Fantasy Townhouse](https://level-eleven-games.itch.io/dark-fantasy-townhouse-64x64-texture-pack) by Level Eleven Games
- [Tiny Texture Pack 1](https://screamingbrainstudios.itch.io/tiny-texture-pack), [2](https://screamingbrainstudios.itch.io/tiny-texture-pack-2), [3](https://screamingbrainstudios.itch.io/tiny-texture-pack-3) by Screaming Brain Studios

## License

[MIT](LICENSE)

## Project Status

**Active Development** - This project is under active development. Expect breaking changes between versions.
