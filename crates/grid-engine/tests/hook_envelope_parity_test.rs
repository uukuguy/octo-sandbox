//! Phase 2.5 S0.T3 — ADR-V2-006 §2/§3 envelope parity integration tests.
//!
//! These tests lock `HookContext::to_json()` + `HookContext::to_env_vars()`
//! to the byte shapes emitted by the Python claude-code-runtime
//! (`lang/claude-code-runtime-python/src/claude_code_runtime/service.py`
//! in `_dispatch_scoped_pre_tool_use` / `_dispatch_scoped_post_tool_use`
//! / `_dispatch_scoped_stop`). Any drift between the two runtimes that
//! survives this file should be considered a D120-regression and
//! blocks Phase 2.5 W1 (goose-runtime) certification.
//!
//! Reference envelopes (ADR-V2-006 §2.1–§2.3 canonical examples):
//!
//! PreToolUse:
//! ```json
//! {
//!   "event": "PreToolUse",
//!   "session_id": "sess-abc123",
//!   "skill_id": "threshold-calibration",
//!   "tool_name": "scada_write",
//!   "tool_args": {"device_id": "xfmr-042", "value": 75.0},
//!   "created_at": "2026-04-15T10:30:00Z"
//! }
//! ```
//!
//! PostToolUse:
//! ```json
//! {
//!   "event": "PostToolUse",
//!   "session_id": "sess-abc123",
//!   "skill_id": "threshold-calibration",
//!   "tool_name": "scada_write",
//!   "tool_result": "ok",
//!   "is_error": false,
//!   "created_at": "2026-04-15T10:30:05Z"
//! }
//! ```
//!
//! Stop:
//! ```json
//! {
//!   "event": "Stop",
//!   "session_id": "sess-abc123",
//!   "skill_id": "threshold-calibration",
//!   "draft_memory_id": "mem-42",
//!   "evidence_anchor_id": "anchor-99",
//!   "created_at": "2026-04-15T10:31:00Z"
//! }
//! ```

use grid_engine::hooks::HookContext;
use serde_json::json;
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// §2 stdin envelope — byte-parity with Python reference
// ---------------------------------------------------------------------------

#[test]
fn hook_envelope_pre_tool_use_matches_adr_example() {
    let ctx = HookContext::new()
        .with_event("PreToolUse")
        .with_session("sess-abc123")
        .with_skill_id("threshold-calibration")
        .with_tool(
            "scada_write",
            json!({"device_id": "xfmr-042", "value": 75.0}),
        )
        .with_created_at("2026-04-15T10:30:00Z");

    let actual = ctx.to_json();
    let expected = json!({
        "event": "PreToolUse",
        "session_id": "sess-abc123",
        "skill_id": "threshold-calibration",
        "tool_name": "scada_write",
        "tool_args": {"device_id": "xfmr-042", "value": 75.0},
        "created_at": "2026-04-15T10:30:00Z",
    });
    assert_eq!(actual, expected, "PreToolUse envelope mismatch");
}

#[test]
fn hook_envelope_post_tool_use_matches_adr_example() {
    // Post envelope uses `tool_result` string + `is_error` bool. The
    // HookContext stores `tool_result: Value` and `success: bool`, so
    // we set them explicitly and assert the envelope mapper produces
    // the canonical shape.
    let mut ctx = HookContext::new()
        .with_event("PostToolUse")
        .with_session("sess-abc123")
        .with_skill_id("threshold-calibration")
        .with_tool("scada_write", json!({}))
        .with_result(true, 0)
        .with_created_at("2026-04-15T10:30:05Z");
    ctx.tool_result = Some(serde_json::Value::String("ok".into()));

    let actual = ctx.to_json();
    let expected = json!({
        "event": "PostToolUse",
        "session_id": "sess-abc123",
        "skill_id": "threshold-calibration",
        "tool_name": "scada_write",
        "tool_result": "ok",
        "is_error": false,
        "created_at": "2026-04-15T10:30:05Z",
    });
    assert_eq!(actual, expected, "PostToolUse envelope mismatch");
}

