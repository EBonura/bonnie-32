use bonnie_32::asset::{AssetHandle, AssetManager, AssetRegistry, AssetSource, AssetType};
use std::path::PathBuf;
use tempfile::TempDir;

#[test]
fn handle_null_and_new() {
    let null = AssetHandle::null();
    assert!(null.is_null());

    let handle = AssetHandle::new();
    assert!(!handle.is_null());

    let default = AssetHandle::default();
    assert!(default.is_null());
}

#[test]
fn handle_equality() {
    let a = AssetHandle::new();
    let b = a;
    assert_eq!(a, b);

    let c = AssetHandle::new();
    assert_ne!(a, c);
}

#[test]
fn registry_register_and_get() {
    let mut registry = AssetRegistry::new();

    let handle = registry.register(
        PathBuf::from("levels/test.ron"),
        AssetType::Level,
        AssetSource::Project,
    );

    assert!(!handle.is_null());
    assert_eq!(registry.len(), 1);

    let entry = registry.get(&handle).unwrap();
    assert_eq!(entry.path, PathBuf::from("levels/test.ron"));
    assert_eq!(entry.asset_type, AssetType::Level);
    assert_eq!(entry.source, AssetSource::Project);
    assert_eq!(entry.name, "test");
}

#[test]
fn registry_deduplicates_paths() {
    let mut registry = AssetRegistry::new();

    let h1 = registry.register(
        PathBuf::from("levels/test.ron"),
        AssetType::Level,
        AssetSource::Project,
    );
    let h2 = registry.register(
        PathBuf::from("levels/test.ron"),
        AssetType::Level,
        AssetSource::Project,
    );

    assert_eq!(h1, h2);
    assert_eq!(registry.len(), 1);
}

#[test]
fn registry_rename_preserves_handle() {
    let mut registry = AssetRegistry::new();

    let handle = registry.register(
        PathBuf::from("levels/old_name.ron"),
        AssetType::Level,
        AssetSource::Project,
    );

    assert!(registry.rename(&handle, PathBuf::from("levels/new_name.ron")));

    let entry = registry.get(&handle).unwrap();
    assert_eq!(entry.path, PathBuf::from("levels/new_name.ron"));
    assert_eq!(entry.name, "new_name");
}

#[test]
fn registry_find_by_path() {
    let mut registry = AssetRegistry::new();

    let handle = registry.register(
        PathBuf::from("songs/track1.ron"),
        AssetType::Song,
        AssetSource::Bundled,
    );

    let found = registry.find_by_path(&PathBuf::from("songs/track1.ron"));
    assert_eq!(found, Some(handle));

    let not_found = registry.find_by_path(&PathBuf::from("songs/nonexistent.ron"));
    assert!(not_found.is_none());
}

#[test]
fn registry_remove() {
    let mut registry = AssetRegistry::new();

    let handle = registry.register(
        PathBuf::from("assets/model.ron"),
        AssetType::Model,
        AssetSource::Project,
    );

    assert_eq!(registry.len(), 1);
    registry.remove(&handle);
    assert_eq!(registry.len(), 0);
    assert!(registry.get(&handle).is_none());
}

#[test]
fn registry_iter_by_type() {
    let mut registry = AssetRegistry::new();

    registry.register(PathBuf::from("levels/a.ron"), AssetType::Level, AssetSource::Project);
    registry.register(PathBuf::from("levels/b.ron"), AssetType::Level, AssetSource::Project);
    registry.register(PathBuf::from("songs/c.ron"), AssetType::Song, AssetSource::Project);

    let levels: Vec<_> = registry.iter_by_type(AssetType::Level).collect();
    assert_eq!(levels.len(), 2);

    let songs: Vec<_> = registry.iter_by_type(AssetType::Song).collect();
    assert_eq!(songs.len(), 1);
}

