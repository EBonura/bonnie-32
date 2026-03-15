use bonnie_32::asset::{AssetSource, AssetType};
use bonnie_32::project::Project;
use bonnie_32::project::manifest::ProjectManifest;
use std::path::PathBuf;
use tempfile::TempDir;

#[test]
fn manifest_create_and_save() {
    let dir = TempDir::new().unwrap();
    let manifest = ProjectManifest::new("Test Game");

    assert_eq!(manifest.name, "Test Game");
    assert_eq!(manifest.resolution, (320, 240));
    assert!(manifest.start_level.is_none());

    let path = dir.path().join("test.b32");
    manifest.save(&path).unwrap();
    assert!(path.exists());
}

#[test]
fn manifest_save_load_roundtrip() {
    let dir = TempDir::new().unwrap();
    let mut manifest = ProjectManifest::new("My Project");
    manifest.author = "ebonura".into();
    manifest.resolution = (640, 480);

    let path = dir.path().join("my_project.b32");
    manifest.save(&path).unwrap();

    let loaded = ProjectManifest::load(&path).unwrap();
    assert_eq!(loaded.name, "My Project");
    assert_eq!(loaded.author, "ebonura");
    assert_eq!(loaded.resolution, (640, 480));
}

#[test]
fn manifest_is_manifest_file() {
    assert!(ProjectManifest::is_manifest_file(&PathBuf::from("game.b32")));
    assert!(!ProjectManifest::is_manifest_file(&PathBuf::from("game.ron")));
    assert!(!ProjectManifest::is_manifest_file(&PathBuf::from("game")));
}

#[test]
fn project_create_directory_structure() {
    let dir = TempDir::new().unwrap();
    let project_root = dir.path().join("my_game");

    let project = Project::create(project_root.clone(), "My Game").unwrap();

    // Manifest file exists
    assert!(project.manifest_path().exists());
    assert_eq!(project.name(), "My Game");

    // Subdirectories created
    assert!(project_root.join("levels").is_dir());
    assert!(project_root.join("assets").is_dir());
    assert!(project_root.join("textures").is_dir());
    assert!(project_root.join("songs").is_dir());
    assert!(project_root.join("scripts").is_dir());
    assert!(project_root.join("meshes").is_dir());

    // Registry file created
    assert!(project_root.join("asset_registry.ron").exists());
}

#[test]
fn project_create_and_reopen() {
    let dir = TempDir::new().unwrap();
    let project_root = dir.path().join("test_project");

    let manifest_path;
    {
        let project = Project::create(project_root.clone(), "Test").unwrap();
        manifest_path = project.manifest_path().to_path_buf();
        project.save().unwrap();
    }

    // Reopen from manifest
    let project = Project::open(manifest_path).unwrap();
    assert_eq!(project.name(), "Test");
    assert_eq!(project.root(), project_root);
}

#[test]
fn project_scan_finds_new_assets() {
    let dir = TempDir::new().unwrap();
    let project_root = dir.path().join("scan_test");

    let mut project = Project::create(project_root.clone(), "Scan Test").unwrap();

    // Drop some files into the project directories
    std::fs::write(project_root.join("levels/dungeon.ron"), "Level()").unwrap();
    std::fs::write(project_root.join("levels/cave.ron"), "Level()").unwrap();
    std::fs::write(project_root.join("songs/theme.ron"), "Song()").unwrap();
    std::fs::write(project_root.join("scripts/enemy.lua"), "function on_update() end").unwrap();

    let found = project.scan_assets();
    assert_eq!(found.len(), 4);

    // Verify types
    let levels: Vec<_> = project
        .assets
        .registry
        .iter_by_type(AssetType::Level)
        .collect();
    assert_eq!(levels.len(), 2);

    let songs: Vec<_> = project
        .assets
        .registry
        .iter_by_type(AssetType::Song)
        .collect();
    assert_eq!(songs.len(), 1);

    let scripts: Vec<_> = project
        .assets
        .registry
        .iter_by_type(AssetType::Script)
        .collect();
    assert_eq!(scripts.len(), 1);
}

#[test]
fn project_scan_is_idempotent() {
    let dir = TempDir::new().unwrap();
    let project_root = dir.path().join("idempotent_test");

    let mut project = Project::create(project_root.clone(), "Idempotent").unwrap();
    std::fs::write(project_root.join("levels/test.ron"), "Level()").unwrap();

    let first = project.scan_assets();
    assert_eq!(first.len(), 1);

    // Second scan finds nothing new
    let second = project.scan_assets();
    assert_eq!(second.len(), 0);

    assert_eq!(project.assets.registry.len(), 1);
}

#[test]
fn project_register_bundled_assets() {
    let dir = TempDir::new().unwrap();
    let project_root = dir.path().join("bundled_test");
    let bundled_root = dir.path().join("bundled");

    // Set up bundled sample assets
    std::fs::create_dir_all(bundled_root.join("levels")).unwrap();
    std::fs::create_dir_all(bundled_root.join("songs")).unwrap();
    std::fs::write(bundled_root.join("levels/Cathedral.ron"), "Level()").unwrap();
    std::fs::write(bundled_root.join("levels/Dungeon.ron"), "Level()").unwrap();
    std::fs::write(bundled_root.join("songs/song_001.ron"), "Song()").unwrap();

    let mut project = Project::create(project_root, "Bundled Test").unwrap();
    let found = project.register_bundled_assets(&bundled_root);
    assert_eq!(found.len(), 3);

    // Verify they're marked as bundled
    let bundled: Vec<_> = project
        .assets
        .registry
        .iter_by_source(AssetSource::Bundled)
        .collect();
    assert_eq!(bundled.len(), 3);
}

#[test]
fn project_save_persists_scanned_assets() {
    let dir = TempDir::new().unwrap();
    let project_root = dir.path().join("persist_test");

    let manifest_path;
    {
        let mut project = Project::create(project_root.clone(), "Persist").unwrap();
        manifest_path = project.manifest_path().to_path_buf();

        std::fs::write(project_root.join("levels/test.ron"), "Level()").unwrap();
        project.scan_assets();
        project.save().unwrap();
    }

    // Reopen and verify the scanned asset persisted
    let project = Project::open(manifest_path).unwrap();
    let levels: Vec<_> = project
        .assets
        .registry
        .iter_by_type(AssetType::Level)
        .collect();
    assert_eq!(levels.len(), 1);
}

#[test]
fn project_resolve_path() {
    let dir = TempDir::new().unwrap();
    let project_root = dir.path().join("resolve_test");

    let project = Project::create(project_root.clone(), "Resolve").unwrap();
    let resolved = project.resolve(&PathBuf::from("levels/test.ron"));
    assert_eq!(resolved, project_root.join("levels/test.ron"));
}