#[test]
fn hook_envelope_stop_matches_adr_example() {
    let ctx = HookContext::new()
        .with_event("Stop")
        .with_session("sess-abc123")
        .with_skill_id("threshold-calibration")
        .with_draft_memory_id("mem-42")
        .with_evidence_anchor_id("anchor-99")
        .with_created_at("2026-04-15T10:31:00Z");

    let actual = ctx.to_json();
    let expected = json!({
        "event": "Stop",
        "session_id": "sess-abc123",
        "skill_id": "threshold-calibration",
        "draft_memory_id": "mem-42",
        "evidence_anchor_id": "anchor-99",
        "created_at": "2026-04-15T10:31:00Z",
    });
    assert_eq!(actual, expected, "Stop envelope mismatch");
}

// ---------------------------------------------------------------------------
// §2.3 optional fields default to empty string (NOT null, NOT missing)
// ---------------------------------------------------------------------------

#[test]
fn hook_envelope_stop_absent_optionals_serialize_as_empty_string() {
    // No `with_draft_memory_id` / `with_evidence_anchor_id` calls —
    // the envelope MUST still emit them as "" per §2.3, never null,
    // never absent.
    let ctx = HookContext::new()
        .with_event("Stop")
        .with_session("sess-xyz")
        .with_skill_id("")
        .with_created_at("2026-04-16T14:31:00Z");

    let json = ctx.to_json();
    assert_eq!(json["draft_memory_id"], "", "must be empty string");
    assert_eq!(json["evidence_anchor_id"], "", "must be empty string");
    assert!(!json["draft_memory_id"].is_null(), "null is forbidden");
    assert!(!json["evidence_anchor_id"].is_null(), "null is forbidden");
    assert!(
        json.get("draft_memory_id").is_some(),
        "missing is forbidden"
    );
    assert!(
        json.get("evidence_anchor_id").is_some(),
        "missing is forbidden"
    );

    // Serialize to a string and grep for the literal "null" to guard
    // against a future serde quirk that might emit null for an Option
    // that slipped past the custom serializer.
    let as_str = serde_json::to_string(&json).unwrap();
    assert!(
        !as_str.contains(r#""draft_memory_id":null"#),
        "draft_memory_id must not serialize to null: {as_str}"
    );
    assert!(
        !as_str.contains(r#""evidence_anchor_id":null"#),
        "evidence_anchor_id must not serialize to null: {as_str}"
    );
}

#[test]
fn hook_envelope_skill_id_empty_when_no_skill_attached() {
    let ctx = HookContext::new()
        .with_event("PreToolUse")
        .with_session("sess-xyz")
        // NOTE: deliberately not setting skill_id — mirrors Python
        // `skill_id = loader.first_skill_id() if loader else ""`.
        .with_skill_id("")
        .with_tool("ls", json!({}))
        .with_created_at("2026-04-16T14:30:00Z");
    let json = ctx.to_json();
    assert_eq!(json["skill_id"], "");
    assert!(!json["skill_id"].is_null());
}

// ---------------------------------------------------------------------------
// §3 environment variables — always 4 in envelope mode
// ---------------------------------------------------------------------------

#[test]
fn hook_env_vars_pre_tool_use_sets_required_four() {
    let ctx = HookContext::new()
        .with_event("PreToolUse")
        .with_session("sess-abc")
        .with_skill_id("threshold-calibration")
        .with_tool("scada_write", json!({}));
    let env: HashMap<String, String> = ctx.to_env_vars().into_iter().collect();
    assert_eq!(
        env.get("GRID_SESSION_ID").map(String::as_str),
        Some("sess-abc")
    );
    assert_eq!(
        env.get("GRID_TOOL_NAME").map(String::as_str),
        Some("scada_write")
    );
    assert_eq!(
        env.get("GRID_SKILL_ID").map(String::as_str),
        Some("threshold-calibration")
    );
    assert_eq!(
        env.get("GRID_EVENT").map(String::as_str),
        Some("PreToolUse")
    );
}

#[test]
fn hook_env_vars_stop_scope_sets_tool_name_as_empty_string() {
    // §3 explicitly allows GRID_TOOL_NAME to be "" for Stop envelopes,
    // but it MUST still be present.
    let ctx = HookContext::new()
        .with_event("Stop")
        .with_session("sess-abc")
        .with_skill_id("");
    let env: HashMap<String, String> = ctx.to_env_vars().into_iter().collect();
    assert!(
        env.contains_key("GRID_TOOL_NAME"),
        "Stop must still set GRID_TOOL_NAME"
    );
    assert_eq!(env.get("GRID_TOOL_NAME").map(String::as_str), Some(""));
    assert_eq!(env.get("GRID_EVENT").map(String::as_str), Some("Stop"));
    assert_eq!(env.get("GRID_SKILL_ID").map(String::as_str), Some(""));
}

// ---------------------------------------------------------------------------
// Legacy backwards-compatibility
// ---------------------------------------------------------------------------

#[test]
fn legacy_hook_context_without_event_still_uses_full_struct_projection() {
    // Pre-ADR callers (Phase AH / declarative hooks) did not set `event`.
    // They rely on `to_json` to emit the full struct. The envelope path
    // must stay opt-in.
    let ctx = HookContext::new()
        .with_session("s1")
        .with_tool("bash", json!({"command": "ls"}));
    let json = ctx.to_json();
    // Full-struct projection exposes tool_input (NOT tool_args), because
    // canonical envelope rename only applies inside envelope mode.
    assert_eq!(json["tool_input"]["command"], "ls");
    assert!(
        json.get("event").is_none(),
        "legacy path MUST NOT invent event"
    );
    assert!(
        json.get("tool_args").is_none(),
        "legacy path MUST NOT rename tool_input"
    );
}

#[test]
fn legacy_to_env_vars_without_event_matches_phase_ah_behavior() {
    // Pre-ADR callers saw "only emit what's set" for GRID_SESSION_ID /
    // GRID_TOOL_NAME. Envelope-required GRID_SKILL_ID / GRID_EVENT are
    // NOT emitted when `event` is None so that existing hooks bound to
    // Phase AH remain byte-identical until they opt in.
    let ctx = HookContext::new().with_session("s1");
    let env: HashMap<String, String> = ctx.to_env_vars().into_iter().collect();
    assert_eq!(env.get("GRID_SESSION_ID").map(String::as_str), Some("s1"));
    assert!(
        !env.contains_key("GRID_EVENT"),
        "legacy path MUST NOT emit GRID_EVENT"
    );
    assert!(
        !env.contains_key("GRID_SKILL_ID"),
        "legacy path MUST NOT emit GRID_SKILL_ID"
    );
}

// ---------------------------------------------------------------------------
// created_at format — must match Python's strftime("%Y-%m-%dT%H:%M:%SZ")
// ---------------------------------------------------------------------------

#[test]
fn hook_envelope_created_at_is_iso8601_zulu_second_precision() {
    let ctx = HookContext::new()
        .with_event("PreToolUse")
        .with_session("s1")
        .with_skill_id("")
        .with_tool("t", json!({}));
    let json = ctx.to_json();
    let ts = json["created_at"]
        .as_str()
        .expect("created_at must be string");
    assert_eq!(ts.len(), 20, "expected 20-char ISO-8601 Zulu, got {ts:?}");
    assert!(ts.ends_with('Z'), "must end with Z: {ts}");
    // Shape: YYYY-MM-DDTHH:MM:SSZ — parse back with chrono to prove it's
    // a valid UTC timestamp (paranoia; blocks accidental locale drift).
    let parsed =
        chrono::NaiveDateTime::parse_from_str(ts.trim_end_matches('Z'), "%Y-%m-%dT%H:%M:%S");
    assert!(
        parsed.is_ok(),
        "failed to parse created_at {ts}: {parsed:?}"
    );
}
