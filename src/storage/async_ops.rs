//! Async Operations
//!
//! Provides non-blocking storage operations using background threads.
//! Operations can be polled each frame to check for completion.

use super::StorageError;
use std::path::PathBuf;

#[cfg(not(target_arch = "wasm32"))]
use super::Storage;
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
// WASM async operations (truly non-blocking via JS operation IDs)
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(target_arch = "wasm32")]
extern "C" {
    fn b32_gcp_storage_write(
        path_ptr: *const u8,
        path_len: usize,
        data_ptr: *const u8,
        data_len: usize,
    ) -> i32;
    fn b32_gcp_storage_read(path_ptr: *const u8, path_len: usize) -> i32;
    fn b32_gcp_storage_list(path_ptr: *const u8, path_len: usize) -> i32;
    fn b32_gcp_storage_poll(op_id: i32) -> i32;
    fn b32_gcp_storage_get_result_len(op_id: i32) -> usize;
    fn b32_gcp_storage_copy_result(op_id: i32, dest_ptr: *mut u8, max_len: usize) -> usize;
    fn b32_gcp_storage_get_error_len(op_id: i32) -> usize;
    fn b32_gcp_storage_copy_error(op_id: i32, dest_ptr: *mut u8, max_len: usize) -> usize;
    fn b32_gcp_storage_free(op_id: i32);
}

/// Poll status codes from JavaScript
#[cfg(target_arch = "wasm32")]
const POLL_PENDING: i32 = 0;
#[cfg(target_arch = "wasm32")]
const POLL_READY: i32 = 1;
#[cfg(target_arch = "wasm32")]
const POLL_ERROR: i32 = 2;

/// Async operation holding a JS operation ID
#[cfg(target_arch = "wasm32")]
pub struct AsyncOp<T> {
    op_id: Option<i32>,
    result: Option<AsyncResult<T>>,
    _marker: std::marker::PhantomData<T>,
}

#[cfg(target_arch = "wasm32")]
impl<T> AsyncOp<T> {
    fn new(op_id: i32) -> Self {
        Self {
            op_id: Some(op_id),
            result: None,
            _marker: std::marker::PhantomData,
        }
    }

    fn with_result(result: AsyncResult<T>) -> Self {
        Self {
            op_id: None,
            result: Some(result),
            _marker: std::marker::PhantomData,
        }
    }

    pub fn result(&self) -> Option<&AsyncResult<T>> {
        self.result.as_ref()
    }
}

#[cfg(target_arch = "wasm32")]
impl AsyncOp<()> {
    /// Check if the operation has completed (single poll, no busy-wait)
    pub fn is_complete(&mut self) -> bool {
        if self.result.is_some() {
            return true;
        }

        let op_id = match self.op_id {
            Some(id) => id,
            None => return true,
        };

        let status = unsafe { b32_gcp_storage_poll(op_id) };

        match status {
            POLL_PENDING => false,
            POLL_READY => {
                self.result = Some(Ok(()));
                true
            }
            POLL_ERROR => {
                let error = get_js_error(op_id);
                self.result = Some(Err(error));
                true
            }
            _ => {
                self.result = Some(Err(StorageError::Other(format!("Unknown poll status: {}", status))));
                true
            }
        }
    }

    pub fn take(mut self) -> Option<AsyncResult<()>> {
        // Free the JS operation if we still have it
        if let Some(op_id) = self.op_id.take() {
            unsafe { b32_gcp_storage_free(op_id) };
        }
        self.result
    }
}

#[cfg(target_arch = "wasm32")]
impl AsyncOp<Vec<u8>> {
    pub fn is_complete(&mut self) -> bool {
        if self.result.is_some() {
            return true;
        }

        let op_id = match self.op_id {
            Some(id) => id,
            None => return true,
        };

        let status = unsafe { b32_gcp_storage_poll(op_id) };

        match status {
            POLL_PENDING => false,
            POLL_READY => {
                let data = get_js_result_bytes(op_id);
                self.result = Some(Ok(data));
                true
            }
            POLL_ERROR => {
                let error = get_js_error(op_id);
                self.result = Some(Err(error));
                true
            }
            _ => {
                self.result = Some(Err(StorageError::Other(format!("Unknown poll status: {}", status))));
                true
            }
        }
    }

    pub fn take(mut self) -> Option<AsyncResult<Vec<u8>>> {
        if let Some(op_id) = self.op_id.take() {
            unsafe { b32_gcp_storage_free(op_id) };
        }
        self.result
    }
}

