# BONNIE-32

[![Version](https://img.shields.io/badge/version-0.1.7-blue.svg)](https://github.com/ebonura/bonnie-32/releases)
[![License](https://img.shields.io/badge/license-MIT-green.svg)](LICENSE)
[![Platform](https://img.shields.io/badge/platform-Windows%20%7C%20macOS%20%7C%20Linux%20%7C%20Web-lightgrey.svg)]()

A fantasy console for making PS1-era 3D games.

**[Try it in your browser](https://ebonura.github.io/bonnie-32)** | **[itch.io](https://bonnie-games.itch.io/)** | **[Buy Me a Coffee](https://buymeacoffee.com/bonniegames)**

---

## Why This Exists

I always imagined how a Souls-like would have been as if it were a late PS1 title. Tried Godot, Love2D, Picotron, even targeting real hardware, but nothing quite fit so I built my own.

BONNIE-32 is a fantasy console aimed at 5th generation hardware, specifically targeting PS1, that gives you:
- A renderer designed around PS1 hardware constraints (320×240, affine textures, vertex snapping)
- Integrated tools for modeling, texturing, music, and level design
- 100% Rust with modern toolchain, works the same in browser and native (Windows, macOS, Linux)



## How the renderer works

The renderer is a software rasterizer written in Rust. It recreates PS1 visuals through optional rendering modes, inspired from how the hardware handled graphics:

| Feature | Effect |
|---------|--------|
| **Affine texture mapping** | No perspective correction, textures warp |
| **Vertex snapping** | Integer coordinates, geometry jitters |
| **RGB555 color** | 15-bit color with optional dithering |
| **No sub-pixel precision** | Polygons snap to pixel grid |
| **Painter's algorithm** | Back-to-front sorting instead of Z-buffer |

The renderer implements these natively as actual rendering techniques, not post-processing.

## Integrated tools

Built-in editors for the full workflow:

### World Editor
<img src="docs/screenshot-world-editor.png" width="600" alt="World Editor">

Room-based level editor with a Tomb Raider-style sector grid.
- 2D grid views (top/front/side) and 3D preview
- Texture painting with palette support
- Portal system for connecting rooms
- Object placement

### Asset Editor
<img src="docs/screenshot-asset-editor.png" width="600" alt="Asset Editor">

Low-poly mesh modeler inspired by PicoCAD and Blender.
- 4-panel viewport (perspective + orthographic)
- G/R/T for grab, rotate, scale
- Texture atlas editor with indexed color
- UV editor
- OBJ import

### Music Tracker
<img src="docs/screenshot-music-tracker.png" width="600" alt="Music Tracker">

Pattern-based tracker with PS1 audio emulation.
- 8 channels, SF2 soundfont support
- PsyQ SDK reverb presets (Room, Studio, Hall, Space Echo...)
- MIDI keyboard input
- Tracker effects (arpeggio, vibrato, portamento)

### Game Mode
To test level being worked on.
- Third-person or free-fly camera
- Gamepad support
- Debug overlay

## Quick Start

### Try it now
[Web demo](https://ebonura.github.io/bonnie-32) runs in your browser.

### Build from source
```bash
git clone https://github.com/EBonura/bonnie-32.git
cd bonnie-32
cargo run --release
```

### Pre-built binaries
Download from [itch.io](https://bonnie-games.itch.io/) or [GitHub Releases](https://github.com/EBonura/bonnie-32/releases).

**macOS users**: Run from the extracted directory and remove quarantine if needed:
```bash
xattr -cr ~/Downloads/bonnie-32-macos-*
cd ~/Downloads/bonnie-32-macos-*
./bonnie-32
```

## Controls Reference

### Asset Editor
| Key | Action |
|-----|--------|
| `1` `2` `3` | Vertex / Edge / Face mode |
| `G` | Grab (move) |
| `R` | Rotate |
| `T` | Scale |
| `E` | Extrude |
| `X` `Y` `Z` | Axis constraint |
| `Space` | Toggle fullscreen viewport |
| `V` | Toggle UV/Build mode |

### World Editor
| Key | Action |
|-----|--------|
| `WASD` | Pan camera |
| `Shift+WASD` | Fast pan |
| `=` `-` | Zoom in/out |
| `1-6` | Select tool |

### Music Tracker
| Key | Action |
|-----|--------|
| `Z`-`M` / `Q`-`P` | Piano keys (2 octaves) |
| `Space` | Play/Stop |
| `Arrow keys` | Navigate pattern |

## Technical specs

Hardware constraints:

| Spec | Value |
|------|-------|
| Resolution | 320×240 |
| Color depth | RGB555 (15-bit) |
| Texture format | 4-bit or 8-bit indexed |
| Max texture size | 256×256 |
| Audio | 8 channels, 44.1kHz |


## Contributing

Contributions welcome! Please open an issue first to discuss changes.

See [CONTRIBUTING.md](CONTRIBUTING.md) for guidelines.

## Credits

Made by [Emanuele Bonura](https://bonnie-games.itch.io/).

Software rasterizer based on [tipsy](https://github.com/nkanaev/tipsy) by nkanaev.

### Included Texture Packs
- [Retro Texture Pack](https://little-martian.itch.io/retro-textures-pack) by Little Martian
- [Low Poly 64x64 Textures](https://phobicpaul.itch.io/low-poly-64x64-textures) by PhobicPaul
- [Quake-Like Texture Pack](https://level-eleven-games.itch.io/quake-like-texture-pack) by Level Eleven Games
- [Dark Fantasy Townhouse](https://level-eleven-games.itch.io/dark-fantasy-townhouse-64x64-texture-pack) by Level Eleven Games
- [Tiny Texture Pack 1](https://screamingbrainstudios.itch.io/tiny-texture-pack), [2](https://screamingbrainstudios.itch.io/tiny-texture-pack-2), [3](https://screamingbrainstudios.itch.io/tiny-texture-pack-3) by Screaming Brain Studios

See [THIRD_PARTY.md](THIRD_PARTY.md) for full license information.

## License

[MIT](LICENSE)

---

*BONNIE-32 is under active development. Expect breaking changes between versions.*
