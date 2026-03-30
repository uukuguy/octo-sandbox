//! End-to-end tests for the WASM hook plugin system.
//!
//! Tests cover: plugin discovery, manifest parsing, host import behavior,
//! capability gating, failure modes, and hooks.yaml integration.

#[cfg(feature = "sandbox-wasm")]
mod wasm_hook_tests {
    use octo_engine::hooks::wasm::host_impl::HookHostState;
    use octo_engine::hooks::wasm::loader::discover_plugins;
    use octo_engine::hooks::wasm::manifest::PluginManifest;
    use octo_engine::hooks::HookContext;
    use std::collections::HashSet;
    use std::fs;
    use std::path::PathBuf;

    // -- Manifest tests --

    #[test]
    fn test_manifest_full_parse() {
        let yaml = r#"
name: my-security-hook
version: 0.1.0
description: Custom security validation
author: user@example.com
wasm: hook.wasm
hook_points:
  - PreToolUse
  - PostToolUse
matcher: "bash|shell_execute"
failure_mode: fail_closed
capabilities:
  - log
  - get-context
  - get-secret
  - http-request
"#;
        let m = PluginManifest::from_yaml(yaml).unwrap();
        assert_eq!(m.name, "my-security-hook");
        assert_eq!(m.version, "0.1.0");
        assert_eq!(m.description.as_deref(), Some("Custom security validation"));
        assert_eq!(m.author.as_deref(), Some("user@example.com"));
        assert_eq!(m.wasm, "hook.wasm");
        assert_eq!(m.hook_points, vec!["PreToolUse", "PostToolUse"]);
        assert_eq!(m.matcher.as_deref(), Some("bash|shell_execute"));
        assert_eq!(m.failure_mode, "fail_closed");
        assert_eq!(m.capabilities.len(), 4);
        assert!(m.has_capability("log"));
        assert!(m.has_capability("get-secret"));
        assert!(m.has_capability("http-request"));
    }

    #[test]
    fn test_manifest_failure_mode_mapping() {
        let yaml_open = r#"
name: open
version: 0.1.0
wasm: hook.wasm
hook_points: [PreToolUse]
failure_mode: fail_open
"#;
        let yaml_closed = r#"
name: closed
version: 0.1.0
wasm: hook.wasm
hook_points: [PreToolUse]
failure_mode: fail_closed
"#;
        let m_open = PluginManifest::from_yaml(yaml_open).unwrap();
        let m_closed = PluginManifest::from_yaml(yaml_closed).unwrap();
        assert_eq!(
            m_open.hook_failure_mode(),
            octo_engine::hooks::HookFailureMode::FailOpen
        );
        assert_eq!(
            m_closed.hook_failure_mode(),
            octo_engine::hooks::HookFailureMode::FailClosed
        );
    }

    #[test]
    fn test_manifest_validation_no_wasm() {
        let yaml = r#"
name: bad
version: 0.1.0
wasm: ""
hook_points: [PreToolUse]
"#;
        assert!(PluginManifest::from_yaml(yaml).is_err());
    }

    #[test]
    fn test_manifest_validation_no_hook_points() {
        let yaml = r#"
name: bad
version: 0.1.0
wasm: hook.wasm
hook_points: []
"#;
        assert!(PluginManifest::from_yaml(yaml).is_err());
    }

    // -- Plugin discovery tests --

    fn create_plugin(dir: &std::path::Path, name: &str, caps: &[&str]) -> PathBuf {
        let plugin_dir = dir.join(name);
        fs::create_dir_all(&plugin_dir).unwrap();

        let caps_yaml: Vec<String> = caps.iter().map(|c| format!("  - {}", c)).collect();
        let caps_section = if caps.is_empty() {
            String::new()
        } else {
            format!("capabilities:\n{}", caps_yaml.join("\n"))
        };

        let manifest = format!(
            "name: {name}\nversion: 0.1.0\nwasm: hook.wasm\nhook_points:\n  - PreToolUse\n{caps_section}\n"
        );
        fs::write(plugin_dir.join("manifest.yaml"), manifest).unwrap();
        fs::write(plugin_dir.join("hook.wasm"), b"fake-wasm-bytes").unwrap();
        plugin_dir
    }

    #[test]
    fn test_discover_multiple_plugins() {
        let tmp = tempfile::tempdir().unwrap();
        create_plugin(tmp.path(), "auth-hook", &["log"]);
        create_plugin(tmp.path(), "audit-hook", &["log", "get-context"]);
        create_plugin(tmp.path(), "network-hook", &["log", "http-request"]);

        let plugins = discover_plugins(&[tmp.path().to_path_buf()]);
        assert_eq!(plugins.len(), 3);
    }

    #[test]
    fn test_discover_project_overrides_global() {
        let global = tempfile::tempdir().unwrap();
        let project = tempfile::tempdir().unwrap();

        create_plugin(global.path(), "shared-hook", &["log"]);
        create_plugin(project.path(), "shared-hook", &["log", "get-secret"]);

        let plugins = discover_plugins(&[
            global.path().to_path_buf(),
            project.path().to_path_buf(),
        ]);
        assert_eq!(plugins.len(), 1);
        // Project-level plugin should win
        assert!(plugins[0].plugin_dir.starts_with(project.path()));
        assert!(plugins[0].manifest.has_capability("get-secret"));
    }

