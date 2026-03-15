# Bonnie-32 v2: Architecture Plan

> North Star: [Hazel Engine](https://github.com/TheCherno/Hazel) by TheCherno
> Stack: Rust + wgpu + winit + egui (replacing macroquad)

## Overview

Bonnie-32 v2 is a **hybrid rewrite**: a clean application shell with Hazel-inspired architecture, powered by the wgpu+egui stack, with proven v1 subsystems ported verbatim. The old codebase is preserved in `v1/` for reference.

### Why Rewrite

- v1 grew organically with 3 duplicate browser implementations, a monolithic main.rs, and no formal project/asset system
- No UUID-based asset references (assets break on rename)
- No project file concept (.b32)
- No scripting system
- macroquad provided minimal UI — all widgets were hand-built from primitives
- egui provides docking panels, tree views, text input, drag-and-drop — ideal for a Hazel-style editor

### Why wgpu + egui (not macroquad)

| | macroquad (v1) | wgpu + egui (v2) |
|---|---|---|
| UI widgets | Hand-built from raw quads/text | Full immediate-mode UI library |
| Panel layout | Manual pixel math | Docking, collapsible, resizable panels |
| Text input | Custom `TextInput` widget | Built-in with selection, clipboard |
| Tree views | None | `egui::CollapsingHeader`, `TreeNode` |
| Rendering control | Limited (quad-based) | Full GPU pipeline control |
| Editor experience | Functional but primitive | Professional editor UX |
| WASM support | Yes (JS bundle) | Yes (WebGPU + WebGL2 fallback) |
| Already proven | v1 | psx-studio |

The software rasterizer is unchanged — it renders to a `Vec<u8>` pixel buffer, then uploads to a `wgpu::Texture` (same pattern as psx-studio).

---

## Technology Stack

| Component | Crate | Version | Purpose |
|---|---|---|---|
| Windowing | `winit` | 0.30 | Cross-platform window + event loop |
| GPU | `wgpu` | 24 | Texture upload, fullscreen quad, WebGPU/WebGL |
| UI | `egui` | 0.31 | Panels, widgets, text input, trees |
| UI+GPU bridge | `egui-wgpu` | 0.31 | Render egui on wgpu |
| UI+Window bridge | `egui-winit` | 0.31 | Feed winit events to egui |
| Async executor | `pollster` | 0.4 | Block on wgpu futures (native only) |
| Serialization | `serde` + `ron` | 1 / 0.8 | RON format for all data |
| Compression | `brotli` | 8.0 | Asset compression on disk |
| UUIDs | `uuid` | 1 | Asset identity |
| Image loading | `image` | 0.25 | PNG/JPEG/BMP textures |
| Audio | `cpal` | 0.15 | Audio output (native + WASM Web Audio) |
| Gamepad | `gilrs` | 0.11 | Gamepad input (native) |
| Scripting | `mlua` | 0.10 | Lua VM for game logic |
| Byte casting | `bytemuck` | 1 | Safe pixel data casting |
| Logging | `log` + `env_logger` | 0.4 / 0.11 | Debug logging |

---

## Directory Structure

```
bonnie-32/
├── v1/                          # Complete v1 backup (reference only)
│   ├── src/
│   ├── assets/
│   ├── Cargo.toml
│   └── ...
│
├── src/
│   ├── main.rs                  # winit entry point, event loop, wgpu init
│   ├── app.rs                   # Application state, mode switching
│   │
│   ├── platform/                # Platform abstraction
│   │   ├── mod.rs
│   │   ├── renderer.rs          # wgpu renderer (fullscreen quad + egui)
│   │   ├── input.rs             # Keyboard, mouse, gamepad state
│   │   └── audio.rs             # cpal audio output stream
│   │
│   ├── asset/                   # Asset system (Hazel-inspired)
│   │   ├── mod.rs
│   │   ├── handle.rs            # AssetHandle (UUID wrapper)
│   │   ├── registry.rs          # AssetRegistry (UUID -> path mapping, persisted)
│   │   ├── manager.rs           # AssetManager (load, cache, resolve)
│   │   ├── types.rs             # AssetType enum, AssetSource enum
│   │   └── component.rs         # AssetComponent variants (ported from v1)
│   │
│   ├── project/                 # Project system
│   │   ├── mod.rs
│   │   └── manifest.rs          # ProjectManifest (.b32 file)
│   │
│   ├── scene/                   # Scene/level system
│   │   ├── mod.rs
│   │   ├── level.rs             # Level, Room, Sector (ported from v1 world/)
│   │   ├── instance.rs          # AssetInstance (placed objects with AssetHandle)
│   │   └── render.rs            # Scene rendering pipeline
│   │
│   ├── editor/                  # Editor panels (Hazel-inspired)
│   │   ├── mod.rs
│   │   ├── context.rs           # EditorContext (shared selection, project state)
│   │   ├── panel.rs             # EditorPanel trait
│   │   ├── content_browser.rs   # Unified asset browser (replaces 3 v1 browsers)
│   │   ├── hierarchy.rs         # Scene hierarchy panel (rooms, entities)
│   │   ├── inspector.rs         # Component inspector panel
│   │   ├── viewport.rs          # 3D viewport panel (software rasterizer output)
│   │   └── toolbar.rs           # Top toolbar / mode switcher
│   │
│   ├── modeler/                 # Asset editor (3D mesh modeler)
│   │   ├── mod.rs
│   │   ├── state.rs             # Modeler state (ported + adapted)
│   │   ├── mesh_editor.rs       # Mesh editing tools (ported)
│   │   └── viewport.rs          # Modeler viewport
│   │
│   ├── tracker/                 # Music editor
│   │   ├── mod.rs
│   │   ├── spu/                 # PS1 SPU engine (ported verbatim)
│   │   │   ├── mod.rs
│   │   │   ├── voice.rs
│   │   │   ├── reverb.rs
│   │   │   ├── adpcm.rs
│   │   │   ├── types.rs
│   │   │   ├── tables.rs
│   │   │   └── convert.rs
│   │   ├── audio.rs             # Audio engine / mixer (ported)
│   │   ├── state.rs             # Tracker state (ported)
│   │   └── pattern.rs           # Pattern data (ported)
│   │
│   ├── scripting/               # Lua scripting (NEW)
│   │   ├── mod.rs
│   │   ├── runtime.rs           # Lua VM lifecycle
│   │   ├── api.rs               # Engine API exposed to Lua (ScriptGlue pattern)
│   │   └── console.rs           # REPL panel
│   │
│   ├── rasterizer/              # PS1 software renderer (ported verbatim)
│   │   ├── mod.rs
│   │   ├── render.rs            # Framebuffer, triangle rasterization
│   │   ├── types.rs             # Color, Texture, Vertex, Face
│   │   ├── math.rs              # Vec3, Vec2, Mat4
│   │   ├── camera.rs            # Camera
│   │   ├── draw.rs              # Line drawing, grids
│   │   ├── ray.rs               # Ray casting / picking
│   │   ├── fixed.rs             # PS1 fixed-point math
│   │   └── constants.rs         # Resolution constants
│   │
│   └── texture/                 # Texture system (ported)
│       ├── mod.rs
│       └── user_texture.rs      # Indexed color textures (CLUT)
│
├── assets/                      # Runtime assets (fonts, icons, soundfonts, samples)
│   ├── runtime/
│   │   ├── fonts/
│   │   ├── icons/
│   │   ├── branding/
│   │   └── soundfonts/
│   └── samples/                 # Bundled sample assets
│       ├── levels/
│       ├── assets/
│       ├── meshes/
│       ├── songs/
│       └── texture-packs/
│
├── docs/                        # Documentation + GitHub Pages
│   ├── V2_PLAN.md               # This file
│   └── index.html               # Web deployment landing page
│
├── Cargo.toml
├── Cargo.lock
├── .gitignore
└── README.md
```

---

## Ported vs New Code

### Ported Verbatim (proven, tested subsystems)

| Subsystem | Source (v1/) | Destination | Notes |
|---|---|---|---|
| Software rasterizer | `v1/src/rasterizer/` | `src/rasterizer/` | Replace `macroquad::get_time()` with `std::time::Instant` |
| PS1 SPU engine | `v1/src/tracker/spu/` | `src/tracker/spu/` | No changes needed |
| Audio engine | `v1/src/tracker/audio.rs` | `src/tracker/audio.rs` | Adapt output to cpal directly (remove macroquad audio) |
| Level geometry | `v1/src/world/geometry.rs` | `src/scene/level.rs` | Types only, clean up imports |
| Asset components | `v1/src/asset/component.rs` | `src/asset/component.rs` | Add Script variant |
| Texture system | `v1/src/texture/user_texture.rs` | `src/texture/` | No changes needed |
| Pattern data | `v1/src/tracker/pattern.rs` | `src/tracker/pattern.rs` | No changes needed |

### Ported with Adaptation

| Subsystem | Source (v1/) | Changes |
|---|---|---|
| Tracker state | `v1/src/tracker/state.rs` | Replace browser references with AssetHandle |
| Tracker layout | `v1/src/tracker/layout.rs` | Rebuild UI in egui (keep logic, replace drawing) |
| Modeler state | `v1/src/modeler/state.rs` | Replace AssetInfo with AssetHandle |
| Mesh editor | `v1/src/modeler/mesh_editor.rs` | Keep algorithms, adapt to egui viewport |
| World viewport | `v1/src/editor/viewport_3d.rs` | Keep rendering, adapt input to egui |
| Scene rendering | `v1/src/scene.rs` | Wire to new Level types |

### Written Fresh

| Subsystem | Reference |
|---|---|
| Application shell (main.rs, app.rs) | psx-studio editor pattern |
| wgpu renderer | psx-studio `wgpu_renderer.rs` |
| Asset registry + handles | Hazel `AssetRegistry.hzr` pattern |
| Asset manager | Hazel `AssetManager` pattern |
| Project system (.b32) | Hazel `.hproj` pattern |
| Editor context (shared state) | Hazel `EditorContext` |
| Content browser panel | Hazel Content Browser |
| Scene hierarchy panel | Hazel Scene Hierarchy |
| Inspector panel | Hazel Inspector |
| Lua scripting | mlua + Hazel ScriptGlue |

---

## Phase 0: Bootstrap Window + Rasterizer

**Goal:** Open a window, render a test triangle via the software rasterizer, display it as a wgpu texture.

### Steps
1. Create `Cargo.toml` with core dependencies (winit, wgpu, egui, pollster, bytemuck, serde, ron)
2. Write `main.rs` with winit event loop + wgpu initialization (follow psx-studio pattern)
3. Write `platform/renderer.rs` — fullscreen quad pipeline that uploads a `Vec<u8>` RGBA framebuffer
4. Port `rasterizer/` from v1 — replace `macroquad::get_time()` with `std::time::Instant`
5. Render a test cube to the framebuffer, display via wgpu
6. Add basic egui overlay (FPS counter) to prove egui integration works

### Verification
- Window opens at 960x720 (3x PS1 resolution)
- Rotating test cube renders with PS1-style affine textures
- egui FPS text overlays on top of the rasterizer output
- Pressing Escape closes the window

### Key Architecture (from psx-studio)

```
main.rs:
  EventLoop::new() -> App::new() -> event_loop.run_app(&mut app)

App implements ApplicationHandler:
  resumed():
    - Create window
    - Init wgpu (instance, adapter, device, queue, surface)
    - Init egui (Context, State, Renderer)

  window_event():
    - Feed events to egui_state
    - Handle resize, close, keyboard
    - On RedrawRequested: do_frame()

  about_to_wait():
    - window.request_redraw()  // continuous rendering

do_frame():
  1. framebuffer.clear()
  2. render_mesh(&mut framebuffer, &cube, &camera)
  3. queue.write_texture(fb_texture, &framebuffer.pixels)
  4. egui_ctx.run(raw_input, |ctx| { egui::Window::new("Debug").show(ctx, ...) })
  5. Render pass 1: fullscreen quad with fb_texture
  6. Render pass 2: egui overlay (LoadOp::Load)
  7. surface.present()
```

---

## Phase 1: Asset System

**Goal:** UUID-based asset references that survive renames and moves.

### Core Types

```rust
// AssetHandle — the only way to reference an asset
#[derive(Clone, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub struct AssetHandle(pub Uuid);

// AssetType — what kind of asset this is
pub enum AssetType { Level, Model, Texture, TexturePack, Song, Script }

// AssetSource — where the asset comes from
pub enum AssetSource { Bundled, Project }

// Registry entry — persisted to disk
pub struct RegistryEntry {
    pub path: PathBuf,           // relative to project root
    pub asset_type: AssetType,
    pub source: AssetSource,
}

// AssetRegistry — the master UUID->path mapping
// Saved as `asset_registry.ron` in project root
pub struct AssetRegistry {
    entries: HashMap<Uuid, RegistryEntry>,
}

// AssetManager — runtime asset cache + registry
pub struct AssetManager {
    registry: AssetRegistry,
    cache: HashMap<Uuid, CachedAsset>,
}
```

### Operations
- `register(path, type) -> AssetHandle` — assign UUID to a file
- `resolve<T>(handle) -> Option<&T>` — load + cache + return
- `import(source_path) -> AssetHandle` — copy file into project, register
- `rename(handle, new_path)` — update registry entry, handle unchanged
- `scan_directory(dir)` — discover unregistered files, auto-register
- `save_registry()` / `load_registry()` — persist to `asset_registry.ron`

### Verification
- Round-trip: register -> save -> load -> resolve returns same asset
- Rename file -> old handle still resolves
- `cargo test` with unit tests

---

## Phase 2: Project System

**Goal:** Formal `.b32` project file that groups assets and persists settings.

### Project Manifest (`project.b32`)

```rust
pub struct ProjectManifest {
    pub name: String,
    pub version: String,
    pub author: String,
    pub start_level: Option<AssetHandle>,
    pub resolution: (u32, u32),    // default (320, 240)
    pub created: String,           // ISO 8601 date
}
```

### Project Directory Layout

```
my_game/
├── project.b32                  # RON-serialized ProjectManifest
├── asset_registry.ron           # UUID -> path mapping
├── levels/                      # Level files (.ron)
├── assets/                      # 3D model definitions (.ron)
├── meshes/                      # OBJ mesh data
├── textures/                    # Texture packs
├── songs/                       # Music tracks (.ron)
└── scripts/                     # Lua scripts (.lua)
```

### Workflows
- **New Project**: pick directory -> create manifest + registry + subdirs -> open
- **Open Project**: select `.b32` file -> load manifest + registry -> populate AssetManager
- **Save Project**: write manifest + registry to disk
- **Recent Projects**: store list in app config (like recent files in any editor)

### Bundled Assets
Engine ships with `assets/samples/` as read-only bundled content. The registry marks them `AssetSource::Bundled`. The content browser shows them under a "Bundled" category. Users can "import" bundled assets into their project (copies the file).

### Verification
- New project creates correct directory structure on disk
- Open project loads all registered assets
- Close + reopen -> everything restored identically

---

## Phase 3: Editor Shell + Content Browser

**Goal:** Hazel-style editor with docking panels, unified content browser, and shared state.

### Editor Context (shared across all panels)

```rust
pub struct EditorContext {
    pub project: Project,
    pub assets: AssetManager,
    pub selection: Selection,
    pub pending_action: Option<EditorAction>,
}

pub enum Selection {
    None,
    Asset(AssetHandle),
    Entity { room_index: usize, entity_index: usize },
    Room(usize),
}

pub enum EditorAction {
    OpenAssetEditor(AssetHandle),
    OpenLevel(AssetHandle),
    NavigateToTracker(AssetHandle),
}
```

### Panel Trait

```rust
pub trait EditorPanel {
    fn title(&self) -> &str;
    fn ui(&mut self, ui: &mut egui::Ui, ctx: &mut EditorContext);
}
```

### Content Browser (replaces 3 v1 browsers)

Single bottom panel showing all project assets:

```
┌─ Content Browser ──────────────────────────────────────────────┐
│ [All] [Levels] [Models] [Textures] [Songs] [Scripts]  🔍 ___  │
│ ┌─ Bundled ──────────────────────────┐ ┌─ Preview ──────────┐ │
│ │ Cathedral.ron                      │ │                     │ │
│ │ Dungeon.ron                        │ │   [3D orbit view    │ │
│ │ warrior.obj                        │ │    or stats panel]  │ │
│ ├─ Project ──────────────────────────┤ │                     │ │
│ │ my_level.ron                       │ │                     │ │
│ │ my_model.ron                       │ │                     │ │
│ └────────────────────────────────────┘ └─────────────────────┘ │
└────────────────────────────────────────────────────────────────┘
```

Features:
- Type filter tabs
- Search bar (fuzzy match on name)
- Two sections: Bundled (read-only) + Project (editable)
- Preview pane: 3D orbit for models/levels, waveform for songs, pixel view for textures
- Double-click -> opens in appropriate editor
- Right-click -> context menu (rename, delete, duplicate, export)
- Drag-and-drop from content browser into viewport (places asset instance)

### Editor Layout

```
┌──────────────────────────────────────────────────────────────┐
│ [Project] [World Editor] [Modeler] [Tracker] [Test] [Script]│
├──────────┬──────────────────────────┬────────────────────────┤
│ Scene    │                          │ Inspector              │
│ Hierarchy│      VIEWPORT            │                        │
│          │  (software rasterizer    │ [Components]           │
│ Rooms    │   output displayed as    │ [Properties]           │
│  └ Room1 │   wgpu texture)          │                        │
│    └ Ent │                          │ Transform: x y z       │
│  └ Room2 │                          │ Mesh: warrior.obj      │
│          │                          │ Script: enemy.lua      │
├──────────┴──────────────────────────┴────────────────────────┤
│                    CONTENT BROWSER                           │
└──────────────────────────────────────────────────────────────┘
```

egui provides:
- `egui::TopBottomPanel` for toolbar and content browser
- `egui::SidePanel` for hierarchy and inspector
- `egui::CentralPanel` for viewport
- All panels resizable by dragging dividers

### Verification
- All panels render and resize correctly
- Content browser shows assets filtered by type
- Selection syncs between hierarchy, viewport, and inspector
- Double-click in content browser navigates to correct editor mode

---

## Phase 4: Port Editors

**Goal:** Working world editor, modeler, and tracker using new architecture.

### 4a: World Editor
- Port viewport rendering (rasterizer -> framebuffer -> wgpu texture)
- Port sector editing tools (draw walls, floors, ceilings)
- Port asset placement (now uses AssetHandle instead of u64 IDs)
- Wire hierarchy panel to room/entity selection
- Wire inspector panel to component editing
- Port camera controls (orbit, pan, zoom) via egui input

### 4b: Asset Editor (Modeler)
- Port mesh editing (vertex/face manipulation, extrusion, etc.)
- Port multi-viewport (perspective + ortho)
- Port OBJ import
- Wire inspector for component editing (mesh, collision, textures)
- Port radial menu for quick tool access

### 4c: Music Editor (Tracker)
- Port SPU + audio engine verbatim (already cpal-ready)
- Rebuild pattern editor UI in egui (keep all logic, replace drawing)
- Song browser becomes a filtered view of the content browser
- Port keyboard shortcuts and MIDI input

### Verification
- Each editor functions equivalently to v1
- Assets save/load with UUID references
- Cross-editor navigation works (double-click asset -> opens in correct editor)

---

## Phase 5: Lua Scripting

**Goal:** Game logic via Lua scripts, following fantasy console tradition.

### Why Lua
- Fantasy console standard (Pico-8, TIC-80, Picotron)
- `mlua` crate — excellent Rust interop, WASM-compatible
- Lightweight, embeddable, fast
- Familiar to the target audience

### Architecture (Hazel ScriptGlue pattern)

```rust
pub struct ScriptRuntime {
    lua: mlua::Lua,
}

impl ScriptRuntime {
    pub fn new() -> Self {
        let lua = mlua::Lua::new();
        // Register engine API
        ScriptApi::register(&lua);
        Self { lua }
    }
}

// Engine API exposed to Lua
pub struct ScriptApi;
impl ScriptApi {
    pub fn register(lua: &mlua::Lua) {
        // Entity manipulation
        lua.globals().set("get_position", |entity_id| { ... });
        lua.globals().set("set_position", |entity_id, x, y, z| { ... });

        // Input (Pico-8 style)
        lua.globals().set("btn", |button_id| { ... });
        lua.globals().set("btnp", |button_id| { ... });

        // Audio
        lua.globals().set("sfx", |sfx_id, channel| { ... });
        lua.globals().set("music", |song_id| { ... });

        // Drawing (optional rasterizer access)
        lua.globals().set("spr", |sprite_id, x, y| { ... });
        lua.globals().set("print", |text, x, y, color| { ... });
    }
}
```

### Script Component

```rust
// Added to AssetComponent enum
AssetComponent::Script {
    script: AssetHandle,     // reference to .lua file
    // Callbacks: on_create(), on_update(dt), on_trigger(other), on_destroy()
}
```

### Script Console
egui panel with a Lua REPL for interactive debugging. Type commands, see results, inspect game state.

### Verification
- Lua script attached to entity runs `on_update(dt)` each frame
- Script can read/write entity position
- Script can respond to input (`btn()`, `btnp()`)
- Script console executes commands and prints results

---

## Phase 6: WASM + Polish

**Goal:** Web deployment via WebGPU/WebGL2, feature parity with v1 web build.

### WASM Considerations

| Concern | Solution |
|---|---|
| Async GPU init | `wasm_bindgen_futures::spawn_local()` (no `pollster::block_on`) |
| WebGPU + fallback | `wgpu = { features = ["webgpu", "webgl"] }` |
| Build tool | Trunk (`trunk serve`, `trunk build --release`) |
| Audio | cpal Web Audio API backend (stable Rust) |
| Gamepad | `gilrs` WASM backend or browser Gamepad API directly |
| File I/O | IndexedDB for project storage, fetch API for bundled assets |
| RUSTFLAGS | `--cfg=web_sys_unstable_apis` for WebGPU |
| Logging | `console_log` + `console_error_panic_hook` |

### Deployment
- GitHub Pages via `docs/` directory (same as v1)
- Asset manifest system for WASM (preload bundled assets)
- itch.io ZIP export via xtask

### Verification
- Web build loads and renders correctly
- Audio plays
- Gamepad input works
- Projects save/load via IndexedDB

---

## Implementation Order

| Phase | What | Depends On | Key Deliverable |
|---|---|---|---|
| **0** | Bootstrap window + rasterizer | Nothing | Window with rotating cube + egui overlay |
| **1** | Asset system | Phase 0 | UUID-based asset registry with persistence |
| **2** | Project system | Phase 1 | .b32 project files, new/open/save workflow |
| **3** | Editor shell + content browser | Phase 1-2 | Hazel-style panel layout with unified browser |
| **4** | Port editors | Phase 3 | World editor, modeler, tracker all functional |
| **5** | Lua scripting | Phase 4 | Scripts attached to entities, REPL console |
| **6** | WASM + polish | Phase 4 | Web deployment, feature parity |

---

## Reference: psx-studio Editor Pattern

The wgpu+egui initialization follows psx-studio's proven pattern:

```
App struct:
  window: Option<Arc<Window>>
  renderer: Option<Renderer>          // wgpu state
  egui_ctx: egui::Context
  egui_state: Option<egui_winit::State>
  egui_renderer: Option<egui_wgpu::Renderer>

ApplicationHandler::resumed():
  1. Create window
  2. Init wgpu (instance -> adapter -> device+queue -> surface)
  3. Create framebuffer texture (RGBA8, 320x240)
  4. Create fullscreen quad pipeline (vertex shader + sampler + texture bind group)
  5. Init egui_winit::State
  6. Init egui_wgpu::Renderer

Frame loop:
  1. Software rasterizer renders to Vec<u8>
  2. queue.write_texture() uploads pixels to GPU
  3. egui_ctx.run() collects UI
  4. Render pass 1: fullscreen quad with framebuffer texture (Clear)
  5. Render pass 2: egui primitives (Load — preserves pass 1)
  6. surface.present()
```

This is the exact same pattern bonnie-32 v2 will use. The software rasterizer output goes through wgpu exactly like psx-studio's emulated PS1 VRAM.
