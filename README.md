# Bonnie Engine

[![Version](https://img.shields.io/badge/version-0.1.3-blue.svg)](https://github.com/ebonura/bonnie-engine/releases)
[![License](https://img.shields.io/badge/license-MIT-green.svg)](LICENSE)
[![Platform](https://img.shields.io/badge/platform-Windows%20%7C%20macOS%20%7C%20Linux%20%7C%20Web-lightgrey.svg)]()

A PS1-style game engine and level editor that answers the question: **"How would a Souls-like have looked on a PS1?"**

**[Live Demo](https://ebonura.github.io/bonnie-engine)** | **[itch.io](https://bonnie-games.itch.io/)** | **[Buy Me a Coffee](https://buymeacoffee.com/bonniegames)**

## Description

Bonnie Engine is a unified development environment for creating games with authentic PlayStation 1 aesthetics. It includes a software rasterizer that faithfully recreates PS1 hardware limitations, a TR1-style level editor, a music tracker with PS1 SPU reverb emulation, and runs both natively and in the browser via WebAssembly.

### Key Features

- **Authentic PS1 Rendering** - Affine texture mapping, vertex snapping, Gouraud shading, 320x240 resolution, no perspective correction
- **Room-Based Level Editor** - TR1-style sector grid with dual 2D/3D viewports, texture painting, and portal culling
- **Music Tracker** - 8-channel tracker with 37-key piano, SF2 soundfont support, and 10 authentic PsyQ SDK reverb presets
- **Cross-Platform** - Runs natively on Windows/macOS/Linux and in browsers via WASM
- **Gamepad Support** - Full controller support with Elden Ring-style Souls-like controls

## Visuals

![Editor Screenshot](https://img.itch.zone/aW1nLzE4NjM0NTk4LnBuZw==/original/8QWKZW.png)

*World Editor with 3D viewport and texture browser*

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

### World Editor

| Input | Action |
|-------|--------|
| WASD | Move camera |
| Q/E | Camera up/down |
| Right-click + drag | Rotate camera |
| Left-click | Select / Place geometry |
| Shift + click | Multi-select |
| Delete | Remove selected |

### Game Mode

| Input | Controller | Action |
|-------|------------|--------|
| WASD | Left Stick | Move |
| Space | A / Cross | Jump |
| Shift | B / Circle | Sprint |
| Mouse | Right Stick | Camera |

### Music Tracker

| Input | Action |
|-------|--------|
| Z to / | Piano keys (lower octave) |
| Q to ] | Piano keys (upper octaves) |
| Space | Play/Pause |
| Arrow keys | Navigate pattern |
| Numpad +/- | Octave up/down |

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