    #[test]
    fn test_discover_skips_invalid_plugins() {
        let tmp = tempfile::tempdir().unwrap();
        create_plugin(tmp.path(), "valid", &[]);

        // Create invalid plugin (no wasm file)
        let bad_dir = tmp.path().join("invalid");
        fs::create_dir_all(&bad_dir).unwrap();
        fs::write(
            bad_dir.join("manifest.yaml"),
            "name: invalid\nversion: 0.1.0\nwasm: missing.wasm\nhook_points:\n  - PreToolUse\n",
        )
        .unwrap();

        let plugins = discover_plugins(&[tmp.path().to_path_buf()]);
        assert_eq!(plugins.len(), 1);
        assert_eq!(plugins[0].manifest.name, "valid");
    }

    // -- Host import capability gating tests --

    use octo_engine::hooks::wasm::bindings::octo::hook::host::Host;

    fn make_host_state(capabilities: &[&str]) -> HookHostState {
        let ctx = HookContext::default();
        let caps: HashSet<String> = capabilities.iter().map(|s| s.to_string()).collect();
        HookHostState::new(ctx, caps, "test-plugin".to_string())
    }

    #[test]
    fn test_get_secret_requires_capability() {
        let mut state = make_host_state(&["log"]);
        let result = state.get_secret("API_KEY".to_string());
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("does not have 'get-secret' capability"));
    }

    #[test]
    fn test_get_secret_with_capability() {
        let mut state = make_host_state(&["get-secret"]);
        // Set a test env var
        std::env::set_var("OCTO_TEST_SECRET_KEY_8492", "test-value");
        let result = state.get_secret("OCTO_TEST_SECRET_KEY_8492".to_string());
        assert_eq!(result.unwrap(), "test-value");
        std::env::remove_var("OCTO_TEST_SECRET_KEY_8492");
    }

    #[test]
    fn test_http_requires_capability() {
        let mut state = make_host_state(&["log"]);
        let result = state.http_request(
            "GET".to_string(),
            "https://example.com".to_string(),
            "{}".to_string(),
            String::new(),
        );
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("does not have 'http-request' capability"));
    }

    #[test]
    fn test_http_ssrf_protection_localhost() {
        let mut state = make_host_state(&["http-request"]);
        let result = state.http_request(
            "GET".to_string(),
            "http://127.0.0.1:9999/secret".to_string(),
            "{}".to_string(),
            String::new(),
        );
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("SSRF blocked"));
    }

    #[test]
    fn test_http_ssrf_protection_private_network() {
        let mut state = make_host_state(&["http-request"]);
        let result = state.http_request(
            "GET".to_string(),
            "http://192.168.1.1/admin".to_string(),
            "{}".to_string(),
            String::new(),
        );
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("SSRF blocked"));
    }

    #[test]
    fn test_log_does_not_panic() {
        let mut state = make_host_state(&[]);
        // Should not panic regardless of log level
        state.log("info".to_string(), "test message".to_string());
        state.log("warn".to_string(), "warning message".to_string());
        state.log("error".to_string(), "error message".to_string());
        state.log("unknown".to_string(), "unknown level".to_string());
    }

    #[test]
    fn test_get_context_returns_valid_json() {
        let mut state = make_host_state(&[]);
        let json = state.get_context();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert!(parsed.is_object());
    }

    #[test]
    fn test_now_millis_reasonable() {
        let mut state = make_host_state(&[]);
        let ts = state.now_millis();
        // Should be after 2024-01-01T00:00:00Z (1704067200000 ms)
        assert!(ts > 1_704_067_200_000);
        // And before 2030 (1893456000000 ms)
        assert!(ts < 1_893_456_000_000);
    }

    // -- hooks.yaml wasm action type integration --

    #[test]
    fn test_hooks_config_with_mixed_actions() {
        let yaml = r#"
version: 1
hooks:
  PreToolUse:
    - matcher: "bash"
      actions:
        - type: command
          command: "python3 validate.py"
        - type: wasm
          plugin: "my-security-hook"
          failure_mode: fail_closed
        - type: prompt
          prompt: "Evaluate safety..."
"#;
        let config: octo_engine::hooks::declarative::HooksConfig =
            serde_yaml::from_str(yaml).unwrap();
        let pre = &config.hooks["PreToolUse"];
        assert_eq!(pre[0].actions.len(), 3);

        // Verify each action type
        assert!(matches!(
            pre[0].actions[0],
            octo_engine::hooks::declarative::HookActionConfig::Command { .. }
        ));
        assert!(matches!(
            pre[0].actions[1],
            octo_engine::hooks::declarative::HookActionConfig::Wasm { .. }
        ));
        assert!(matches!(
            pre[0].actions[2],
            octo_engine::hooks::declarative::HookActionConfig::Prompt { .. }
        ));
    }
}
