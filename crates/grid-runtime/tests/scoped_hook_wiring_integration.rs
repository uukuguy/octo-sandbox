//! S3.T5 integration tests for the scoped-hook executor wiring.
//!
//! Covers two landed implementations that `harness_payload_integration.rs`
//! only exercises at the surface:
//!
//! - **T5.C** — `build_hook_vars` inside `GridHarness::initialize` resolves
//!   `${SKILL_DIR}` by either materialising inline `skill_instructions.content`
//!   to `{workspace}/grid-session-{session_id}/skill/SKILL.md`, or falling
//!   back to `{EAASP_SKILL_CACHE_DIR}/{skill_id}` when that directory exists
//!   on disk. Per-hook substitution errors are fail-open (logged, hook
//!   skipped) so the session always initialises (ADR-V2-006 §7).
//!
//! - **T5.D** — `ScopedStopHookBridge` adapts a bash Stop-scope hook to the
//!   `grid_engine::agent::StopHook` trait expected by the loop's natural
//!   termination boundary. `exit 2` → `InjectAndContinue(system_msg)`,
//!   `exit 0` → `Noop`, subprocess errors → `Noop` (fail-open).
//!
//! These tests deliberately exercise the REAL harness + registry path
//! (not a stub) so they catch integration-level issues that unit tests
//! miss — e.g. the `register_session_stop_hooks` staging/drain dance
//! sitting between `register_scoped_hooks` and the executor spawn.
//!
//! ## Concurrency note
//!
//! Cargo runs integration tests in parallel by default. Tests that mutate
//! process-global `EAASP_RUNTIME_WORKSPACE` / `EAASP_SKILL_CACHE_DIR` take
//! `ENV_MUTEX` around the set-var + initialize call to avoid cross-test
//! pollution. Unit-only tests (Test 3) don't need it.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use grid_engine::agent::stop_hooks::{StopHook, StopHookDecision};
use grid_engine::hooks::{HookContext, HookPoint};
use grid_runtime::contract::{RuntimeContract, ScopedHook, SessionPayload, SkillInstructions};
use grid_runtime::harness::GridHarness;
use grid_runtime::scoped_hook_handler::ScopedStopHookBridge;
use grid_types::MessageRole;
use tempfile::TempDir;

/// Serialises any test that mutates `EAASP_*` env vars. `set_var` is
/// process-global; two parallel tests racing on the same key see each
/// other's values.
static ENV_MUTEX: Mutex<()> = Mutex::new(());

/// Match the helper used in `harness_payload_integration.rs` so test
/// setup costs are identical (in-memory SQLite AgentRuntime).
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

/// Build a minimal payload with a single scoped hook.
///
/// `skill_content` drives the `build_hook_vars` resolution branch:
/// - non-empty + non-empty `hooks` → inline materialization
/// - empty + `EAASP_SKILL_CACHE_DIR` set → cache-dir fallback
/// - empty + no cache → `skill_dir=None` (unresolved)
fn payload_with_hook(skill_id: &str, skill_content: &str, hooks: Vec<ScopedHook>) -> SessionPayload {
    let mut p = SessionPayload::new();
    p.user_id = "scoped-hook-user".into();
    p.runtime_id = "grid-harness".into();
    p.skill_instructions = Some(SkillInstructions {
        skill_id: skill_id.into(),
        name: "Test Skill".into(),
        content: skill_content.into(),
        frontmatter_hooks: hooks,
        metadata: Default::default(),
        dependencies: vec![],
        required_tools: vec![],
    });
    p
}

// ─────────────────────────────────────────────────────────────────────────────
// Test 1 — Inline skill content materialises to {workspace}/skill/SKILL.md
// ─────────────────────────────────────────────────────────────────────────────

