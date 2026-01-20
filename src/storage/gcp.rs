//! GCP Cloud Storage backend
//!
//! Provides storage operations via Cloud Run API.
//! Uses a fire-and-poll pattern to integrate with async JavaScript fetch.

use super::{StorageError, StorageHandle};

/// Cloud Run API endpoint (deployed via bonnie-32-infra)
#[allow(dead_code)]
const CLOUD_RUN_URL: &str = "https://bonnie32-storage-api-4bcenv646q-uc.a.run.app";

/// Maximum file size (100 KB)
const MAX_FILE_SIZE: u64 = 100 * 1024;

/// User quota (1 MB)
const USER_QUOTA: u64 = 1024 * 1024;

// ─────────────────────────────────────────────────────────────────────────────
// FFI bindings to JavaScript GcpStorage object (WASM only)
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(target_arch = "wasm32")]
extern "C" {
    fn b32_gcp_storage_list(path_ptr: *const u8, path_len: usize) -> i32;
    fn b32_gcp_storage_read(path_ptr: *const u8, path_len: usize) -> i32;
    fn b32_gcp_storage_write(
        path_ptr: *const u8,
        path_len: usize,
        data_ptr: *const u8,
        data_len: usize,
    ) -> i32;
    fn b32_gcp_storage_delete(path_ptr: *const u8, path_len: usize) -> i32;
    fn b32_gcp_storage_poll(op_id: i32) -> i32;
    fn b32_gcp_storage_get_result_len(op_id: i32) -> usize;
    fn b32_gcp_storage_copy_result(op_id: i32, dest_ptr: *mut u8, max_len: usize) -> usize;
    fn b32_gcp_storage_get_error_len(op_id: i32) -> usize;
    fn b32_gcp_storage_copy_error(op_id: i32, dest_ptr: *mut u8, max_len: usize) -> usize;
    fn b32_gcp_storage_free(op_id: i32);
    fn b32_gcp_storage_get_quota() -> i32;
}

/// Poll status codes from JavaScript
#[cfg(target_arch = "wasm32")]
const POLL_PENDING: i32 = 0;
#[cfg(target_arch = "wasm32")]
const POLL_READY: i32 = 1;
#[cfg(target_arch = "wasm32")]
const POLL_ERROR: i32 = 2;

/// Maximum poll iterations before giving up (prevents infinite loops)
#[cfg(target_arch = "wasm32")]
const MAX_POLL_ITERATIONS: u32 = 100_000;

// ─────────────────────────────────────────────────────────────────────────────
// GCP Cloud Storage backend
// ─────────────────────────────────────────────────────────────────────────────

/// GCP Cloud Storage backend
///
/// Communicates with the Cloud Run service for storage operations.
/// On WASM: Uses FFI to JavaScript for actual network requests.
/// On native: Uses ureq for HTTP requests.
#[derive(Debug, Clone, Default)]
pub struct GcpStorage {
    /// Cached quota usage (updated after operations)
    quota_used: u64,
}

impl GcpStorage {
    /// Create a new GCP storage backend
    pub fn new() -> Self {
        Self::default()
    }

    /// Get the cached quota usage
    pub fn quota_used(&self) -> u64 {
        self.quota_used
    }

    /// Get the quota limit
    pub fn quota_limit(&self) -> u64 {
        USER_QUOTA
    }

    /// Get the Cloud Run API URL
    pub fn api_url(&self) -> &'static str {
        CLOUD_RUN_URL
    }

    /// Check if this backend can write (based on quota)
    pub fn can_write(&self) -> bool {
        self.quota_used < USER_QUOTA
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// WASM implementation (uses JavaScript FFI)
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(target_arch = "wasm32")]
impl GcpStorage {
    /// Wait for an operation to complete, then return result or error
    fn wait_for_operation<T, F>(&self, op_id: i32, extract: F) -> StorageHandle<T>
    where
        F: FnOnce(i32) -> T,
    {
        // Busy-wait for operation to complete
        // Note: This blocks the main thread but is necessary for synchronous API
        for _ in 0..MAX_POLL_ITERATIONS {
            let status = unsafe { b32_gcp_storage_poll(op_id) };

            match status {
                POLL_PENDING => {
                    // Still pending, continue polling
                    continue;
                }
                POLL_READY => {
                    let result = extract(op_id);
                    unsafe { b32_gcp_storage_free(op_id) };
                    return StorageHandle::ready(result);
                }
                POLL_ERROR => {
                    let error = self.get_error(op_id);
                    unsafe { b32_gcp_storage_free(op_id) };
                    return StorageHandle::error(error);
                }
                _ => {
                    unsafe { b32_gcp_storage_free(op_id) };
                    return StorageHandle::error(StorageError::Other(format!(
                        "Unknown poll status: {}",
                        status
                    )));
                }
            }
        }

        // Timed out
        unsafe { b32_gcp_storage_free(op_id) };
        StorageHandle::error(StorageError::NetworkError(
            "Operation timed out".to_string(),
        ))
    }

