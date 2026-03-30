//! Host import implementations for WASM hook plugins.
//!
//! Implements the 5 host functions defined in `octo-hook.wit`:
//! - `log(level, message)` — structured logging
//! - `get-context()` — full HookContext as JSON
//! - `get-secret(key)` — capability-gated secret retrieval
//! - `http-request(method, url, headers, body)` — capability-gated HTTP
//! - `now-millis()` — current timestamp

use std::collections::HashSet;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::hooks::HookContext;

/// Host state passed to WASM component instances.
///
/// Contains the hook context and security constraints (allowed capabilities).
pub struct HookHostState {
    /// The current hook context (serialized to JSON on `get_context`).
    pub context: HookContext,
    /// Set of allowed host capabilities (from manifest.capabilities).
    pub allowed_capabilities: HashSet<String>,
    /// Plugin name (for logging).
    pub plugin_name: String,
}

impl HookHostState {
    pub fn new(
        context: HookContext,
        allowed_capabilities: HashSet<String>,
        plugin_name: String,
    ) -> Self {
        Self {
            context,
            allowed_capabilities,
            plugin_name,
        }
    }

    fn has_capability(&self, cap: &str) -> bool {
        self.allowed_capabilities.contains(cap)
    }
}

/// Implementation of the WIT `host` interface.
///
/// Uses the generated `Host` trait from `bindings::octo::hook::host::Host`.
#[cfg(feature = "sandbox-wasm")]
impl super::bindings::octo::hook::host::Host for HookHostState {
    fn log(&mut self, level: String, message: String) {
        match level.as_str() {
            "trace" => tracing::trace!(plugin = %self.plugin_name, "{}", message),
            "debug" => tracing::debug!(plugin = %self.plugin_name, "{}", message),
            "info" => tracing::info!(plugin = %self.plugin_name, "{}", message),
            "warn" => tracing::warn!(plugin = %self.plugin_name, "{}", message),
            "error" => tracing::error!(plugin = %self.plugin_name, "{}", message),
            _ => tracing::info!(plugin = %self.plugin_name, level = %level, "{}", message),
        }
    }

    fn get_context(&mut self) -> String {
        serde_json::to_string(&self.context).unwrap_or_else(|e| {
            format!("{{\"error\": \"Failed to serialize context: {}\"}}", e)
        })
    }

    fn get_secret(&mut self, key: String) -> Result<String, String> {
        if !self.has_capability("get-secret") {
            return Err(format!(
                "Plugin '{}' does not have 'get-secret' capability",
                self.plugin_name
            ));
        }
        // In production, this would delegate to CredentialResolver.
        // For now, check environment variables as a simple fallback.
        std::env::var(&key).map_err(|_| format!("Secret '{}' not found", key))
    }

    fn http_request(
        &mut self,
        method: String,
        url: String,
        headers_json: String,
        body: String,
    ) -> Result<String, String> {
        if !self.has_capability("http-request") {
            return Err(format!(
                "Plugin '{}' does not have 'http-request' capability",
                self.plugin_name
            ));
        }

        // SSRF protection: block localhost and private networks
        if let Ok(parsed) = url::Url::parse(&url) {
            if let Some(host) = parsed.host_str() {
                let is_private = host == "localhost"
                    || host == "127.0.0.1"
                    || host == "::1"
                    || host.starts_with("10.")
                    || host.starts_with("192.168.")
                    || host.starts_with("172.");
                if is_private {
                    return Err(format!("SSRF blocked: requests to '{}' are not allowed", host));
                }
            }
        }

        // Synchronous HTTP request (WASM component calls are synchronous)
        let client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(5))
            .build()
            .map_err(|e| format!("Failed to build HTTP client: {}", e))?;

        let parsed_method = reqwest::Method::from_bytes(method.as_bytes())
            .map_err(|e| format!("Invalid HTTP method: {}", e))?;

        let mut request = client.request(parsed_method, &url);

        // Parse and apply headers
        if !headers_json.is_empty() {
            if let Ok(headers) = serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(&headers_json) {
                for (k, v) in headers {
                    if let Some(val) = v.as_str() {
                        request = request.header(&k, val);
                    }
                }
            }
        }

        if !body.is_empty() {
            request = request.body(body);
        }

        let response = request.send().map_err(|e| format!("HTTP request failed: {}", e))?;

        // Enforce 1MB response limit
        let status = response.status().as_u16();
        let body_bytes = response
            .bytes()
            .map_err(|e| format!("Failed to read response: {}", e))?;

        if body_bytes.len() > 1_048_576 {
            return Err("Response body exceeds 1MB limit".to_string());
        }

        let body_str = String::from_utf8_lossy(&body_bytes);
        Ok(serde_json::json!({
            "status": status,
            "body": body_str,
        })
        .to_string())
    }

    fn now_millis(&mut self) -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(feature = "sandbox-wasm")]
    use super::super::bindings::octo::hook::host::Host;

    fn make_state(capabilities: &[&str]) -> HookHostState {
        let mut context = HookContext::default();
        context.tool_name = Some("test-tool".to_string());
        let caps: HashSet<String> = capabilities.iter().map(|s| s.to_string()).collect();
        HookHostState::new(context, caps, "test-plugin".to_string())
    }

    #[test]
    fn test_has_capability() {
        let state = make_state(&["log", "get-context"]);
        assert!(state.has_capability("log"));
        assert!(state.has_capability("get-context"));
        assert!(!state.has_capability("get-secret"));
    }

    #[cfg(feature = "sandbox-wasm")]
    #[test]
    fn test_now_millis() {
        let mut state = make_state(&[]);
        let ts = state.now_millis();
        assert!(ts > 1_700_000_000_000); // After 2023
    }

    #[cfg(feature = "sandbox-wasm")]
    #[test]
    fn test_get_context_returns_json() {
        let mut state = make_state(&[]);
        let json = state.get_context();
        assert!(json.contains("test-tool"));
    }

    #[cfg(feature = "sandbox-wasm")]
    #[test]
    fn test_get_secret_without_capability() {
        let mut state = make_state(&["log"]);
        let result = state.get_secret("MY_KEY".to_string());
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("does not have 'get-secret' capability"));
    }

    #[cfg(feature = "sandbox-wasm")]
    #[test]
    fn test_http_request_without_capability() {
        let mut state = make_state(&["log"]);
        let result = state.http_request(
            "GET".to_string(),
            "https://example.com".to_string(),
            "{}".to_string(),
            String::new(),
        );
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("does not have 'http-request' capability"));
    }

    #[cfg(feature = "sandbox-wasm")]
    #[test]
    fn test_http_request_ssrf_blocked() {
        let mut state = make_state(&["http-request"]);
        let result = state.http_request(
            "GET".to_string(),
            "http://localhost:8080/admin".to_string(),
            "{}".to_string(),
            String::new(),
        );
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("SSRF blocked"));
    }
}