/// When `skill_instructions.content` is non-empty AND at least one scoped
/// hook is declared, `build_hook_vars` must write `SKILL.md` to
/// `{EAASP_RUNTIME_WORKSPACE}/grid-session-{session_id}/skill/SKILL.md`
/// so hooks referencing `${SKILL_DIR}` resolve to a real on-disk path.
///
/// We assert the file exists with the exact content — proving the
/// materialisation side-effect survives even though we can't inspect
/// `hook_vars.skill_dir` directly (it is consumed inside `initialize`).
#[tokio::test]
async fn inline_skill_content_materializes_skill_dir() {
    let _guard = ENV_MUTEX.lock().unwrap_or_else(|e| e.into_inner());

    let workspace = TempDir::new().expect("create workspace tempdir");
    std::env::set_var("EAASP_RUNTIME_WORKSPACE", workspace.path());
    std::env::remove_var("EAASP_SKILL_CACHE_DIR");

    let content = "# Test skill prose\n";
    let payload = payload_with_hook(
        "skill-materialize-test",
        content,
        vec![ScopedHook {
            hook_id: "h1".into(),
            // `register_scoped_hooks` reads `condition` first (scope) and
            // falls back to `hook_type`. Leaving `condition` empty routes
            // this hook at `PreToolUse` via `hook_type`.
            hook_type: "PreToolUse".into(),
            condition: "".into(),
            // No `${...}` so substitution always succeeds — the hook's
            // execution path is irrelevant for this test.
            action: "/bin/true".into(),
            precedence: 0,
        }],
    );

    let harness = build_harness().await;
    let handle = harness
        .initialize(payload)
        .await
        .expect("initialize must succeed with inline skill content");

    // The materialised path mirrors `build_hook_vars`:
    //   {workspace}/grid-session-{session_id}/skill/SKILL.md
    let skill_md = workspace
        .path()
        .join(format!("grid-session-{}", handle.session_id))
        .join("skill")
        .join("SKILL.md");

    assert!(
        skill_md.exists(),
        "SKILL.md must be materialised at {}",
        skill_md.display()
    );
    let on_disk = std::fs::read_to_string(&skill_md).expect("read materialised SKILL.md");
    assert_eq!(on_disk, content, "materialised content must match payload");

    harness.terminate(&handle).await.ok();
    std::env::remove_var("EAASP_RUNTIME_WORKSPACE");
}

// ─────────────────────────────────────────────────────────────────────────────
// Test 2 — Cache-dir fallback when skill content is empty
// ─────────────────────────────────────────────────────────────────────────────

/// When `skill_instructions.content` is empty but `EAASP_SKILL_CACHE_DIR`
/// is set AND `{cache}/{skill_id}` exists on disk, `build_hook_vars` adopts
/// that as `skill_dir`. Inline materialisation must NOT happen (we assert
/// the would-be `{workspace}/skill/SKILL.md` is absent).
///
/// We rely on the hook having no `${...}` reference, so substitution
/// succeeds regardless of `skill_dir` — the real assertion is that
/// `initialize` succeeds and DOES NOT materialise anything.
#[tokio::test]
async fn cache_dir_resolution_when_no_inline_content() {
    let _guard = ENV_MUTEX.lock().unwrap_or_else(|e| e.into_inner());

    let cache_root = TempDir::new().expect("create cache tempdir");
    let workspace = TempDir::new().expect("create workspace tempdir");
    let skill_id = "skill-cache-test";
    // `build_hook_vars` only adopts the cache dir if `{cache}/{skill_id}`
    // actually exists on disk — create it.
    std::fs::create_dir_all(cache_root.path().join(skill_id)).expect("mkdir cache subdir");

    std::env::set_var("EAASP_SKILL_CACHE_DIR", cache_root.path());
    std::env::set_var("EAASP_RUNTIME_WORKSPACE", workspace.path());

    let payload = payload_with_hook(
        skill_id,
        "", // empty → skip inline materialisation branch
        vec![ScopedHook {
            hook_id: "cache-hook".into(),
            hook_type: "PreToolUse".into(),
            condition: "".into(),
            action: "/bin/true".into(),
            precedence: 0,
        }],
    );

    let harness = build_harness().await;
    let handle = harness
        .initialize(payload)
        .await
        .expect("initialize must succeed with cache-dir skill");

    // Inline branch MUST have been skipped — no SKILL.md under workspace.
    let would_be_skill_md = workspace
        .path()
        .join(format!("grid-session-{}", handle.session_id))
        .join("skill")
        .join("SKILL.md");
    assert!(
        !would_be_skill_md.exists(),
        "inline materialisation must NOT run when content is empty; \
         unexpected file at {}",
        would_be_skill_md.display()
    );

    // Session lifecycle must be clean.
    harness.terminate(&handle).await.ok();

    std::env::remove_var("EAASP_SKILL_CACHE_DIR");
    std::env::remove_var("EAASP_RUNTIME_WORKSPACE");
}

