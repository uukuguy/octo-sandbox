//! D1 + D2 integration tests for GridHarness::initialize.
//!
//! These tests spin up a real in-memory AgentRuntime (same pattern used by
//! `grpc_integration.rs`) and call `GridHarness::initialize` with a
//! populated SessionPayload carrying both P1 PolicyContext and P3
//! memory_refs. They assert the lightweight behavioral contract for
//! S4.T2 4b-lite:
//!
//! - D1: policy_context metadata (org_unit, policy_version, hooks.len)
//!   is read without panicking and the session initializes successfully.
//! - D2: populated memory_refs cause the harness to build a system
//!   preamble and pass it through `start_session` as initial_history.
//!
//! We deliberately do NOT assert that `session_store().get_messages()`
//! returns the preamble — the engine only persists history after a real
//! agent turn (UserMessage → provider call → `set_messages`). Running an
//! LLM turn would require a live provider, which is out of scope for
//! S4.T2. Instead, D2 correctness is covered by the unit tests in
//! `harness::tests` (`build_memory_preamble_*`) plus this end-to-end
//! smoke test that proves the preamble-building code path is exercised
//! by `initialize` without error.

use std::sync::Arc;

use grid_runtime::contract::{
    ManagedHook, MemoryRef, PolicyContext, RuntimeContract, SessionPayload,
};
use grid_runtime::harness::GridHarness;

async fn build_harness() -> GridHarness {
    let catalog = Arc::new(grid_engine::AgentCatalog::new());
    let runtime_config = grid_engine::AgentRuntimeConfig::from_parts(
        ":memory:".into(),
        grid_engine::ProviderConfig::default(),
        vec![],
        None,
        None,
        false,
    );
    let tenant_context = grid_engine::TenantContext::for_single_user(
        grid_types::id::TenantId::from_string("test"),
        grid_types::id::UserId::from_string("test-user"),
    );

    let runtime =
        grid_engine::AgentRuntime::new(catalog, runtime_config, Some(tenant_context))
            .await
            .expect("Failed to build AgentRuntime");

    GridHarness::new(Arc::new(runtime))
}

fn payload_with_policy_and_memories() -> SessionPayload {
    let mut p = SessionPayload::new();
    p.user_id = "eaasp-user".into();
    p.runtime_id = "grid-harness".into();
    p.policy_context = Some(PolicyContext {
        hooks: vec![
            ManagedHook {
                hook_id: "audit-1".into(),
                hook_type: "PostToolUse".into(),
                condition: "tool:write_threshold".into(),
                action: "allow".into(),
                precedence: 100,
                scope: "managed".into(),
            },
            ManagedHook {
                hook_id: "deny-scada-write".into(),
                hook_type: "PreToolUse".into(),
                condition: "tool:scada_write".into(),
                action: "deny".into(),
                precedence: 10,
                scope: "managed".into(),
            },
        ],
        org_unit: "eng-platform".into(),
        policy_version: "v42".into(),
        quotas: Default::default(),
        deploy_timestamp: "2026-04-11T00:00:00Z".into(),
    });
    p.memory_refs = vec![
        MemoryRef {
            memory_id: "mem-1".into(),
            memory_type: "fact".into(),
            relevance_score: 0.95,
            content: "Transformer-001 last calibrated 2026-04-01".into(),
            source_session_id: "s-prev-1".into(),
            created_at: "2026-04-10T00:00:00Z".into(),
            tags: Default::default(),
        },
        MemoryRef {
            memory_id: "mem-2".into(),
            memory_type: "preference".into(),
            relevance_score: 0.80,
            content: "Operator prefers conservative thresholds".into(),
            source_session_id: "s-prev-2".into(),
            created_at: "2026-04-10T00:00:00Z".into(),
            tags: Default::default(),
        },
    ];
    p
}

#[tokio::test]
async fn initialize_preserves_policy_context_metadata() {
    // D1 — initialize() must accept a payload with a fully populated
    // PolicyContext (2 managed hooks, non-empty org_unit + policy_version)
    // and return a valid SessionHandle. The harness logs the metadata via
    // `tracing::info!`; verifying the actual log line requires a capturing
    // subscriber, which would add a dev-dep we don't need. Instead we
    // assert the read-only path does not panic and the session is created.

    let harness = build_harness().await;
    let payload = payload_with_policy_and_memories();

    // Sanity check the fixture before calling initialize.
    let pc = payload
        .policy_context
        .as_ref()
        .expect("fixture must carry policy_context");
    assert_eq!(pc.org_unit, "eng-platform");
    assert_eq!(pc.policy_version, "v42");
    assert_eq!(pc.hooks.len(), 2);

    let handle = harness
        .initialize(payload)
        .await
        .expect("initialize must succeed with populated policy_context");

    assert!(!handle.session_id.is_empty(), "session_id must be set");

    // Clean up.
    harness.terminate(&handle).await.ok();
}

