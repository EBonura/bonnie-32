# BONNIE-32

[![Version](https://img.shields.io/badge/version-0.1.7-blue.svg)](https://github.com/ebonura/bonnie-32/releases)
[![License](https://img.shields.io/badge/license-MIT-green.svg)](LICENSE)
[![Platform](https://img.shields.io/badge/platform-Windows%20%7C%20macOS%20%7C%20Linux%20%7C%20Web-lightgrey.svg)]()

**A fantasy console for PS1-era 3D games.** Think PICO-8, but for low-poly 3D.

**[Try it in your browser](https://ebonura.github.io/bonnie-32)** | **[itch.io](https://bonnie-games.itch.io/)** | **[Buy Me a Coffee](https://buymeacoffee.com/bonniegames)**

---

## What is BONNIE-32?

PICO-8 unlocked retro 2D gamedev with its constraints and all-in-one tooling. BONNIE-32 aims to do the same for late 90s-style 3D.

Like a fantasy console, BONNIE-32 provides:
- **Fixed constraints** that encourage creativity (PS1 hardware limitations)
- **Integrated tools** for the complete workflow (modeling, texturing, music, levels)
- **A unified platform** that runs the same everywhere (native + browser via WASM)

Everything is built from scratch in Rust: the software rasterizer, the editor UI, the level format. No shaders faking the look - the renderer actually works like PS1 hardware.

## The PS1 Aesthetic

The software rasterizer recreates the quirks that defined the PS1 look:

| Feature | What it does |
|---------|-------------|
| **Affine texture mapping** | No perspective correction = signature texture warping |
| **Vertex snapping** | Integer coordinates = subtle jitter on moving objects |
| **RGB555 color** | 15-bit color with optional dithering |
| **No sub-pixel precision** | Polygons "pop" when they move |
| **No Z-buffer** | Painter's algorithm with face sorting |

These aren't post-processing effects - they're how the renderer actually works.

## Integrated Tools

BONNIE-32 includes everything you need to create PS1-style games:

### World Editor
<img src="docs/screenshot-world-editor.png" width="600" alt="World Editor">

TR1-inspired room-based level editor with a sector grid system.
- 2D grid view (top/front/side) + 3D software-rendered preview
- Texture painting with palette support
- Portal system for connecting rooms
- Object placement and properties

### Asset Editor
<img src="docs/screenshot-asset-editor.png" width="600" alt="Asset Editor">

PicoCAD-inspired low-poly mesh modeler.
- 4-panel viewport (perspective + orthographic views)
- Blender-style controls: G (grab), R (rotate), S (scale)
- Per-object texture atlases with indexed color
- UV editor with direct vertex dragging
- OBJ import support

### Music Tracker
<img src="docs/screenshot-music-tracker.png" width="600" alt="Music Tracker">

Pattern-based tracker for authentic PS1 audio.
- 8 channels with SF2 soundfont support
- 10 authentic PsyQ SDK reverb presets (Room, Studio, Hall, Space Echo...)
- MIDI keyboard input with hot-plug detection
- Classic tracker effects (arpeggio, vibrato, portamento)

### Game Mode
Test your levels in real-time.
- Third-person character controller or free-fly camera
- Gamepad support (Xbox/PlayStation layouts)
- Debug overlay with performance timings

## Quick Start

### Try it now
Open the [web demo](https://ebonura.github.io/bonnie-32) - no install needed.

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
| `S` | Scale |
| `E` | Extrude |
| `X` `Y` `Z` | Axis constraint |
| `Space` | Toggle fullscreen viewport |
| `V` | Toggle UV/Build mode |

### World Editor
| Key | Action |
|-----|--------|
| `WASD` | Pan camera |
| `Shift+WASD` | Fast pan |
| `Q` `E` | Zoom in/out |
| `1-6` | Select tool |

### Music Tracker
| Key | Action |
|-----|--------|
| `Z`-`M` / `Q`-`P` | Piano keys (2 octaves) |
| `Space` | Play/Stop |
| `Arrow keys` | Navigate pattern |

## Technical Specs

Embracing PS1-era constraints:

| Spec | Value |
|------|-------|
| Resolution | 320×240 |
| Color depth | RGB555 (15-bit) |
| Texture format | 4-bit or 8-bit indexed |
| Max texture size | 256×256 |
| Audio | 8 channels, 44.1kHz |

## Why not Unity/Godot?

Modern engines are designed for modern games. Getting true PS1-style rendering means fighting against their design - disabling features, adding post-processing to fake limitations.

BONNIE-32 embraces the constraints from the ground up. The renderer doesn't have perspective-correct textures to disable - it simply doesn't do them. The result is more authentic and often simpler to work with.

## The Goal

Ship a Souls-like game as if it were a late PS1 title. BONNIE-32 and its tools are open source; the game will be a shareware demo + full release on Steam.

## Contributing

Contributions welcome! Please open an issue first to discuss changes.

See [CONTRIBUTING.md](CONTRIBUTING.md) for guidelines.

## Credits

**Created by [Emanuele Bonura](https://bonnie-games.itch.io/)**

The software rasterizer is based on [tipsy](https://github.com/nkanaev/tipsy) by nkanaev.

### Texture Packs
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
