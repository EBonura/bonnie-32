//! GCP Authentication Module
//!
//! Provides authentication state and methods for Google OAuth.
//! - WASM: Uses JavaScript FFI bindings to Google Identity Services
//! - Native: Opens browser for OAuth, receives callback on localhost:4040

/// Authentication state
#[derive(Debug, Clone, Default)]
pub struct AuthState {
    /// Whether the user is authenticated
    pub authenticated: bool,
    /// Hashed user ID (SHA256 of Google user ID)
    pub user_id_hash: Option<String>,
}

impl AuthState {
    /// Create a new auth state
    pub fn new() -> Self {
        Self::default()
    }

    /// Update auth state from current authentication status
    pub fn update(&mut self) {
        self.authenticated = is_authenticated();
        if self.authenticated {
            self.user_id_hash = Some(get_user_id_hash());
        } else {
            self.user_id_hash = None;
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// WASM implementation (uses JavaScript FFI)
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(target_arch = "wasm32")]
extern "C" {
    fn b32_gcp_auth_init();
    fn b32_gcp_auth_sign_in();
    fn b32_gcp_auth_sign_out();
    fn b32_gcp_auth_is_authenticated() -> i32;
    fn b32_gcp_auth_get_token_len() -> usize;
    fn b32_gcp_auth_copy_token(dest: *mut u8, max_len: usize) -> usize;
    fn b32_gcp_auth_get_user_hash_len() -> usize;
    fn b32_gcp_auth_copy_user_hash(dest: *mut u8, max_len: usize) -> usize;
}

#[cfg(target_arch = "wasm32")]
pub fn init() {
    unsafe { b32_gcp_auth_init() }
}

#[cfg(target_arch = "wasm32")]
pub fn sign_in() {
    unsafe { b32_gcp_auth_sign_in() }
}

#[cfg(target_arch = "wasm32")]
pub fn sign_out() {
    unsafe { b32_gcp_auth_sign_out() }
}

#[cfg(target_arch = "wasm32")]
pub fn is_authenticated() -> bool {
    unsafe { b32_gcp_auth_is_authenticated() != 0 }
}

#[cfg(target_arch = "wasm32")]
pub fn get_access_token() -> String {
    let len = unsafe { b32_gcp_auth_get_token_len() };
    if len == 0 {
        return String::new();
    }

    let mut buf = vec![0u8; len];
    let copied = unsafe { b32_gcp_auth_copy_token(buf.as_mut_ptr(), len) };
    buf.truncate(copied);
    String::from_utf8_lossy(&buf).to_string()
}

#[cfg(target_arch = "wasm32")]
pub fn get_user_id_hash() -> String {
    let len = unsafe { b32_gcp_auth_get_user_hash_len() };
    if len == 0 {
        return String::new();
    }

    let mut buf = vec![0u8; len];
    let copied = unsafe { b32_gcp_auth_copy_user_hash(buf.as_mut_ptr(), len) };
    buf.truncate(copied);
    String::from_utf8_lossy(&buf).to_string()
}

// ─────────────────────────────────────────────────────────────────────────────
// Native implementation (browser OAuth with localhost callback)
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(not(target_arch = "wasm32"))]
mod native {
    use sha2::{Sha256, Digest};
    use std::sync::Mutex;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    /// OAuth client ID for Desktop app (public, safe to embed)
    /// Note: This is different from the Web client ID used in WASM (index.html)
    const CLIENT_ID: &str = "93370111666-9ofn2c618pt2l557j24tet98aevb03ve.apps.googleusercontent.com";

    /// OAuth client secret for Desktop app
    /// Note: This is NOT truly secret for installed/desktop apps - Google acknowledges
    /// that desktop apps "cannot keep secrets" and classifies them as public clients.
    /// The secret can be extracted from binaries. PKCE is the real security mechanism.
    const CLIENT_SECRET: &str = "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";

    /// Redirect URI for native OAuth
    const REDIRECT_URI: &str = "http://localhost:4040/callback";

    /// Token storage
    #[derive(Default)]
    pub struct TokenStore {
        pub access_token: Option<String>,
        pub id_token: Option<String>,  // JWT for Cloud Run auth
        pub user_id_hash: Option<String>,
        pub token_expiry: u64, // Unix timestamp in seconds
    }

    lazy_static::lazy_static! {
        pub static ref TOKENS: Mutex<TokenStore> = Mutex::new(TokenStore::default());
    }

    /// Generate a random PKCE code verifier (43-128 characters)
    fn generate_code_verifier() -> String {
        use rand::Rng;
        const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-._~";
        let mut rng = rand::thread_rng();
        (0..64)
            .map(|_| {
                let idx = rng.gen_range(0..CHARSET.len());
                CHARSET[idx] as char
            })
            .collect()
    }

    /// Generate PKCE code challenge from verifier (SHA256, base64url encoded)
    fn generate_code_challenge(verifier: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(verifier.as_bytes());
        let hash = hasher.finalize();
        base64_url_encode(&hash)
    }

