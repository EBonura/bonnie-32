# docs/

This folder contains the **web deployment source** for BONNIE-32's GitHub Pages site, not traditional documentation.

## Contents

- `index.html` - Main HTML entry point for web builds
- `audio-processor.js` - AudioWorklet processor for low-latency audio
- `screenshot-*.png` - Screenshots for the main README
- `favicon-*.png`, `apple-touch-icon.png` - Web app icons
- `mq_js_bundle.js` - (gitignored) Downloaded from macroquad during builds

## Build Process

When running `cargo xtask build-web`, files from this folder are copied to `dist/web/` along with:
- Compiled WASM binary
- Assets from `assets/`
- Downloaded macroquad JS bundle

## GitHub Pages

GitHub Pages serves this folder directly at: https://ebonura.github.io/bonnie-32