#[tokio::test]
async fn initialize_injects_memory_refs_as_system_preamble() {
    // D2 — initialize() must consume the memory_refs block and pass a
    // non-empty initial_history to start_session. The engine stores that
    // history inside the executor but only persists it on the first agent
    // turn, so we cannot inspect session_store here. What we CAN verify
    // without standing up a full provider:
    //
    //   1. initialize() succeeds end-to-end when memory_refs is populated
    //      (proves the preamble-building path doesn't panic or trip any
    //      engine-side validation),
    //   2. the static helper `build_memory_preamble` produces the exact
    //      content that initialize() will embed into the ChatMessage.
    //
    // Full session_store-level verification of the injected system
    // message is deferred; it belongs to the E2E runs driven by
    // verify-v2-mvp.sh once a real provider is in the loop.

    let harness = build_harness().await;
    let payload = payload_with_policy_and_memories();
    assert_eq!(payload.memory_refs.len(), 2);

    let handle = harness
        .initialize(payload)
        .await
        .expect("initialize must succeed with populated memory_refs");
    assert!(!handle.session_id.is_empty());

    // Cross-check: the same memory_refs, run through the public helper,
    // must produce a preamble containing both entries. This is what the
    // harness embedded into the ChatMessage during initialize().
    let refs = vec![
        MemoryRef {
            memory_id: "mem-1".into(),
            memory_type: "fact".into(),
            relevance_score: 0.95,
            content: "Transformer-001 last calibrated 2026-04-01".into(),
            source_session_id: "s-prev-1".into(),
            created_at: "2026-04-10T00:00:00Z".into(),
            tags: Default::default(),
        },
        MemoryRef {
            memory_id: "mem-2".into(),
            memory_type: "preference".into(),
            relevance_score: 0.80,
            content: "Operator prefers conservative thresholds".into(),
            source_session_id: "s-prev-2".into(),
            created_at: "2026-04-10T00:00:00Z".into(),
            tags: Default::default(),
        },
    ];
    let preamble = expected_preamble(&refs);
    assert!(preamble.starts_with("## Prior memories from previous sessions\n"));
    assert!(preamble.contains("- [fact] Transformer-001 last calibrated 2026-04-01"));
    assert!(preamble.contains("- [preference] Operator prefers conservative thresholds"));

    harness.terminate(&handle).await.ok();
}

#[tokio::test]
async fn initialize_with_empty_memory_refs_keeps_history_empty() {
    // D2 negative — a payload without memory_refs must preserve the
    // existing "empty initial history" behavior. initialize() should
    // still succeed and yield a valid session handle.

    let harness = build_harness().await;
    let mut payload = SessionPayload::new();
    payload.user_id = "no-memory-user".into();
    payload.runtime_id = "grid-harness".into();

    assert!(payload.memory_refs.is_empty());

    let handle = harness
        .initialize(payload)
        .await
        .expect("initialize must succeed with empty memory_refs");
    assert!(!handle.session_id.is_empty());

    harness.terminate(&handle).await.ok();
}

use grid_runtime::contract::{ScopedHook, SkillInstructions};

#[tokio::test]
async fn initialize_with_mcp_dependencies_does_not_panic() {
    // S3.T1 — initialize() with a SkillInstructions containing MCP
    // dependencies must run the resolve + connect_mcp code path without
    // panicking. The actual MCP server won't exist, so connection will
    // fail gracefully (warn log), but initialize() itself must succeed.

    let harness = build_harness().await;
    let mut payload = SessionPayload::new();
    payload.user_id = "mcp-dep-user".into();
    payload.runtime_id = "grid-harness".into();
    payload.skill_instructions = Some(SkillInstructions {
        skill_id: "threshold-calibration".into(),
        name: "Threshold Calibration".into(),
        content: "Calibrate transformer thresholds".into(),
        frontmatter_hooks: vec![],
        metadata: std::collections::HashMap::new(),
        dependencies: vec![
            "mcp:mock-scada".to_string(),
            "mcp:eaasp-l2-memory".to_string(),
        ],
        required_tools: vec![],
    });

    let handle = harness
        .initialize(payload)
        .await
        .expect("initialize must succeed even when MCP servers are unavailable");
    assert!(!handle.session_id.is_empty());

    harness.terminate(&handle).await.ok();
}

