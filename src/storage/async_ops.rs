//! Async Operations
//!
//! Provides non-blocking storage operations using background threads.
//! Operations can be polled each frame to check for completion.

use super::{Storage, StorageError};
use std::path::PathBuf;

#[cfg(not(target_arch = "wasm32"))]
use std::sync::mpsc::{channel, Receiver, TryRecvError};
#[cfg(not(target_arch = "wasm32"))]
use std::thread;

/// Result type for async operations
pub type AsyncResult<T> = Result<T, StorageError>;

/// A handle to a pending async operation that can be polled
#[cfg(not(target_arch = "wasm32"))]
pub struct AsyncOp<T> {
    receiver: Receiver<AsyncResult<T>>,
    result: Option<AsyncResult<T>>,
}

#[cfg(not(target_arch = "wasm32"))]
impl<T> AsyncOp<T> {
    /// Create from a receiver
    fn from_receiver(receiver: Receiver<AsyncResult<T>>) -> Self {
        Self {
            receiver,
            result: None,
        }
    }

    /// Check if the operation has completed (polls the channel)
    pub fn is_complete(&mut self) -> bool {
        if self.result.is_some() {
            return true;
        }

        match self.receiver.try_recv() {
            Ok(result) => {
                self.result = Some(result);
                true
            }
            Err(TryRecvError::Empty) => false,
            Err(TryRecvError::Disconnected) => {
                // Thread panicked or dropped sender
                self.result = Some(Err(StorageError::Other("Operation failed".into())));
                true
            }
        }
    }

    /// Take the result if complete
    pub fn take(mut self) -> Option<AsyncResult<T>> {
        if self.result.is_none() {
            // Try one more time
            if let Ok(result) = self.receiver.try_recv() {
                self.result = Some(result);
            }
        }
        self.result
    }

    /// Get a reference to the result if complete
    pub fn result(&self) -> Option<&AsyncResult<T>> {
        self.result.as_ref()
    }
}

/// Pending save operation
#[cfg(not(target_arch = "wasm32"))]
pub struct PendingSave {
    pub op: AsyncOp<()>,
    pub path: PathBuf,
}

/// Pending load operation
#[cfg(not(target_arch = "wasm32"))]
pub struct PendingLoad {
    pub op: AsyncOp<Vec<u8>>,
    pub path: PathBuf,
}

/// Pending list operation
#[cfg(not(target_arch = "wasm32"))]
pub struct PendingList {
    pub op: AsyncOp<Vec<String>>,
    pub path: String,
}

/// Start an async save operation
#[cfg(not(target_arch = "wasm32"))]
pub fn save_async(path: PathBuf, data: Vec<u8>) -> PendingSave {
    let (sender, receiver) = channel();
    let path_str = path.to_string_lossy().to_string();

    thread::spawn(move || {
        // Re-create storage in this thread (it doesn't hold any state that matters)
        let mut storage = Storage::new();
        storage.update_for_auth();

        let result = storage.write_sync(&path_str, &data);
        let _ = sender.send(result);
    });

    PendingSave {
        op: AsyncOp::from_receiver(receiver),
        path,
    }
}

/// Start an async load operation
#[cfg(not(target_arch = "wasm32"))]
pub fn load_async(path: PathBuf) -> PendingLoad {
    let (sender, receiver) = channel();
    let path_str = path.to_string_lossy().to_string();

    thread::spawn(move || {
        let mut storage = Storage::new();
        storage.update_for_auth();

        let result = storage.read_sync(&path_str);
        let _ = sender.send(result);
    });

    PendingLoad {
        op: AsyncOp::from_receiver(receiver),
        path,
    }
}

/// Start an async list operation
#[cfg(not(target_arch = "wasm32"))]
pub fn list_async(path: String) -> PendingList {
    let (sender, receiver) = channel();
    let path_clone = path.clone();

    thread::spawn(move || {
        let mut storage = Storage::new();
        storage.update_for_auth();

        let result = storage.list_sync(&path_clone);
        let _ = sender.send(result);
    });

    PendingList {
        op: AsyncOp::from_receiver(receiver),
        path,
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// WASM stubs (operations complete "immediately" since they're already async in JS)
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(target_arch = "wasm32")]
pub struct AsyncOp<T> {
    result: Option<AsyncResult<T>>,
}

#[cfg(target_arch = "wasm32")]
impl<T> AsyncOp<T> {
    pub fn is_complete(&mut self) -> bool {
        true
    }

    pub fn take(self) -> Option<AsyncResult<T>> {
        self.result
    }

    pub fn result(&self) -> Option<&AsyncResult<T>> {
        self.result.as_ref()
    }
}

#[cfg(target_arch = "wasm32")]
pub struct PendingSave {
    pub op: AsyncOp<()>,
    pub path: PathBuf,
}

#[cfg(target_arch = "wasm32")]
pub struct PendingLoad {
    pub op: AsyncOp<Vec<u8>>,
    pub path: PathBuf,
}

#[cfg(target_arch = "wasm32")]
pub struct PendingList {
    pub op: AsyncOp<Vec<String>>,
    pub path: String,
}

#[cfg(target_arch = "wasm32")]
pub fn save_async(path: PathBuf, data: Vec<u8>) -> PendingSave {
    // On WASM, do synchronous (which actually goes through async JS)
    let mut storage = Storage::new();
    storage.update_for_auth();
    let path_str = path.to_string_lossy().to_string();
    let result = storage.write_sync(&path_str, &data);
    PendingSave {
        op: AsyncOp { result: Some(result) },
        path,
    }
}

#[cfg(target_arch = "wasm32")]
pub fn load_async(path: PathBuf) -> PendingLoad {
    let mut storage = Storage::new();
    storage.update_for_auth();
    let path_str = path.to_string_lossy().to_string();
    let result = storage.read_sync(&path_str);
    PendingLoad {
        op: AsyncOp { result: Some(result) },
        path,
    }
}

#[cfg(target_arch = "wasm32")]
pub fn list_async(path: String) -> PendingList {
    let mut storage = Storage::new();
    storage.update_for_auth();
    let result = storage.list_sync(&path);
    PendingList {
        op: AsyncOp { result: Some(result) },
        path,
    }
}
