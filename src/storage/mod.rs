//! Storage Abstraction Layer
//!
//! Provides a unified interface for storage operations with path-based routing:
//! - `assets/userdata/*` → Cloud storage (when authenticated) or error (WASM) or local (native)
//! - Everything else → Local filesystem
//!
//! Uses a fire-and-poll async pattern that works with macroquad's single-threaded model.

pub mod gcp;
pub mod local;

use gcp::GcpStorage;
use local::LocalStorage;
use std::fmt;

/// Path prefix for user-created content that should sync to cloud
const USERDATA_PREFIX: &str = "assets/userdata/";

/// Storage operation status (fire-and-poll pattern)
///
/// All storage operations return immediately with a handle that can be polled
/// for completion. This allows the UI to remain responsive while operations
/// complete in the background.
#[derive(Debug, Clone)]
pub enum StorageStatus<T> {
    /// Operation is still in progress
    Pending,
    /// Operation completed successfully
    Ready(T),
    /// Operation failed
    Error(StorageError),
}

impl<T> StorageStatus<T> {
    /// Check if the operation is still pending
    pub fn is_pending(&self) -> bool {
        matches!(self, StorageStatus::Pending)
    }

    /// Check if the operation is ready (success or error)
    pub fn is_ready(&self) -> bool {
        !self.is_pending()
    }

    /// Take the result if ready, returning None if still pending
    pub fn take(self) -> Option<Result<T, StorageError>> {
        match self {
            StorageStatus::Pending => None,
            StorageStatus::Ready(v) => Some(Ok(v)),
            StorageStatus::Error(e) => Some(Err(e)),
        }
    }
}

/// Storage error types
#[derive(Debug, Clone, PartialEq)]
pub enum StorageError {
    /// File or directory not found
    NotFound(String),
    /// Permission denied
    PermissionDenied(String),
    /// I/O error
    IoError(String),
    /// Network error (cloud storage only)
    NetworkError(String),
    /// Authentication required
    AuthRequired,
    /// Quota exceeded
    QuotaExceeded { used: u64, limit: u64 },
    /// File too large
    FileTooLarge { size: u64, max: u64 },
    /// Rate limited
    RateLimited,
    /// Serialization/deserialization error
    SerdeError(String),
    /// Other error
    Other(String),
}

impl fmt::Display for StorageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StorageError::NotFound(path) => write!(f, "not found: {}", path),
            StorageError::PermissionDenied(msg) => write!(f, "permission denied: {}", msg),
            StorageError::IoError(msg) => write!(f, "I/O error: {}", msg),
            StorageError::NetworkError(msg) => write!(f, "network error: {}", msg),
            StorageError::AuthRequired => write!(f, "authentication required"),
            StorageError::QuotaExceeded { used, limit } => {
                write!(f, "quota exceeded: {} / {} bytes", used, limit)
            }
            StorageError::FileTooLarge { size, max } => {
                write!(f, "file too large: {} bytes (max: {})", size, max)
            }
            StorageError::RateLimited => write!(f, "rate limited, try again later"),
            StorageError::SerdeError(msg) => write!(f, "serialization error: {}", msg),
            StorageError::Other(msg) => write!(f, "{}", msg),
        }
    }
}

impl std::error::Error for StorageError {}

impl From<std::io::Error> for StorageError {
    fn from(e: std::io::Error) -> Self {
        match e.kind() {
            std::io::ErrorKind::NotFound => StorageError::NotFound(e.to_string()),
            std::io::ErrorKind::PermissionDenied => StorageError::PermissionDenied(e.to_string()),
            _ => StorageError::IoError(e.to_string()),
        }
    }
}

/// Storage mode indicator
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StorageMode {
    /// Local filesystem storage (native only)
    Local,
    /// GCP Cloud Storage (requires authentication)
    Cloud,
}

impl StorageMode {
    /// Human-readable label for the storage mode
    pub fn label(&self) -> &'static str {
        match self {
            StorageMode::Local => "Local",
            StorageMode::Cloud => "Cloud",
        }
    }
}

/// Handle for tracking an in-progress storage operation
///
/// The handle holds the result of a storage operation. For local storage,
/// the result is available immediately. For cloud storage, the operation
/// may take time and the handle should be polled.
#[derive(Debug)]
pub struct StorageHandle<T> {
    status: StorageStatus<T>,
}

impl<T> StorageHandle<T> {
    /// Create a handle that's immediately ready with a value
    pub fn ready(value: T) -> Self {
        Self {
            status: StorageStatus::Ready(value),
        }
    }

    /// Create a handle that's immediately ready with an error
    pub fn error(err: StorageError) -> Self {
        Self {
            status: StorageStatus::Error(err),
        }
    }