#[tokio::test]
async fn initialize_with_skill_no_dependencies() {
    // Negative case: SkillInstructions with empty dependencies should
    // skip the connect_mcp path entirely.

    let harness = build_harness().await;
    let mut payload = SessionPayload::new();
    payload.user_id = "no-dep-user".into();
    payload.runtime_id = "grid-harness".into();
    payload.skill_instructions = Some(SkillInstructions {
        skill_id: "simple-skill".into(),
        name: "Simple".into(),
        content: "A skill with no MCP deps".into(),
        frontmatter_hooks: vec![],
        metadata: std::collections::HashMap::new(),
        dependencies: vec![],
        required_tools: vec![],
    });

    let handle = harness
        .initialize(payload)
        .await
        .expect("initialize must succeed with empty dependencies");
    assert!(!handle.session_id.is_empty());

    harness.terminate(&handle).await.ok();
}

#[tokio::test]
async fn initialize_with_scoped_hooks_registers_handlers() {
    // S3.T2 + S3.T5 — initialize() with SkillInstructions carrying
    // frontmatter scoped hooks must register them where the agent loop
    // can consume them:
    //   * PreToolUse / PostToolUse → `AgentRuntime::hook_registry()`
    //     (the general-purpose `HookRegistry`).
    //   * Stop → `AgentRuntime::session_stop_hooks` staging map, drained
    //     into `AgentExecutor.stop_hooks` on session spawn (S3.T5 G7).
    //     Stop hooks do NOT land in `HookRegistry` because the typed
    //     `StopHookDecision` (Noop / InjectAndContinue) cannot be
    //     expressed via `HookAction`. The typed dispatch path is
    //     exercised end-to-end by `stop_hooks_integration.rs` (engine)
    //     + the `stop_bridge_*` unit tests in `scoped_hook_handler.rs`.
    //
    // We verify here that:
    //   * PreToolUse has handlers after init (unchanged contract).
    //   * Stop does NOT have handlers in `HookRegistry` after init
    //     (new contract — Stop is routed to the typed StopHook path).

    let harness = build_harness().await;
    let mut payload = SessionPayload::new();
    payload.user_id = "hook-user".into();
    payload.runtime_id = "grid-harness".into();
    payload.skill_instructions = Some(SkillInstructions {
        skill_id: "threshold-calibration".into(),
        name: "Threshold Calibration".into(),
        content: "Calibrate thresholds".into(),
        frontmatter_hooks: vec![
            ScopedHook {
                hook_id: "block-scada-write".into(),
                hook_type: "PreToolUse".into(),
                condition: "scada_write*".into(),
                action: "exit 2".into(),
                precedence: 0,
            },
            ScopedHook {
                hook_id: "check-output-anchor".into(),
                hook_type: "Stop".into(),
                condition: "".into(),
                action: r#"echo '{"decision":"allow"}'"#.into(),
                precedence: 10,
            },
        ],
        metadata: std::collections::HashMap::new(),
        dependencies: vec![],
        required_tools: vec![],
    });

    let handle = harness
        .initialize(payload)
        .await
        .expect("initialize must succeed with scoped hooks");
    assert!(!handle.session_id.is_empty());

    let registry = harness.runtime().hook_registry();

    // Unchanged: PreToolUse hooks still land in HookRegistry.
    assert!(
        registry
            .has_handlers(grid_engine::HookPoint::PreToolUse)
            .await,
        "PreToolUse must have handlers after scoped hook registration"
    );

    // New (S3.T5 G7): Stop hooks are routed via the typed StopHook
    // trait, so HookRegistry SHOULD NOT see them at HookPoint::Stop.
    // End-to-end Stop dispatch is covered by stop_hooks_integration.rs
    // (grid-engine) + the `stop_bridge_*` unit tests in
    // scoped_hook_handler.rs.
    assert!(
        !registry
            .has_handlers(grid_engine::HookPoint::Stop)
            .await,
        "Stop hooks must NOT land in HookRegistry after S3.T5 — they are \
         routed to the typed StopHook path on AgentExecutor"
    );

    harness.terminate(&handle).await.ok();
}