    /// Base64 URL encode (no padding)
    fn base64_url_encode(data: &[u8]) -> String {
        base64::Engine::encode(&base64::engine::general_purpose::URL_SAFE_NO_PAD, data)
    }

    /// SHA256 hash a string and return hex
    fn sha256_hex(input: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(input.as_bytes());
        let hash = hasher.finalize();
        hash.iter().map(|b| format!("{:02x}", b)).collect()
    }

    /// Get the token file path
    fn token_file_path() -> std::path::PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("."))
            .join("bonnie-32")
            .join("auth_token.json")
    }

    /// Save tokens to disk
    fn save_tokens(store: &TokenStore) {
        let path = token_file_path();
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }

        let json = serde_json::json!({
            "access_token": store.access_token,
            "id_token": store.id_token,
            "user_id_hash": store.user_id_hash,
            "token_expiry": store.token_expiry,
        });

        if let Ok(contents) = serde_json::to_string_pretty(&json) {
            let _ = std::fs::write(&path, contents);
        }
    }

    /// Load tokens from disk
    fn load_tokens() -> Option<TokenStore> {
        let path = token_file_path();
        let contents = std::fs::read_to_string(&path).ok()?;
        let json: serde_json::Value = serde_json::from_str(&contents).ok()?;

        let access_token = json["access_token"].as_str().map(|s| s.to_string());
        let id_token = json["id_token"].as_str().map(|s| s.to_string());
        let user_id_hash = json["user_id_hash"].as_str().map(|s| s.to_string());
        let token_expiry = json["token_expiry"].as_u64().unwrap_or(0);

        Some(TokenStore {
            access_token,
            id_token,
            user_id_hash,
            token_expiry,
        })
    }

    /// Delete tokens from disk
    fn delete_tokens() {
        let path = token_file_path();
        let _ = std::fs::remove_file(&path);
    }

    /// Current unix timestamp
    fn now_unix() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or(Duration::ZERO)
            .as_secs()
    }

    /// Initialize auth - load tokens from disk if valid
    pub fn init() {
        if let Some(stored) = load_tokens() {
            // Check if token is still valid (with 5 minute buffer)
            if stored.token_expiry > now_unix() + 300 {
                let mut tokens = TOKENS.lock().unwrap();
                *tokens = stored;
                println!("Auth restored from disk");
            } else {
                // Token expired, delete it
                delete_tokens();
            }
        }
    }

    /// Start OAuth sign-in flow
    pub fn sign_in() {
        // Spawn a thread to handle OAuth flow (don't block the main thread)
        std::thread::spawn(|| {
            if let Err(e) = do_sign_in() {
                eprintln!("Sign-in failed: {}", e);
            }
        });
    }

    /// Perform the OAuth flow
    fn do_sign_in() -> Result<(), String> {
        // Generate PKCE codes
        let code_verifier = generate_code_verifier();
        let code_challenge = generate_code_challenge(&code_verifier);

        // Build authorization URL
        let auth_url = format!(
            "https://accounts.google.com/o/oauth2/v2/auth?\
            client_id={}&\
            redirect_uri={}&\
            response_type=code&\
            scope=openid%20email&\
            code_challenge={}&\
            code_challenge_method=S256&\
            access_type=offline",
            CLIENT_ID,
            urlencoding::encode(REDIRECT_URI),
            code_challenge
        );

        // Start local HTTP server to receive callback
        let server = tiny_http::Server::http("127.0.0.1:4040")
            .map_err(|e| format!("Failed to start callback server: {}", e))?;

        // Open browser
        println!("Opening browser for Google sign-in...");
        if webbrowser::open(&auth_url).is_err() {
            eprintln!("Failed to open browser. Please visit:\n{}", auth_url);
        }

        // Wait for callback (with timeout)
        println!("Waiting for authentication...");
        let auth_code = wait_for_callback(&server)?;

        // Exchange auth code for tokens
        println!("Exchanging auth code for tokens...");
        let (access_token, id_token, expires_in) = exchange_code(&auth_code, &code_verifier)?;

        // Fetch user info
        println!("Fetching user info...");
        let user_id_hash = fetch_user_info(&access_token)?;

        // Store tokens
        let token_expiry = now_unix() + expires_in;
        {
            let mut tokens = TOKENS.lock().unwrap();
            tokens.access_token = Some(access_token);
            tokens.id_token = id_token;
            tokens.user_id_hash = Some(user_id_hash);
            tokens.token_expiry = token_expiry;
            save_tokens(&tokens);
        }

        println!("Sign-in successful!");
        Ok(())
    }

    /// Wait for OAuth callback on the local server
    fn wait_for_callback(server: &tiny_http::Server) -> Result<String, String> {
        // Set a timeout by using recv_timeout
        let timeout = Duration::from_secs(120);
        let start = std::time::Instant::now();

        loop {
            if start.elapsed() > timeout {
                return Err("Authentication timed out".to_string());
            }

            // Check for incoming request (non-blocking with short timeout)
            if let Ok(Some(request)) = server.recv_timeout(Duration::from_millis(100)) {
                let url = request.url().to_string();

                // Parse the callback URL for the auth code
                if url.starts_with("/callback") {
                    // Extract code from query string
                    let code = url
                        .split('?')
                        .nth(1)
                        .and_then(|query| {
                            query.split('&').find_map(|param| {
                                let mut parts = param.split('=');
                                if parts.next() == Some("code") {
                                    parts.next().map(|s| urlencoding::decode(s).unwrap_or_default().to_string())
                                } else {
                                    None
                                }
                            })
                        });

                    // Send response to browser
                    let response_body = if code.is_some() {
                        "<html><body><h1>Authentication successful!</h1><p>You can close this window and return to BONNIE-32.</p></body></html>"
                    } else {
                        "<html><body><h1>Authentication failed</h1><p>No authorization code received.</p></body></html>"
                    };

                    let response = tiny_http::Response::from_string(response_body)
                        .with_header(tiny_http::Header::from_bytes(&b"Content-Type"[..], &b"text/html"[..]).unwrap());
                    let _ = request.respond(response);

                    if let Some(code) = code {
                        return Ok(code);
                    } else {
                        return Err("No authorization code in callback".to_string());
                    }
                }

                // Respond to other requests with 404
                let response = tiny_http::Response::from_string("Not Found")
                    .with_status_code(404);
                let _ = request.respond(response);
            }
        }
    }

    /// Exchange authorization code for tokens (access_token and id_token)
    fn exchange_code(auth_code: &str, code_verifier: &str) -> Result<(String, Option<String>, u64), String> {
        let body = format!(
            "client_id={}&client_secret={}&code={}&redirect_uri={}&grant_type=authorization_code&code_verifier={}",
            CLIENT_ID,
            CLIENT_SECRET,
            urlencoding::encode(auth_code),
            urlencoding::encode(REDIRECT_URI),
            code_verifier
        );

        let response = ureq::post("https://oauth2.googleapis.com/token")
            .set("Content-Type", "application/x-www-form-urlencoded")
            .send_string(&body)
            .map_err(|e| {
                // Try to extract the error response body for better debugging
                match e {
                    ureq::Error::Status(code, response) => {
                        let body = response.into_string().unwrap_or_default();
                        format!("Token exchange failed ({}): {}", code, body)
                    }
                    other => format!("Token exchange failed: {}", other)
                }
            })?;

        let json: serde_json::Value = response.into_json()
            .map_err(|e| format!("Failed to parse token response: {}", e))?;

        let access_token = json["access_token"]
            .as_str()
            .ok_or("No access_token in response")?
            .to_string();

        // ID token is returned when openid scope is requested
        let id_token = json["id_token"]
            .as_str()
            .map(|s| s.to_string());

        let expires_in = json["expires_in"]
            .as_u64()
            .unwrap_or(3600);

        Ok((access_token, id_token, expires_in))
    }

    /// Fetch user info and return hashed user ID
    fn fetch_user_info(access_token: &str) -> Result<String, String> {
        let response = ureq::get("https://www.googleapis.com/oauth2/v3/userinfo")
            .set("Authorization", &format!("Bearer {}", access_token))
            .call()
            .map_err(|e| format!("Failed to fetch user info: {}", e))?;

        let json: serde_json::Value = response.into_json()
            .map_err(|e| format!("Failed to parse user info: {}", e))?;

        let sub = json["sub"]
            .as_str()
            .ok_or("No 'sub' field in user info")?;

        // Hash the user ID for privacy
        Ok(sha256_hex(sub))
    }

    /// Sign out - clear tokens
    pub fn sign_out() {
        let mut tokens = TOKENS.lock().unwrap();
        tokens.access_token = None;
        tokens.id_token = None;
        tokens.user_id_hash = None;
        tokens.token_expiry = 0;
        delete_tokens();
        println!("Signed out");
    }

    /// Check if authenticated (token exists and not expired)
    pub fn is_authenticated() -> bool {
        let tokens = TOKENS.lock().unwrap();
        let has_token = tokens.access_token.is_some();
        let not_expired = tokens.token_expiry > now_unix();
        has_token && not_expired
    }

    /// Get the access token (for Google APIs like userinfo)
    pub fn get_access_token() -> String {
        let tokens = TOKENS.lock().unwrap();
        tokens.access_token.clone().unwrap_or_default()
    }

    /// Get the ID token (JWT for Cloud Run authentication)
    pub fn get_id_token() -> String {
        let tokens = TOKENS.lock().unwrap();
        tokens.id_token.clone().unwrap_or_default()
    }

    /// Get the hashed user ID
    pub fn get_user_id_hash() -> String {
        let tokens = TOKENS.lock().unwrap();
        tokens.user_id_hash.clone().unwrap_or_default()
    }
}

// Re-export native functions
#[cfg(not(target_arch = "wasm32"))]
pub use native::{init, sign_in, sign_out, is_authenticated, get_id_token, get_user_id_hash};