    /// Create a pending handle (for async operations)
    pub fn pending() -> Self {
        Self {
            status: StorageStatus::Pending,
        }
    }

    /// Check if the operation is still pending
    pub fn is_pending(&self) -> bool {
        self.status.is_pending()
    }

    /// Check if the operation is ready
    pub fn is_ready(&self) -> bool {
        self.status.is_ready()
    }

    /// Poll the operation status
    pub fn poll(&self) -> &StorageStatus<T> {
        &self.status
    }

    /// Take the result, consuming the handle
    ///
    /// Returns None if the operation is still pending.
    pub fn take(self) -> Option<Result<T, StorageError>> {
        self.status.take()
    }
}

impl<T: Clone> StorageHandle<T> {
    /// Get a clone of the result if ready
    pub fn try_get(&self) -> Option<Result<T, StorageError>> {
        match &self.status {
            StorageStatus::Pending => None,
            StorageStatus::Ready(v) => Some(Ok(v.clone())),
            StorageStatus::Error(e) => Some(Err(e.clone())),
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Storage - Unified storage with path-based routing
// ─────────────────────────────────────────────────────────────────────────────

/// Unified storage with path-based routing
///
/// Routes storage operations based on path prefix:
/// - `assets/userdata/*` → Cloud (when authenticated) or Local (native) or Error (WASM)
/// - Everything else → Always Local
#[derive(Debug)]
pub struct Storage {
    local: LocalStorage,
    cloud: Option<GcpStorage>,
}

impl Storage {
    /// Create a new storage context
    ///
    /// On native: Local storage is always available
    /// On WASM: Starts with no cloud (user must authenticate)
    pub fn new() -> Self {
        Self {
            local: LocalStorage::new(),
            cloud: None,
        }
    }

    /// Check if a path should be routed to cloud storage
    fn is_userdata_path(path: &str) -> bool {
        path.starts_with(USERDATA_PREFIX)
    }

    /// Get the storage mode for userdata paths
    pub fn mode(&self) -> StorageMode {
        if self.cloud.is_some() {
            StorageMode::Cloud
        } else {
            StorageMode::Local
        }
    }

    /// Check if cloud storage is available
    pub fn has_cloud(&self) -> bool {
        self.cloud.is_some()
    }

    /// Check if this storage supports writing userdata
    pub fn can_write(&self) -> bool {
        // Native: always can write (to local)
        // WASM + cloud: can write to cloud
        // WASM + no cloud: cannot write
        #[cfg(not(target_arch = "wasm32"))]
        {
            true
        }
        #[cfg(target_arch = "wasm32")]
        {
            self.cloud.is_some()
        }
    }

    /// Update storage based on authentication state
    ///
    /// Call this after authentication state changes.
    pub fn update_for_auth(&mut self) {
        use crate::auth;

        if auth::is_authenticated() {
            self.cloud = Some(GcpStorage::new());
            #[cfg(not(target_arch = "wasm32"))]
            println!("Storage: Cloud enabled for userdata");
        } else {
            self.cloud = None;
            #[cfg(not(target_arch = "wasm32"))]
            println!("Storage: Cloud disabled");
        }
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Path-based routing
    // ─────────────────────────────────────────────────────────────────────────

    /// List files in a directory
    pub fn list(&self, path: &str) -> StorageHandle<Vec<String>> {
        if Self::is_userdata_path(path) {
            if let Some(cloud) = &self.cloud {
                return cloud.list(path);
            }
            #[cfg(target_arch = "wasm32")]
            return StorageHandle::ready(Vec::new()); // No files when not authenticated
        }
        self.local.list(path)
    }

    /// Read a file
    pub fn read(&self, path: &str) -> StorageHandle<Vec<u8>> {
        if Self::is_userdata_path(path) {
            if let Some(cloud) = &self.cloud {
                return cloud.read(path);
            }
            #[cfg(target_arch = "wasm32")]
            return StorageHandle::error(StorageError::AuthRequired);
        }
        self.local.read(path)
    }

    /// Write a file
    pub fn write(&self, path: &str, data: &[u8]) -> StorageHandle<()> {
        if Self::is_userdata_path(path) {
            if let Some(cloud) = &self.cloud {
                return cloud.write(path, data);
            }
            #[cfg(target_arch = "wasm32")]
            return StorageHandle::error(StorageError::AuthRequired);
        }
        self.local.write(path, data)
    }

    /// Delete a file
    pub fn delete(&self, path: &str) -> StorageHandle<()> {
        if Self::is_userdata_path(path) {
            if let Some(cloud) = &self.cloud {
                return cloud.delete(path);
            }
            #[cfg(target_arch = "wasm32")]
            return StorageHandle::error(StorageError::AuthRequired);
        }
        self.local.delete(path)
    }

    /// Check if a file exists
    pub fn exists(&self, path: &str) -> StorageHandle<bool> {
        if Self::is_userdata_path(path) {
            if let Some(cloud) = &self.cloud {
                return cloud.exists(path);
            }
            #[cfg(target_arch = "wasm32")]
            return StorageHandle::ready(false); // Nothing exists when not authenticated
        }
        self.local.exists(path)
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Synchronous methods (for local storage or when blocking is acceptable)
    // These will panic if called on a cloud path that returns Pending
    // ─────────────────────────────────────────────────────────────────────────

    /// List files synchronously
    ///
    /// # Panics
    /// Panics if the operation returns a pending result.
    pub fn list_sync(&self, path: &str) -> Result<Vec<String>, StorageError> {
        self.list(path)
            .take()
            .expect("list_sync called on async backend")
    }

    /// Read a file synchronously
    ///
    /// # Panics
    /// Panics if the operation returns a pending result.
    pub fn read_sync(&self, path: &str) -> Result<Vec<u8>, StorageError> {
        self.read(path)
            .take()
            .expect("read_sync called on async backend")
    }

    /// Write a file synchronously
    ///
    /// # Panics
    /// Panics if the operation returns a pending result.
    pub fn write_sync(&self, path: &str, data: &[u8]) -> Result<(), StorageError> {
        self.write(path, data)
            .take()
            .expect("write_sync called on async backend")
    }

    /// Delete a file synchronously
    ///
    /// # Panics
    /// Panics if the operation returns a pending result.
    pub fn delete_sync(&self, path: &str) -> Result<(), StorageError> {
        self.delete(path)
            .take()
            .expect("delete_sync called on async backend")
    }

    /// Check if a file exists synchronously
    ///
    /// # Panics
    /// Panics if the operation returns a pending result.
    pub fn exists_sync(&self, path: &str) -> Result<bool, StorageError> {
        self.exists(path)
            .take()
            .expect("exists_sync called on async backend")
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Convenience methods
    // ─────────────────────────────────────────────────────────────────────────

    /// Read a file as a UTF-8 string synchronously
    pub fn read_string_sync(&self, path: &str) -> Result<String, StorageError> {
        let bytes = self.read_sync(path)?;
        String::from_utf8(bytes).map_err(|e| StorageError::Other(e.to_string()))
    }

    /// Write a string to a file synchronously
    pub fn write_string_sync(&self, path: &str, content: &str) -> Result<(), StorageError> {
        self.write_sync(path, content.as_bytes())
    }

    /// Check if operations complete synchronously for a given path
    ///
    /// Returns true for local paths, false for cloud paths when authenticated.
    pub fn is_sync(&self, path: &str) -> bool {
        if Self::is_userdata_path(path) && self.cloud.is_some() {
            false
        } else {
            true
        }
    }
}

impl Default for Storage {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_storage_status() {
        let pending: StorageStatus<i32> = StorageStatus::Pending;
        assert!(pending.is_pending());
        assert!(!pending.is_ready());

        let ready: StorageStatus<i32> = StorageStatus::Ready(42);
        assert!(!ready.is_pending());
        assert!(ready.is_ready());

        let error: StorageStatus<i32> =
            StorageStatus::Error(StorageError::NotFound("test".into()));
        assert!(!error.is_pending());
        assert!(error.is_ready());
    }

    #[test]
    fn test_storage_handle() {
        let handle = StorageHandle::ready(42);
        assert!(handle.is_ready());
        assert_eq!(handle.take(), Some(Ok(42)));

        let handle = StorageHandle::<i32>::error(StorageError::AuthRequired);
        assert!(handle.is_ready());
        assert!(matches!(
            handle.take(),
            Some(Err(StorageError::AuthRequired))
        ));

        let handle = StorageHandle::<i32>::pending();
        assert!(handle.is_pending());
        assert_eq!(handle.take(), None);
    }

    #[test]
    fn test_is_userdata_path() {
        assert!(Storage::is_userdata_path("assets/userdata/levels/test.ron"));
        assert!(Storage::is_userdata_path("assets/userdata/assets/model.ron"));
        assert!(!Storage::is_userdata_path("assets/samples/levels/test.ron"));
        assert!(!Storage::is_userdata_path("assets/runtime/fonts/test.ttf"));
        assert!(!Storage::is_userdata_path("random/path.txt"));
    }

    #[test]
    fn test_storage_mode() {
        let storage = Storage::new();
        // Without cloud, mode is Local
        assert_eq!(storage.mode(), StorageMode::Local);
        assert!(!storage.has_cloud());
    }
}