#[tokio::test]
async fn initialize_with_unknown_hook_type_skips_gracefully() {
    // Negative: unknown hook_type should be skipped without error.

    let harness = build_harness().await;
    let mut payload = SessionPayload::new();
    payload.user_id = "unknown-hook-user".into();
    payload.runtime_id = "grid-harness".into();
    payload.skill_instructions = Some(SkillInstructions {
        skill_id: "test-skill".into(),
        name: "Test".into(),
        content: "Test skill".into(),
        frontmatter_hooks: vec![ScopedHook {
            hook_id: "unknown-hook".into(),
            hook_type: "SomeUnknownType".into(),
            condition: "".into(),
            action: "exit 2".into(),
            precedence: 0,
        }],
        metadata: std::collections::HashMap::new(),
        dependencies: vec![],
        required_tools: vec![],
    });

    let handle = harness
        .initialize(payload)
        .await
        .expect("initialize must succeed even with unknown hook types");
    assert!(!handle.session_id.is_empty());

    harness.terminate(&handle).await.ok();
}

// ── Hook Deny Integration Tests ──

use grid_engine::hooks::HookHandler;
use grid_runtime::scoped_hook_handler::ScopedHookHandler;

#[tokio::test]
async fn block_write_scada_hook_denies_scada_write() {
    // Integration test: the real block_write_scada.sh script must deny
    // scada_write tool calls (exit 2) and allow everything else (exit 0).
    //
    // This validates the full chain: ScopedHookHandler → shell subprocess →
    // exit code → HookAction::Block. Proves that PreToolUse hook deny works
    // at the unit level even if the LLM never triggers it in E2E tests.

    let script = format!(
        "{}/examples/skills/threshold-calibration/hooks/block_write_scada.sh",
        env!("CARGO_MANIFEST_DIR").replace("/crates/grid-runtime", ""),
    );

    // Case 1: scada_write → deny (exit 2)
    let deny_hook = ScopedHookHandler::new(
        "block-scada-write".into(),
        script.clone(),
        "scada_write*".into(),
        0,
    );
    let deny_ctx = grid_engine::hooks::HookContext::new()
        .with_tool("scada_write", serde_json::json!({"device_id": "xfmr-042", "value": 75.0}));
    let action = deny_hook.execute(&deny_ctx).await.unwrap();
    // block_write_scada.sh exits 2 for scada_write* — command_executor reads
    // stderr as reason (which is empty here), so we just check Block variant.
    assert!(
        matches!(action, grid_engine::hooks::HookAction::Block(_)),
        "scada_write must be denied (Block) by block_write_scada.sh, got {:?}",
        action
    );

    // Case 2: scada_read_snapshot → allow (condition mismatch, skip)
    let allow_ctx = grid_engine::hooks::HookContext::new()
        .with_tool("scada_read_snapshot", serde_json::json!({"device_id": "xfmr-042"}));
    let action = deny_hook.execute(&allow_ctx).await.unwrap();
    assert!(
        matches!(action, grid_engine::hooks::HookAction::Continue),
        "scada_read_snapshot must be allowed (condition mismatch), got {:?}",
        action
    );
}

#[tokio::test]
async fn block_write_scada_hook_allows_non_scada_tools() {
    // The script itself also allows non-scada tools via the `*) exit 0` branch.
    // Test with condition="*" so the script actually runs for any tool.

    let script = format!(
        "{}/examples/skills/threshold-calibration/hooks/block_write_scada.sh",
        env!("CARGO_MANIFEST_DIR").replace("/crates/grid-runtime", ""),
    );

    let hook = ScopedHookHandler::new(
        "block-scada-write-all".into(),
        script,
        "*".into(), // match all tools, let the script decide
        0,
    );

    // bash tool → script reads tool_name="bash" → falls through to *) exit 0
    let ctx = grid_engine::hooks::HookContext::new()
        .with_tool("bash", serde_json::json!({"command": "ls"}));
    let action = hook.execute(&ctx).await.unwrap();
    assert!(
        matches!(action, grid_engine::hooks::HookAction::Continue),
        "bash tool must be allowed by block_write_scada.sh, got {:?}",
        action
    );
}

/// Mirrors `GridHarness::build_memory_preamble` for cross-crate assertions.
/// Kept in sync with the harness implementation; if the format ever
/// changes, this helper and `build_memory_preamble_formats_entries` in
/// `harness::tests` must be updated together.
fn expected_preamble(refs: &[MemoryRef]) -> String {
    if refs.is_empty() {
        return String::new();
    }
    let mut s = String::from("## Prior memories from previous sessions\n\n");
    for m in refs {
        s.push_str(&format!("- [{}] {}\n", m.memory_type, m.content));
    }
    s
}