    /// Get error message from JavaScript
    fn get_error(&self, op_id: i32) -> StorageError {
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
    fn get_result_bytes(&self, op_id: i32) -> Vec<u8> {
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
    fn get_result_string(&self, op_id: i32) -> String {
        let bytes = self.get_result_bytes(op_id);
        String::from_utf8_lossy(&bytes).to_string()
    }

    /// Get user's current quota usage
    pub fn get_quota(&self) -> Option<QuotaInfo> {
        let op_id = unsafe { b32_gcp_storage_get_quota() };

        for _ in 0..MAX_POLL_ITERATIONS {
            let status = unsafe { b32_gcp_storage_poll(op_id) };

            match status {
                POLL_PENDING => continue,
                POLL_READY => {
                    let json = self.get_result_string(op_id);
                    unsafe { b32_gcp_storage_free(op_id) };

                    // Parse JSON manually (avoid serde dependency)
                    let used = parse_json_u64(&json, "used_bytes").unwrap_or(0);
                    let limit = parse_json_u64(&json, "max_bytes").unwrap_or(USER_QUOTA);

                    return Some(QuotaInfo {
                        used,
                        remaining: limit.saturating_sub(used),
                        limit,
                    });
                }
                _ => {
                    unsafe { b32_gcp_storage_free(op_id) };
                    return None;
                }
            }
        }

        unsafe { b32_gcp_storage_free(op_id) };
        None
    }

    /// List files in a directory
    pub fn list(&self, path: &str) -> StorageHandle<Vec<String>> {
        let op_id = unsafe { b32_gcp_storage_list(path.as_ptr(), path.len()) };

        self.wait_for_operation(op_id, |id| {
            let result = self.get_result_string(id);
            if result.is_empty() {
                Vec::new()
            } else {
                result.lines().map(|s| s.to_string()).collect()
            }
        })
    }

    /// Read a file
    pub fn read(&self, path: &str) -> StorageHandle<Vec<u8>> {
        let op_id = unsafe { b32_gcp_storage_read(path.as_ptr(), path.len()) };

        self.wait_for_operation(op_id, |id| self.get_result_bytes(id))
    }

    /// Write a file
    pub fn write(&self, path: &str, data: &[u8]) -> StorageHandle<()> {
        // Check file size limit
        if data.len() as u64 > MAX_FILE_SIZE {
            return StorageHandle::error(StorageError::FileTooLarge {
                size: data.len() as u64,
                max: MAX_FILE_SIZE,
            });
        }

        // Check quota (client-side check, server enforces too)
        if self.quota_used + data.len() as u64 > USER_QUOTA {
            return StorageHandle::error(StorageError::QuotaExceeded {
                used: self.quota_used,
                limit: USER_QUOTA,
            });
        }

        let op_id =
            unsafe { b32_gcp_storage_write(path.as_ptr(), path.len(), data.as_ptr(), data.len()) };

        self.wait_for_operation(op_id, |_| ())
    }

    /// Delete a file
    pub fn delete(&self, path: &str) -> StorageHandle<()> {
        let op_id = unsafe { b32_gcp_storage_delete(path.as_ptr(), path.len()) };

        self.wait_for_operation(op_id, |_| ())
    }

    /// Check if a file exists
    pub fn exists(&self, path: &str) -> StorageHandle<bool> {
        // Check if file exists by trying to read it
        let op_id = unsafe { b32_gcp_storage_read(path.as_ptr(), path.len()) };

        for _ in 0..MAX_POLL_ITERATIONS {
            let status = unsafe { b32_gcp_storage_poll(op_id) };

            match status {
                POLL_PENDING => continue,
                POLL_READY => {
                    unsafe { b32_gcp_storage_free(op_id) };
                    return StorageHandle::ready(true);
                }
                POLL_ERROR => {
                    let error = self.get_error(op_id);
                    unsafe { b32_gcp_storage_free(op_id) };
                    if matches!(error, StorageError::NotFound(_)) {
                        return StorageHandle::ready(false);
                    }
                    return StorageHandle::error(error);
                }
                _ => {
                    unsafe { b32_gcp_storage_free(op_id) };
                    return StorageHandle::error(StorageError::Other("Unknown poll status".into()));
                }
            }
        }

        unsafe { b32_gcp_storage_free(op_id) };
        StorageHandle::error(StorageError::NetworkError("Operation timed out".into()))
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Native implementation (uses ureq for HTTP requests)
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(not(target_arch = "wasm32"))]
impl GcpStorage {
    /// Get user's current quota usage
    pub fn get_quota(&self) -> Option<QuotaInfo> {
        // Use ID token (JWT) for Cloud Run authentication, not access token
        let token = crate::auth::get_id_token();
        if token.is_empty() {
            return None;
        }

        let url = format!("{}/quota", CLOUD_RUN_URL);
        let response = ureq::get(&url)
            .set("Authorization", &format!("Bearer {}", token))
            .call()
            .ok()?;

        // Response format: {"success": true, "data": {"used_bytes": N, "max_bytes": N}}
        let json: serde_json::Value = response.into_json().ok()?;
        let used = json["data"]["used_bytes"].as_u64().unwrap_or(0);
        let limit = json["data"]["max_bytes"].as_u64().unwrap_or(USER_QUOTA);

        Some(QuotaInfo {
            used,
            remaining: limit.saturating_sub(used),
            limit,
        })
    }

    /// Make an authenticated GET request
    fn get_request(&self, endpoint: &str) -> Result<ureq::Response, StorageError> {
        // Use ID token (JWT) for Cloud Run authentication, not access token
        let token = crate::auth::get_id_token();
        if token.is_empty() {
            return Err(StorageError::AuthRequired);
        }

        let url = format!("{}{}", CLOUD_RUN_URL, endpoint);
        ureq::get(&url)
            .set("Authorization", &format!("Bearer {}", token))
            .call()
            .map_err(|e| Self::convert_error(e))
    }

    /// Make an authenticated POST request with JSON body
    fn post_request(
        &self,
        endpoint: &str,
        body: &serde_json::Value,
    ) -> Result<ureq::Response, StorageError> {
        // Use ID token (JWT) for Cloud Run authentication, not access token
        let token = crate::auth::get_id_token();
        if token.is_empty() {
            return Err(StorageError::AuthRequired);
        }

        let url = format!("{}{}", CLOUD_RUN_URL, endpoint);
        ureq::post(&url)
            .set("Authorization", &format!("Bearer {}", token))
            .set("Content-Type", "application/json")
            .send_json(body)
            .map_err(|e| Self::convert_error(e))
    }

    /// Convert ureq error to StorageError
    fn convert_error(e: ureq::Error) -> StorageError {
        match e {
            ureq::Error::Status(401, _) | ureq::Error::Status(403, _) => {
                StorageError::AuthRequired
            }
            ureq::Error::Status(404, _) => StorageError::NotFound("File not found".into()),
            ureq::Error::Status(429, _) => StorageError::RateLimited,
            ureq::Error::Status(code, response) => {
                let body = response.into_string().unwrap_or_default();
                if body.contains("quota") || body.contains("Quota") {
                    // Try to parse quota error
                    StorageError::QuotaExceeded {
                        used: 0,
                        limit: USER_QUOTA,
                    }
                } else {
                    StorageError::NetworkError(format!("HTTP {}: {}", code, body))
                }
            }
            other => StorageError::NetworkError(other.to_string()),
        }
    }

    /// List files in a directory
    pub fn list(&self, path: &str) -> StorageHandle<Vec<String>> {
        let endpoint = format!("/list?prefix={}", urlencoding::encode(path));
        match self.get_request(&endpoint) {
            Ok(response) => match response.into_json::<serde_json::Value>() {
                Ok(json) => {
                    // Response format: {"success": true, "data": {"files": [{"path": "...", "size": N}], "count": N}}
                    let files = json["data"]["files"]
                        .as_array()
                        .map(|arr| {
                            arr.iter()
                                .filter_map(|v| v["path"].as_str().map(|s| s.to_string()))
                                .collect()
                        })
                        .unwrap_or_default();
                    StorageHandle::ready(files)
                }
                Err(e) => {
                    StorageHandle::error(StorageError::Other(format!("JSON parse error: {}", e)))
                }
            },
            Err(e) => StorageHandle::error(e),
        }
    }

    /// Read a file
    pub fn read(&self, path: &str) -> StorageHandle<Vec<u8>> {
        let endpoint = format!("/get?path={}", urlencoding::encode(path));
        match self.get_request(&endpoint) {
            Ok(response) => match response.into_json::<serde_json::Value>() {
                Ok(json) => {
                    // Response format: {"success": true, "data": {"path": "...", "content": "base64...", "size": N}}
                    if let Some(content) = json["data"]["content"].as_str() {
                        // Decode base64 content
                        match base64::Engine::decode(
                            &base64::engine::general_purpose::STANDARD,
                            content,
                        ) {
                            Ok(bytes) => StorageHandle::ready(bytes),
                            Err(e) => StorageHandle::error(StorageError::Other(format!(
                                "Base64 decode error: {}",
                                e
                            ))),
                        }
                    } else {
                        StorageHandle::error(StorageError::Other("No content in response".into()))
                    }
                }
                Err(e) => {
                    StorageHandle::error(StorageError::Other(format!("JSON parse error: {}", e)))
                }
            },
            Err(e) => StorageHandle::error(e),
        }
    }

    /// Write a file
    pub fn write(&self, path: &str, data: &[u8]) -> StorageHandle<()> {
        // Check file size limit
        if data.len() as u64 > MAX_FILE_SIZE {
            return StorageHandle::error(StorageError::FileTooLarge {
                size: data.len() as u64,
                max: MAX_FILE_SIZE,
            });
        }

        // Encode data as base64
        let content = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, data);

        let body = serde_json::json!({
            "path": path,
            "content": content
        });

        match self.post_request("/upload", &body) {
            Ok(_) => StorageHandle::ready(()),
            Err(e) => StorageHandle::error(e),
        }
    }

    /// Delete a file
    pub fn delete(&self, path: &str) -> StorageHandle<()> {
        let body = serde_json::json!({
            "path": path
        });

        match self.post_request("/delete", &body) {
            Ok(_) => StorageHandle::ready(()),
            Err(e) => StorageHandle::error(e),
        }
    }

    /// Check if a file exists
    pub fn exists(&self, path: &str) -> StorageHandle<bool> {
        let endpoint = format!("/get?path={}", urlencoding::encode(path));
        match self.get_request(&endpoint) {
            Ok(_) => StorageHandle::ready(true),
            Err(StorageError::NotFound(_)) => StorageHandle::ready(false),
            Err(e) => StorageHandle::error(e),
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Quota information
// ─────────────────────────────────────────────────────────────────────────────

/// Quota information from the server
#[derive(Debug, Clone)]
pub struct QuotaInfo {
    /// Bytes used
    pub used: u64,
    /// Bytes remaining
    pub remaining: u64,
    /// Total quota limit
    pub limit: u64,
}

impl QuotaInfo {
    /// Usage percentage (0.0 - 1.0)
    pub fn usage_percent(&self) -> f32 {
        if self.limit == 0 {
            0.0
        } else {
            self.used as f32 / self.limit as f32
        }
    }

    /// Human-readable usage string
    pub fn usage_string(&self) -> String {
        let used_kb = self.used as f64 / 1024.0;
        let limit_kb = self.limit as f64 / 1024.0;
        format!("{:.1} KB / {:.1} KB", used_kb, limit_kb)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────────────

/// Simple JSON u64 parser (avoids serde dependency)
#[cfg(target_arch = "wasm32")]
fn parse_json_u64(json: &str, key: &str) -> Option<u64> {
    let pattern = format!("\"{}\":", key);
    let start = json.find(&pattern)? + pattern.len();
    let rest = &json[start..];
    let rest = rest.trim_start();
    let end = rest.find(|c: char| !c.is_ascii_digit()).unwrap_or(rest.len());
    rest[..end].parse().ok()
}