// ─────────────────────────────────────────────────────────────────────────────
// Test 3 — ScopedStopHookBridge behaviour at the StopHook trait boundary
// ─────────────────────────────────────────────────────────────────────────────

/// Directly exercise `ScopedStopHookBridge` — the adapter that bridges
/// bash Stop-scope hooks onto the `StopHook` trait the agent loop consults.
///
/// Three sub-cases cover the three decision paths through
/// `execute_command`:
///
/// 1. **Deny path (exit 2 from bash one-liner)**: the bridge must return
///    `StopHookDecision::InjectAndContinue` carrying exactly one System
///    `ChatMessage` whose text surfaces the stderr reason to the LLM on
///    the re-entered round.
/// 2. **Allow path (exit 0)**: bridge returns `Noop`, letting termination
///    proceed.
/// 3. **Real-script path**: the threshold-calibration `check_output_anchor.sh`
///    (already shipped at `examples/skills/threshold-calibration/hooks/`)
///    must deny when the envelope lacks `evidence_anchor_id`. This pulls
///    the contract test into the real skill fixture used by end-to-end
///    runs so any future script drift is caught here.
///
/// The bridge is constructed directly (no `register_scoped_hooks` detour)
/// so the assertion is about the trait behaviour, not the registration
/// plumbing covered by Tests 1/2/4.
#[tokio::test]
async fn stop_scope_hook_registers_bridge() {
    // ── Sub-case A: deny via bash one-liner ────────────────────────────
    let bridge = ScopedStopHookBridge::new(
        "t3-deny".into(),
        "echo 'anchor missing' >&2; exit 2".into(),
    );
    let ctx = HookContext::new().with_session("t3-session");
    let decision = bridge
        .execute(&ctx)
        .await
        .expect("bridge must not surface an Err on exit-2 (deny is a value, not an error)");
    match decision {
        StopHookDecision::InjectAndContinue(msgs) => {
            assert_eq!(msgs.len(), 1, "exactly one system message must be injected");
            assert!(
                matches!(msgs[0].role, MessageRole::System),
                "injected message must be System role, got {:?}",
                msgs[0].role
            );
            let text = msgs[0].text_content();
            assert!(
                text.contains("anchor missing"),
                "injected text must surface stderr reason, got {:?}",
                text
            );
        }
        StopHookDecision::Noop => panic!("expected InjectAndContinue for exit 2, got Noop"),
    }

    // ── Sub-case B: allow via exit 0 ───────────────────────────────────
    let allow_bridge = ScopedStopHookBridge::new("t3-allow".into(), "exit 0".into());
    let decision = allow_bridge
        .execute(&ctx)
        .await
        .expect("bridge must not error on exit 0");
    assert!(
        matches!(decision, StopHookDecision::Noop),
        "exit 0 must map to Noop, got {:?}",
        decision
    );

    // ── Sub-case C: real check_output_anchor.sh rejects missing anchor ──
    // Walk up from `crates/grid-runtime` to the repo root and locate the
    // shipped skill's hook script. Matches the `CARGO_MANIFEST_DIR`
    // convention used by `block_write_scada_hook_denies_scada_write`.
    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR").replace("/crates/grid-runtime", ""));
    let script = repo_root
        .join("examples/skills/threshold-calibration/hooks/check_output_anchor.sh")
        .to_string_lossy()
        .into_owned();
    let real_bridge = ScopedStopHookBridge::new("t3-real".into(), script);
    // Context without `output.evidence_anchor_id` → script exits 2.
    let decision = real_bridge
        .execute(&ctx)
        .await
        .expect("real script must not surface an Err on its deny path");
    assert!(
        matches!(decision, StopHookDecision::InjectAndContinue(_)),
        "check_output_anchor.sh must deny when evidence_anchor_id is absent; got {:?}",
        decision
    );
    assert_eq!(real_bridge.name(), "t3-real");
}