#[cfg(target_arch = "wasm32")]
impl AsyncOp<Vec<String>> {
    pub fn is_complete(&mut self) -> bool {
        if self.result.is_some() {
            return true;
        }

        let op_id = match self.op_id {
            Some(id) => id,
            None => return true,
        };

        let status = unsafe { b32_gcp_storage_poll(op_id) };

        match status {
            POLL_PENDING => false,
            POLL_READY => {
                let data = get_js_result_string(op_id);
                let files: Vec<String> = if data.is_empty() {
                    Vec::new()
                } else {
                    data.lines().map(|s| s.to_string()).collect()
                };
                self.result = Some(Ok(files));
                true
            }
            POLL_ERROR => {
                let error = get_js_error(op_id);
                self.result = Some(Err(error));
                true
            }
            _ => {
                self.result = Some(Err(StorageError::Other(format!("Unknown poll status: {}", status))));
                true
            }
        }
    }

    pub fn take(mut self) -> Option<AsyncResult<Vec<String>>> {
        if let Some(op_id) = self.op_id.take() {
            unsafe { b32_gcp_storage_free(op_id) };
        }
        self.result
    }
}

/// Get error message from JavaScript
#[cfg(target_arch = "wasm32")]
fn get_js_error(op_id: i32) -> StorageError {
    let len = unsafe { b32_gcp_storage_get_error_len(op_id) };
    if len == 0 {
        return StorageError::Other("Unknown error".to_string());
    }

    let mut buf = vec![0u8; len];
    let copied = unsafe { b32_gcp_storage_copy_error(op_id, buf.as_mut_ptr(), len) };
    buf.truncate(copied);
    let msg = String::from_utf8_lossy(&buf).to_string();

    // Parse error message to determine type
    if msg.contains("401") || msg.contains("403") || msg.contains("Not authenticated") {
        StorageError::AuthRequired
    } else if msg.contains("404") {
        StorageError::NotFound(msg)
    } else if msg.contains("429") {
        StorageError::RateLimited
    } else {
        StorageError::NetworkError(msg)
    }
}

/// Get result bytes from JavaScript
#[cfg(target_arch = "wasm32")]
fn get_js_result_bytes(op_id: i32) -> Vec<u8> {
    let len = unsafe { b32_gcp_storage_get_result_len(op_id) };
    if len == 0 {
        return Vec::new();
    }

    let mut buf = vec![0u8; len];
    let copied = unsafe { b32_gcp_storage_copy_result(op_id, buf.as_mut_ptr(), len) };
    buf.truncate(copied);
    buf
}

/// Get result as string from JavaScript
#[cfg(target_arch = "wasm32")]
fn get_js_result_string(op_id: i32) -> String {
    let bytes = get_js_result_bytes(op_id);
    String::from_utf8_lossy(&bytes).to_string()
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

/// Start an async save operation (returns immediately, poll for completion)
#[cfg(target_arch = "wasm32")]
pub fn save_async(path: PathBuf, data: Vec<u8>) -> PendingSave {
    use crate::auth;

    // Check if authenticated
    if !auth::is_authenticated() {
        return PendingSave {
            op: AsyncOp::with_result(Err(StorageError::AuthRequired)),
            path,
        };
    }

    // Start the JS write operation (non-blocking)
    let path_str = path.to_string_lossy();
    let op_id = unsafe {
        b32_gcp_storage_write(
            path_str.as_ptr(),
            path_str.len(),
            data.as_ptr(),
            data.len(),
        )
    };

    eprintln!("[async_ops] save_async started, op_id={}, path={}", op_id, path_str);

    PendingSave {
        op: AsyncOp::new(op_id),
        path,
    }
}

/// Start an async load operation (returns immediately, poll for completion)
#[cfg(target_arch = "wasm32")]
pub fn load_async(path: PathBuf) -> PendingLoad {
    use crate::auth;

    if !auth::is_authenticated() {
        return PendingLoad {
            op: AsyncOp::with_result(Err(StorageError::AuthRequired)),
            path,
        };
    }

    let path_str = path.to_string_lossy();
    let op_id = unsafe { b32_gcp_storage_read(path_str.as_ptr(), path_str.len()) };

    PendingLoad {
        op: AsyncOp::new(op_id),
        path,
    }
}

/// Start an async list operation (returns immediately, poll for completion)
#[cfg(target_arch = "wasm32")]
pub fn list_async(path: String) -> PendingList {
    use crate::auth;

    if !auth::is_authenticated() {
        return PendingList {
            op: AsyncOp::with_result(Err(StorageError::AuthRequired)),
            path,
        };
    }

    let op_id = unsafe { b32_gcp_storage_list(path.as_ptr(), path.len()) };

    PendingList {
        op: AsyncOp::new(op_id),
        path,
    }
}
