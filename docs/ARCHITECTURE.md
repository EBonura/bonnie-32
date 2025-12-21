# Bonnie Engine Architecture

## Shared State Model

The engine uses a **shared state architecture** where all game assets live in a central `ProjectData` struct. This enables **live editing**: changes made in any editor are immediately visible in all other views, including the game preview.

### Before (Per-Tool Ownership)

```
AppState
├── world_editor: WorldEditorState
│   └── editor_state: EditorState
│       └── level: Level              ← OWNS data (copy)
├── modeler: ModelerToolState
│   └── modeler_state: ModelerState
│       └── spine_model/mesh/etc      ← OWNS data (copy)
├── tracker: TrackerState
│   └── song: Song                    ← OWNS data (copy)
│   └── audio: AudioEngine
└── (no game tool)
```

**Problems:**
- Each tool owns its own copy of the data
- Changes in one tool require manual sync to others
- No way to see editor changes in game preview without explicit "play" action
- Duplication of data in memory

### After (Shared State)

```
AppState
├── project: ProjectData              ← SINGLE SOURCE OF TRUTH
│   ├── level: Level                  ← One level, shared by all
│   ├── models: Vec<RiggedModel>
│   ├── meshes: Vec<EditableMesh>
│   ├── spines: Vec<SpineModel>
│   └── songs: Vec<Song>
│
├── audio: AudioEngine                ← Shared audio system
│
├── world_editor: EditorUiState       ← UI state only (camera, selection, etc.)
├── game: GameToolState               ← ECS world + camera (references project.level)
├── modeler: ModelerUiState           ← UI state only
└── tracker: TrackerUiState           ← UI state only
```

**Benefits:**
- Single source of truth for all data
- Live editing: modify in World Editor, see immediately in Game tab
- No sync needed between tools
- Reduced memory usage

## Key Structs

### ProjectData (src/project.rs)

Central container for all project assets:

```rust
pub struct ProjectData {
    pub level: Level,              // World geometry (rooms, sectors, portals)
    pub models: Vec<RiggedModel>,  // Character/object models with skeleton
    pub meshes: Vec<EditableMesh>, // Standalone meshes (not rigged)
    pub spines: Vec<SpineModel>,   // Spine-based procedural meshes
    pub songs: Vec<Song>,          // Music tracks
}
```

### EditorUiState (refactored from EditorState)

UI-only state for the World Editor. Does NOT own the level:

```rust
pub struct EditorUiState {
    // File management
    pub current_file: Option<PathBuf>,
    pub dirty: bool,

    // Tool state
    pub tool: EditorTool,
    pub selection: Selection,

    // Camera
    pub camera_3d: Camera,
    pub camera_mode: CameraMode,
    pub orbit_target: Vec3,
    // ... other camera state

    // Undo/redo (stores level snapshots)
    pub undo_stack: Vec<Level>,
    pub redo_stack: Vec<Level>,

    // UI state (texture palette, properties scroll, etc.)
    // ...
}
```

### GameToolState (src/game/runtime.rs)

Game preview state with ECS world for dynamic entities:

```rust
pub struct GameToolState {
    pub world: game::World,        // ECS entities (player, enemies, items)
    pub events: game::Events,      // Event queues
    pub camera: Camera,            // Game camera (separate from editor)
    pub playing: bool,             // Play/pause state
    pub player_entity: Option<Entity>,
    // ...
}
```

## Data Flow

### Editing Flow

```
User edits wall in World Editor
        │
        ▼
project.level is modified directly
        │
        ▼
All views see the change immediately:
├── World Editor 3D viewport (re-renders)
├── Game tab (re-renders same level)
└── Any other tool that reads project.level
```

### Game Simulation Flow

```
project.level                 game.world (ECS)
     │                              │
     │  Static geometry             │  Dynamic entities
     │  (rooms, walls, floors)      │  (player, enemies, items)
     │                              │
     └──────────┬───────────────────┘
                │
                ▼
         Game Renderer
         (combines static + dynamic)
```

The level provides static geometry (collision, rendering). The ECS world manages dynamic entities that move, take damage, etc. Both are rendered together.

## Undo/Redo Strategy

Since `project.level` is shared, undo/redo needs careful handling:

1. **Undo stack stores Level snapshots** - When user makes an edit, we clone the current `project.level` to the undo stack
2. **Undo replaces project.level** - Restoring from undo stack replaces the shared level
3. **All views update automatically** - Since they all reference the same level

```rust
// In EditorUiState
pub fn save_undo(&mut self, project: &mut ProjectData) {
    self.undo_stack.push(project.level.clone());
    self.redo_stack.clear();
    self.dirty = true;
}

pub fn undo(&mut self, project: &mut ProjectData) {
    if let Some(prev) = self.undo_stack.pop() {
        self.redo_stack.push(project.level.clone());
        project.level = prev;
    }
}
```

## Function Signature Changes

Functions that previously took `&mut EditorState` now take separate parameters:

```rust
// Before
pub fn draw_editor(
    state: &mut EditorState,
    layout: &mut EditorLayout,
    ...
) -> EditorAction

// After
pub fn draw_editor(
    project: &mut ProjectData,
    ui: &mut EditorUiState,
    layout: &mut EditorLayout,
    ...
) -> EditorAction
```

This makes the data flow explicit: `project` is the shared data, `ui` is the tool-specific UI state.

## Migration Path

The refactor is done in phases to avoid breaking everything at once:

### Phase 1: Add ProjectData (non-breaking) ✓
- Create `src/project.rs` with `ProjectData` struct
- Add `project` field to `AppState`
- Nothing uses it yet - existing code unchanged

### Phase 2a: Sync-Based Live Editing (Pragmatic) ✓
- Keep `level` in `EditorState` for now (avoid 100+ line changes)
- Sync `editor_state.level` → `project.level` on every frame
- Game tool reads from `project.level`
- **Result: Live editing works with minimal code changes**

### Phase 2b: Full Migration (Future)
- Remove `level` from `EditorState`
- Update all editor functions to take `&mut ProjectData`
- Update main.rs to pass project reference
- *This is optional cleanup - sync approach works fine*

### Phase 3: Add Game Tool ✓
- Create `GameToolState` with ECS world
- Add Game tab to UI
- Render level from `project.level`
- Test live editing works

### Phase 4: Migrate Models (Future)
- Move model data from `ModelerState` to `ProjectData`
- Update modeler functions

### Phase 5: Migrate Audio (Future)
- Move `song` from `TrackerState` to `ProjectData`
- Move `AudioEngine` to `AppState` (shared)

## Sync-Based Approach (Current Implementation)

For pragmatic reasons, we use a sync-based approach rather than full refactoring:

```rust
// In main.rs, every frame:
app.project.level = app.world_editor.editor_state.level.clone();
```

This is slightly less efficient (clone each frame) but:
- Requires minimal code changes
- Provides live editing behavior
- Can be optimized later with dirty flags
- Full refactor can be done incrementally

## Live Editing in Action

After the refactor, live editing works automatically:

1. User is in World Editor, editing a room
2. User changes wall height
3. `project.level` is modified
4. User switches to Game tab (or has split view)
5. Game renders the same `project.level`
6. **Wall height change is immediately visible in game**

No explicit "sync" or "apply" button needed. The game always shows the current state of the level.
