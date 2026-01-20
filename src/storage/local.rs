//! Local filesystem storage backend
//!
//! Provides storage operations on the local filesystem.
//! All operations complete immediately (synchronous).

use super::{StorageError, StorageHandle};
use std::path::PathBuf;

/// Local filesystem storage backend
///
/// Wraps standard filesystem operations. All operations complete immediately,
/// so handles are always in Ready state.
#[derive(Debug, Clone)]
pub struct LocalStorage {
    /// Base directory for relative paths (usually current working directory)
    base_dir: PathBuf,
}

impl Default for LocalStorage {
    fn default() -> Self {
        Self::new()
    }
}

impl LocalStorage {
    /// Create a new local storage backend rooted at the current directory
    pub fn new() -> Self {
        Self {
            base_dir: PathBuf::from("."),
        }
    }

    /// Create a local storage backend with a custom base directory
    pub fn with_base_dir(base_dir: impl Into<PathBuf>) -> Self {
        Self {
            base_dir: base_dir.into(),
        }
    }

    /// Resolve a path relative to the base directory
    fn resolve(&self, path: &str) -> PathBuf {
        self.base_dir.join(path)
    }

    /// List files in a directory
    ///
    /// Returns a list of filenames (not full paths) in the directory.
    pub fn list(&self, path: &str) -> StorageHandle<Vec<String>> {
        let full_path = self.resolve(path);

        match std::fs::read_dir(&full_path) {
            Ok(entries) => {
                let files: Vec<String> = entries
                    .filter_map(|e| e.ok())
                    .filter(|e| e.path().is_file())
                    .filter_map(|e| e.file_name().into_string().ok())
                    .collect();
                StorageHandle::ready(files)
            }
            Err(e) => StorageHandle::error(StorageError::from(e)),
        }
    }

    /// Read a file
    ///
    /// Returns the file contents as bytes.
    pub fn read(&self, path: &str) -> StorageHandle<Vec<u8>> {
        let full_path = self.resolve(path);

        match std::fs::read(&full_path) {
            Ok(data) => StorageHandle::ready(data),
            Err(e) => StorageHandle::error(StorageError::from(e)),
        }
    }

    /// Write a file
    ///
    /// Creates or overwrites the file with the given data.
    pub fn write(&self, path: &str, data: &[u8]) -> StorageHandle<()> {
        let full_path = self.resolve(path);

        // Ensure parent directory exists
        if let Some(parent) = full_path.parent() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                return StorageHandle::error(StorageError::from(e));
            }
        }

        match std::fs::write(&full_path, data) {
            Ok(()) => StorageHandle::ready(()),
            Err(e) => StorageHandle::error(StorageError::from(e)),
        }
    }

    /// Delete a file
    pub fn delete(&self, path: &str) -> StorageHandle<()> {
        let full_path = self.resolve(path);

        match std::fs::remove_file(&full_path) {
            Ok(()) => StorageHandle::ready(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                // Not found is OK for delete
                StorageHandle::ready(())
            }
            Err(e) => StorageHandle::error(StorageError::from(e)),
        }
    }

    /// Check if a file exists
    pub fn exists(&self, path: &str) -> StorageHandle<bool> {
        let full_path = self.resolve(path);
        StorageHandle::ready(full_path.exists())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn setup_test_dir() -> (TempDir, LocalStorage) {
        let dir = TempDir::new().unwrap();
        let storage = LocalStorage::with_base_dir(dir.path());
        (dir, storage)
    }

    #[test]
    fn test_write_and_read() {
        let (_dir, storage) = setup_test_dir();

        // Write a file
        let data = b"hello world";
        let handle = storage.write("test.txt", data);
        assert!(handle.is_ready());
        assert!(handle.take().unwrap().is_ok());

        // Read it back
        let handle = storage.read("test.txt");
        assert!(handle.is_ready());
        let result = handle.take().unwrap().unwrap();
        assert_eq!(result, data);
    }

    #[test]
    fn test_read_not_found() {
        let (_dir, storage) = setup_test_dir();

        let handle = storage.read("nonexistent.txt");
        assert!(handle.is_ready());
        let result = handle.take().unwrap();
        assert!(matches!(result, Err(StorageError::NotFound(_))));
    }

    #[test]
    fn test_list() {
        let (dir, storage) = setup_test_dir();

        // Create some files
        std::fs::write(dir.path().join("a.txt"), "a").unwrap();
        std::fs::write(dir.path().join("b.txt"), "b").unwrap();
        std::fs::create_dir(dir.path().join("subdir")).unwrap();

        let handle = storage.list(".");
        assert!(handle.is_ready());
        let mut files = handle.take().unwrap().unwrap();
        files.sort();

        assert_eq!(files, vec!["a.txt", "b.txt"]);
    }

    #[test]
    fn test_delete() {
        let (dir, storage) = setup_test_dir();

        // Create a file
        std::fs::write(dir.path().join("delete_me.txt"), "x").unwrap();

        // Delete it
        let handle = storage.delete("delete_me.txt");
        assert!(handle.is_ready());
        assert!(handle.take().unwrap().is_ok());

        // Should be gone
        assert!(!dir.path().join("delete_me.txt").exists());

        // Deleting again should be OK
        let handle = storage.delete("delete_me.txt");
        assert!(handle.take().unwrap().is_ok());
    }

    #[test]
    fn test_exists() {
        let (dir, storage) = setup_test_dir();

        std::fs::write(dir.path().join("exists.txt"), "x").unwrap();

        let handle = storage.exists("exists.txt");
        assert_eq!(handle.take().unwrap().unwrap(), true);

        let handle = storage.exists("not_exists.txt");
        assert_eq!(handle.take().unwrap().unwrap(), false);
    }

    #[test]
    fn test_write_creates_parent_dirs() {
        let (_dir, storage) = setup_test_dir();

        let handle = storage.write("deep/nested/dir/file.txt", b"data");
        assert!(handle.is_ready());
        assert!(handle.take().unwrap().is_ok());

        let handle = storage.read("deep/nested/dir/file.txt");
        assert_eq!(handle.take().unwrap().unwrap(), b"data");
    }
}