// ─────────────────────────────────────────────────────────────────────────────
// Test 4 — Unresolved ${SKILL_DIR} → hook skipped, session still succeeds
// ─────────────────────────────────────────────────────────────────────────────

/// When a hook references `${SKILL_DIR}` but `skill_dir` cannot be
/// resolved (empty content AND no matching cache dir), per-hook
/// substitution returns `Unbound`. `register_scoped_hooks` must:
/// a) log at ERROR, b) skip the broken hook, c) still register any sibling
/// hook that substitutes cleanly, d) never fail `initialize`.
///
/// Assertion strategy — PreToolUse always carries at least one builtin
/// (`SecurityPolicyHandler`), so `has_handlers` is useless here. We diff
/// `handler_count` before / after `initialize` on a harness with two
/// payloads: one with only the broken hook, and one with a broken + good
/// hook pair. The delta between the two proves only the good hook made
/// it in.
#[tokio::test]
async fn unresolved_skill_dir_skips_hook_with_log() {
    let _guard = ENV_MUTEX.lock().unwrap_or_else(|e| e.into_inner());

    // Neither env var set → skill_dir = None, session_dir defaults to a
    // per-test tempdir via fallback (std::env::temp_dir).
    std::env::remove_var("EAASP_SKILL_CACHE_DIR");
    let workspace = TempDir::new().expect("create workspace tempdir");
    std::env::set_var("EAASP_RUNTIME_WORKSPACE", workspace.path());

    let broken_hook = ScopedHook {
        hook_id: "broken-skilldir".into(),
        hook_type: "PreToolUse".into(),
        condition: "".into(),
        // ${SKILL_DIR} is unresolved because content is empty AND cache
        // env is unset → substitute_hook_vars returns Unbound → skip.
        action: "${SKILL_DIR}/never-runs.sh".into(),
        precedence: 0,
    };
    let good_hook = ScopedHook {
        hook_id: "good-no-vars".into(),
        hook_type: "PreToolUse".into(),
        condition: "".into(),
        action: "/bin/true".into(),
        precedence: 0,
    };

    // Baseline: only the broken hook. handler_count at PreToolUse should
    // equal `builtin_count` (whatever the engine auto-registers). We don't
    // hard-code the number — we just diff against the "good" payload.
    let harness_a = build_harness().await;
    let handle_a = harness_a
        .initialize(payload_with_hook(
            "unresolved-a",
            "",
            vec![broken_hook.clone()],
        ))
        .await
        .expect("initialize must succeed even when a hook is skipped");
    let baseline_count = harness_a
        .runtime()
        .hook_registry()
        .handler_count(HookPoint::PreToolUse)
        .await;
    harness_a.terminate(&handle_a).await.ok();

    // With the good hook alongside the broken one, PreToolUse should gain
    // EXACTLY one handler over baseline — the broken hook is skipped.
    let harness_b = build_harness().await;
    let handle_b = harness_b
        .initialize(payload_with_hook(
            "unresolved-b",
            "",
            vec![broken_hook, good_hook],
        ))
        .await
        .expect("initialize must succeed with a broken + good hook pair");
    let with_good_count = harness_b
        .runtime()
        .hook_registry()
        .handler_count(HookPoint::PreToolUse)
        .await;
    harness_b.terminate(&handle_b).await.ok();

    assert_eq!(
        with_good_count,
        baseline_count + 1,
        "adding a substitutable hook must add exactly one handler \
         (broken hook must be silently skipped); baseline={}, with_good={}",
        baseline_count,
        with_good_count
    );

    std::env::remove_var("EAASP_RUNTIME_WORKSPACE");
}