#[test]
fn registry_save_load_roundtrip() {
    let dir = TempDir::new().unwrap();

    let handle;
    {
        let mut registry = AssetRegistry::new();
        handle = registry.register(
            PathBuf::from("levels/dungeon.ron"),
            AssetType::Level,
            AssetSource::Project,
        );
        registry.register(
            PathBuf::from("songs/theme.ron"),
            AssetType::Song,
            AssetSource::Bundled,
        );
        registry.save(dir.path()).unwrap();
    }

    let loaded = AssetRegistry::load(dir.path()).unwrap();
    assert_eq!(loaded.len(), 2);

    let entry = loaded.get(&handle).unwrap();
    assert_eq!(entry.path, PathBuf::from("levels/dungeon.ron"));
    assert_eq!(entry.asset_type, AssetType::Level);
    assert_eq!(entry.name, "dungeon");
}

#[test]
fn manager_register_and_resolve() {
    let dir = TempDir::new().unwrap();
    let mut manager = AssetManager::with_project(dir.path().to_path_buf()).unwrap();

    let handle = manager.register(
        PathBuf::from("levels/test.ron"),
        AssetType::Level,
        AssetSource::Project,
    );

    let resolved = manager.resolve_path(&handle).unwrap();
    assert_eq!(resolved, dir.path().join("levels/test.ron"));
}

#[test]
fn manager_import_copies_file() {
    let project_dir = TempDir::new().unwrap();
    let source_dir = TempDir::new().unwrap();

    // Create source file
    let source_file = source_dir.path().join("enemy.lua");
    std::fs::write(&source_file, "function on_update(dt) end").unwrap();

    let mut manager = AssetManager::with_project(project_dir.path().to_path_buf()).unwrap();
    let handle = manager.import(&source_file, AssetType::Script).unwrap();

    // File should be copied into project
    let resolved = manager.resolve_path(&handle).unwrap();
    assert!(resolved.exists());
    assert_eq!(
        std::fs::read_to_string(&resolved).unwrap(),
        "function on_update(dt) end"
    );

    // Should be in the scripts directory
    let entry = manager.registry.get(&handle).unwrap();
    assert_eq!(entry.path, PathBuf::from("scripts/enemy.lua"));
}

#[test]
fn manager_load_raw() {
    let dir = TempDir::new().unwrap();
    std::fs::create_dir_all(dir.path().join("levels")).unwrap();
    std::fs::write(dir.path().join("levels/test.ron"), "Level()").unwrap();

    let mut manager = AssetManager::with_project(dir.path().to_path_buf()).unwrap();
    let handle = manager.register(
        PathBuf::from("levels/test.ron"),
        AssetType::Level,
        AssetSource::Project,
    );

    let data = manager.load_raw(&handle).unwrap();
    assert_eq!(data, b"Level()");

    // Second load should come from cache
    let data2 = manager.load_raw(&handle).unwrap();
    assert_eq!(data2, b"Level()");
}

#[test]
fn manager_evict_clears_cache() {
    let dir = TempDir::new().unwrap();
    std::fs::create_dir_all(dir.path().join("levels")).unwrap();
    std::fs::write(dir.path().join("levels/test.ron"), "Level()").unwrap();

    let mut manager = AssetManager::with_project(dir.path().to_path_buf()).unwrap();
    let handle = manager.register(
        PathBuf::from("levels/test.ron"),
        AssetType::Level,
        AssetSource::Project,
    );

    // Load to populate cache
    manager.load_raw(&handle);

    // Modify file on disk
    std::fs::write(dir.path().join("levels/test.ron"), "Level(v2)").unwrap();

    // Cache still returns old data
    let old = manager.load_raw(&handle).unwrap();
    assert_eq!(old, b"Level()");

    // After evict, next load gets new data
    manager.evict(&handle);
    let new = manager.load_raw(&handle).unwrap();
    assert_eq!(new, b"Level(v2)");
}

#[test]
fn manager_save_and_reload_registry() {
    let dir = TempDir::new().unwrap();

    let handle;
    {
        let mut manager = AssetManager::with_project(dir.path().to_path_buf()).unwrap();
        handle = manager.register(
            PathBuf::from("assets/warrior.ron"),
            AssetType::Model,
            AssetSource::Project,
        );
        manager.save_registry().unwrap();
    }

    // Reload from disk
    let manager = AssetManager::with_project(dir.path().to_path_buf()).unwrap();
    let entry = manager.registry.get(&handle).unwrap();
    assert_eq!(entry.name, "warrior");
    assert_eq!(entry.asset_type, AssetType::Model);
}
