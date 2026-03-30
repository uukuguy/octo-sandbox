//! Example WASM hook plugin: Security Validator
//!
//! This plugin blocks dangerous bash commands (rm -rf /, fork bombs, etc.)
//! while allowing safe commands through. Demonstrates the octo-hook WIT
//! interface with host imports (log, get-context).

// Generate guest bindings from the WIT interface
wit_bindgen::generate!({
    world: "octo-hook-plugin",
    path: "../../crates/octo-engine/wit/octo-hook.wit",
});

struct SecurityHook;

impl Guest for SecurityHook {
    fn name() -> String {
        "security-validator".to_string()
    }

    fn priority() -> u32 {
        10 // High priority — runs early
    }

    fn supported_events() -> String {
        "PreToolUse".to_string()
    }

    fn execute(context_json: String) -> Result<String, String> {
        // Log that we're running
        octo::hook::host::log("info", "security-validator: checking command");

        // Parse the context to extract tool input
        let context: serde_json::Value =
            serde_json::from_str(&context_json).map_err(|e| format!("JSON parse error: {}", e))?;

        let tool_name = context
            .get("tool_name")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        // Only check bash/shell commands
        if tool_name != "bash" && tool_name != "shell_execute" {
            return Ok(r#"{"decision": "allow"}"#.to_string());
        }

        // Extract the command from tool_input
        let command = context
            .get("tool_input")
            .and_then(|v| v.get("command"))
            .and_then(|v| v.as_str())
            .unwrap_or("");

        // Check for dangerous patterns
        let dangerous_patterns = [
            "rm -rf /",
            "rm -rf /*",
            ":(){ :|:& };:",      // fork bomb
            "dd if=/dev/zero",
            "mkfs.",
            "> /dev/sda",
            "chmod -R 777 /",
        ];

        for pattern in &dangerous_patterns {
            if command.contains(pattern) {
                let reason = format!("Blocked dangerous command pattern: '{}'", pattern);
                octo::hook::host::log("warn", &format!("security-validator: {}", reason));

                return Ok(serde_json::json!({
                    "decision": "deny",
                    "reason": reason
                })
                .to_string());
            }
        }

        // Get timestamp for audit logging
        let _ts = octo::hook::host::now_millis();

        Ok(r#"{"decision": "allow"}"#.to_string())
    }
}

export!(SecurityHook);
