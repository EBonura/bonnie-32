# bonnie-rs

PS1-style software rasterizer engine - a souls-like game engine with authentic PlayStation 1 rendering.

## Features

- Affine texture mapping (warpy textures)
- Vertex snapping (jittery vertices)
- Gouraud shading
- Low resolution (320x240)
- TR1-style room-based levels with portal culling
- TRLE-inspired level editor

## Controls

### Editor Mode
- Click 'Play' to test level
- Right-click + drag: Look around (3D viewport)
- WASD: Move camera
- Q/E: Move up/down
- Left-click: Select/paint textures on faces
- Drag vertices to edit geometry

### Game Mode
- Press Esc to return to editor
- Right-click + drag: Look around
- WASD: Move camera
- Q/E: Move up/down
- 1/2/3: Shading mode (None/Flat/Gouraud)
- P: Toggle perspective correction
- J: Toggle vertex jitter
- Z: Toggle Z-buffer

## Building

```bash
cargo run
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

## Roadmap

- [x] Cross-platform file save/load (browser: Import/Export, desktop: Open/Save As)
- [ ] Audio system with sequencer/tracker
- [ ] Particle system (dust, sparks, blood splatter)
- [ ] Sprite/billboard rendering (classic PS1 technique for trees, items, enemies)
- [ ] UI system (XMB-style menus, inventory screens)

## Acknowledgments

The software rasterizer is based on [tipsy](https://github.com/nkanaev/tipsy), a minimal PS1-style software renderer written in C99 by nkanaev.

## License

MIT
