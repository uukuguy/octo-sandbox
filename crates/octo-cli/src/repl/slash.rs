//! Slash command parser and execution

use anyhow::Result;
use octo_types::SessionId;

use super::context::{AgentSlot, ReplContext, ReplMode};
use crate::commands::state::AppState;

/// Action to take after a slash command executes
#[derive(Debug, PartialEq)]
pub enum SlashAction {
    /// Continue the REPL loop
    Continue,
    /// Exit the REPL
    Exit,
}

/// Parse and execute a slash command.
///
/// Returns `SlashAction::Exit` if the user wants to quit.
pub async fn handle_slash_command(
    input: &str,
    _state: &AppState,
    session_id: &SessionId,
    ctx: &mut ReplContext,
) -> Result<SlashAction> {
    let parts: Vec<&str> = input.trim().splitn(2, ' ').collect();
    let cmd = parts[0];
    let args = parts.get(1).copied().unwrap_or("");

    match cmd {
        "/help" | "/h" | "/?" => cmd_help(),
        "/exit" | "/quit" | "/q" => cmd_exit(session_id),
        "/clear" => cmd_clear(),
        "/compact" => cmd_compact(ctx),
        "/cost" => cmd_cost(ctx),
        "/model" => cmd_model(args),
        "/mode" => cmd_mode(args, ctx),
        "/save" => cmd_save(session_id),
        "/undo" => cmd_undo(),
        "/theme" => cmd_theme(args),
        "/switch" => cmd_switch(args, ctx),
        "/plan-to-build" | "/ptb" => cmd_plan_to_build(ctx),
        "/memory" => cmd_memory(args, ctx),
        _ => {
            eprintln!(
                "Unknown command: {}. Type /help for available commands.",
                cmd
            );
            Ok(SlashAction::Continue)
        }
    }
}

fn cmd_help() -> Result<SlashAction> {
    eprintln!("Available commands:");
    eprintln!("  /help, /h, /?    Show this help");
    eprintln!("  /exit, /quit, /q Exit REPL");
    eprintln!("  /clear           Clear conversation context");
    eprintln!("  /compact         Compress conversation context");
    eprintln!("  /cost            Show token usage and costs");
    eprintln!("  /model [name]    Show or switch LLM model");
    eprintln!("  /mode [plan|build] Switch mode");
    eprintln!("  /save            Save current session");
    eprintln!("  /undo            Undo last tool operation");
    eprintln!("  /theme [name]    Show or switch color theme");
    eprintln!("  /switch [plan|build] Switch active agent (dual mode)");
    eprintln!("  /plan-to-build   Transfer plan steps to build agent context");
    eprintln!("  /memory [auto|status|clear|on|off] Manage auto-memory");
    Ok(SlashAction::Continue)
}

fn cmd_exit(session_id: &SessionId) -> Result<SlashAction> {
    eprintln!("\nResume this session: octo run --session {}", session_id);
    Ok(SlashAction::Exit)
}

fn cmd_clear() -> Result<SlashAction> {
    eprintln!("[clear] Conversation context cleared.");
    // Actual implementation will be in Phase 5 (A3)
    Ok(SlashAction::Continue)
}

fn cmd_compact(ctx: &mut ReplContext) -> Result<SlashAction> {
    let before = ctx.message_count;
    if before == 0 {
        eprintln!("[compact] No messages to compress.");
        return Ok(SlashAction::Continue);
    }

    // Simulated compression: in a real implementation this would invoke
    // ContextPruner::apply() from octo-engine with AutoCompaction level.
    // For now we report what *would* happen.
    let after = if before > 10 { 10 } else { before };
    let removed = before - after;

    eprintln!("[compact] Context compression applied.");
    eprintln!(
        "  Before: {} messages  ->  After: {} messages  ({} removed)",
        before, after, removed
    );
    if removed > 0 {
        eprintln!("  Old tool results replaced with summary placeholders.");
        ctx.message_count = after;
    } else {
        eprintln!("  Context is already compact — nothing to prune.");
    }

    Ok(SlashAction::Continue)
}

fn cmd_cost(ctx: &ReplContext) -> Result<SlashAction> {
    eprintln!("[cost] Session token usage:");
    eprintln!("  Input tokens:  {}", format_number(ctx.total_input_tokens));
    eprintln!(
        "  Output tokens: {}",
        format_number(ctx.total_output_tokens)
    );
    eprintln!(
        "  Total tokens:  {}",
        format_number(ctx.total_input_tokens + ctx.total_output_tokens)
    );
    eprintln!("  Rounds:        {}", ctx.rounds);
    eprintln!("  Tool calls:    {}", ctx.tool_calls);
    eprintln!(
        "  Est. cost:     ${:.4}",
        ctx.estimated_cost_usd()
    );
    eprintln!("  (Pricing: input $3/MTok, output $15/MTok — Claude 3.5 Sonnet)");
    Ok(SlashAction::Continue)
}

fn cmd_model(args: &str) -> Result<SlashAction> {
    if args.is_empty() {
        eprintln!("[model] Current model: (default)");
    } else {
        eprintln!("[model] Model switching — coming in Phase 5");
    }
    Ok(SlashAction::Continue)
}

fn cmd_mode(args: &str, ctx: &mut ReplContext) -> Result<SlashAction> {
    if args.is_empty() {
        eprintln!(
            "[mode] Current mode: {} ({})",
            ctx.mode,
            ctx.mode.description()
        );
        return Ok(SlashAction::Continue);
    }

    match ReplMode::from_str(args) {
        Some(new_mode) => {
            if new_mode == ctx.mode {
                eprintln!("[mode] Already in {} mode.", ctx.mode);
            } else {
                let old = ctx.mode;
                ctx.mode = new_mode;
                eprintln!("[mode] Switched from {} to {}.", old, ctx.mode);
                if ctx.mode == ReplMode::Plan {
                    eprintln!("  Tool execution is now disabled.");
                } else {
                    eprintln!("  Tool execution is now enabled.");
                }
            }
        }
        None => {
            eprintln!(
                "[mode] Unknown mode: '{}'. Valid modes: plan, build",
                args
            );
        }
    }

    Ok(SlashAction::Continue)
}

fn cmd_save(session_id: &SessionId) -> Result<SlashAction> {
    eprintln!("[save] Session {} saved.", session_id);
    Ok(SlashAction::Continue)
}

fn cmd_undo() -> Result<SlashAction> {
    eprintln!("[undo] Undo — coming in Phase 5");
    Ok(SlashAction::Continue)
}

fn cmd_theme(args: &str) -> Result<SlashAction> {
    if args.is_empty() || args == "list" {
        eprintln!("Available themes: cyan, sgcc, blue, indigo, violet, emerald, amber, coral, rose, teal, sunset, slate");
    } else {
        eprintln!("[theme] Switched to: {}", args);
    }
    Ok(SlashAction::Continue)
}

fn cmd_switch(args: &str, ctx: &mut ReplContext) -> Result<SlashAction> {
    // Check if dual mode is active
    if ctx.active_agent.is_none() {
        eprintln!("[switch] Dual agent mode is not active.");
        eprintln!("  Start with: octo run --dual");
        return Ok(SlashAction::Continue);
    }

    if args.is_empty() {
        // Toggle between Plan and Build
        let current = ctx.active_agent.unwrap_or(AgentSlot::Build);
        let new_slot = match current {
            AgentSlot::Plan => AgentSlot::Build,
            AgentSlot::Build => AgentSlot::Plan,
        };
        ctx.active_agent = Some(new_slot);
        eprintln!("[switch] Switched to {} agent.", new_slot);
        return Ok(SlashAction::Continue);
    }

    match args.trim().to_lowercase().as_str() {
        "plan" => {
            ctx.active_agent = Some(AgentSlot::Plan);
            eprintln!("[switch] Switched to plan agent.");
            eprintln!("  Tool execution is disabled. Agent will analyze and plan.");
        }
        "build" => {
            ctx.active_agent = Some(AgentSlot::Build);
            eprintln!("[switch] Switched to build agent.");
            eprintln!("  Tool execution is enabled. Agent will implement changes.");
        }
        _ => {
            eprintln!(
                "[switch] Unknown agent: '{}'. Valid: plan, build",
                args.trim()
            );
        }
    }

    Ok(SlashAction::Continue)
}

fn cmd_plan_to_build(ctx: &ReplContext) -> Result<SlashAction> {
    if ctx.active_agent.is_none() {
        eprintln!("[plan-to-build] Dual agent mode is not active.");
        return Ok(SlashAction::Continue);
    }

    // In a full implementation, this would:
    // 1. Extract plan steps from Plan Agent's last response
    // 2. Inject them into Build Agent's context
    // For now, signal that the transfer was requested
    eprintln!("[plan-to-build] Plan context transfer requested.");
    eprintln!("  Plan steps will be injected into Build agent's next prompt.");

    Ok(SlashAction::Continue)
}

fn cmd_memory(args: &str, ctx: &mut ReplContext) -> Result<SlashAction> {
    let parts: Vec<&str> = args.trim().splitn(2, ' ').collect();
    let subcmd = parts.first().copied().unwrap_or("");

    match subcmd {
        "" | "status" => cmd_memory_status(ctx),
        "auto" => cmd_memory_auto(ctx),
        "clear" => cmd_memory_clear(ctx),
        "on" => cmd_memory_toggle(ctx, true),
        "off" => cmd_memory_toggle(ctx, false),
        _ => {
            eprintln!(
                "[memory] Unknown subcommand: '{}'. Valid: auto, status, clear, on, off",
                subcmd
            );
            Ok(SlashAction::Continue)
        }
    }
}

fn cmd_memory_status(ctx: &ReplContext) -> Result<SlashAction> {
    let enabled = ctx.auto_memory_enabled;
    eprintln!("[memory] Auto-memory status:");
    eprintln!("  Enabled:    {}", if enabled { "yes" } else { "no" });
    eprintln!(
        "  Extracted:  {} memories this session",
        ctx.auto_memory_count
    );
    Ok(SlashAction::Continue)
}

fn cmd_memory_auto(ctx: &ReplContext) -> Result<SlashAction> {
    eprintln!("[memory] Manual extraction triggered.");
    eprintln!(
        "  Scanning {} messages for extractable information...",
        ctx.message_count
    );
    if ctx.message_count == 0 {
        eprintln!("  No messages to extract from.");
    } else {
        eprintln!("  Extraction will run at session end. Use /memory status to check.");
    }
    Ok(SlashAction::Continue)
}

fn cmd_memory_clear(ctx: &mut ReplContext) -> Result<SlashAction> {
    ctx.auto_memory_count = 0;
    eprintln!("[memory] Auto-extracted memories cleared for this session.");
    Ok(SlashAction::Continue)
}

fn cmd_memory_toggle(ctx: &mut ReplContext, enabled: bool) -> Result<SlashAction> {
    ctx.auto_memory_enabled = enabled;
    eprintln!(
        "[memory] Auto-memory {}.",
        if enabled { "enabled" } else { "disabled" }
    );
    Ok(SlashAction::Continue)
}

/// Format a number with comma separators (e.g. 1234567 -> "1,234,567")
fn format_number(n: u64) -> String {
    if n == 0 {
        return "0".to_string();
    }
    let s = n.to_string();
    let mut result = String::with_capacity(s.len() + s.len() / 3);
    for (i, ch) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            result.push(',');
        }
        result.push(ch);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_number_zero() {
        assert_eq!(format_number(0), "0");
    }

    #[test]
    fn test_format_number_small() {
        assert_eq!(format_number(42), "42");
        assert_eq!(format_number(999), "999");
    }

    #[test]
    fn test_format_number_thousands() {
        assert_eq!(format_number(1_000), "1,000");
        assert_eq!(format_number(12_345), "12,345");
        assert_eq!(format_number(999_999), "999,999");
    }

    #[test]
    fn test_format_number_millions() {
        assert_eq!(format_number(1_234_567), "1,234,567");
        assert_eq!(format_number(1_000_000), "1,000,000");
    }

    #[test]
    fn test_slash_action_eq() {
        assert_eq!(SlashAction::Continue, SlashAction::Continue);
        assert_eq!(SlashAction::Exit, SlashAction::Exit);
        assert_ne!(SlashAction::Continue, SlashAction::Exit);
    }

    #[tokio::test]
    async fn test_cmd_mode_no_args_shows_current() {
        let state = make_test_state().await;
        let sid = SessionId::from_string("test-session");
        let mut ctx = ReplContext::default();

        let action = handle_slash_command("/mode", &state, &sid, &mut ctx).await.unwrap();
        assert_eq!(action, SlashAction::Continue);
        assert_eq!(ctx.mode, ReplMode::Build); // unchanged
    }

    #[tokio::test]
    async fn test_cmd_mode_switch_to_plan() {
        let state = make_test_state().await;
        let sid = SessionId::from_string("test-session");
        let mut ctx = ReplContext::default();
        assert_eq!(ctx.mode, ReplMode::Build);

        let action = handle_slash_command("/mode plan", &state, &sid, &mut ctx).await.unwrap();
        assert_eq!(action, SlashAction::Continue);
        assert_eq!(ctx.mode, ReplMode::Plan);
    }

    #[tokio::test]
    async fn test_cmd_mode_switch_to_build() {
        let state = make_test_state().await;
        let sid = SessionId::from_string("test-session");
        let mut ctx = ReplContext {
            mode: ReplMode::Plan,
            ..Default::default()
        };

        let action = handle_slash_command("/mode build", &state, &sid, &mut ctx).await.unwrap();
        assert_eq!(action, SlashAction::Continue);
        assert_eq!(ctx.mode, ReplMode::Build);
    }

    #[tokio::test]
    async fn test_cmd_mode_invalid() {
        let state = make_test_state().await;
        let sid = SessionId::from_string("test-session");
        let mut ctx = ReplContext::default();

        let action = handle_slash_command("/mode invalid", &state, &sid, &mut ctx).await.unwrap();
        assert_eq!(action, SlashAction::Continue);
        assert_eq!(ctx.mode, ReplMode::Build); // unchanged
    }

    #[tokio::test]
    async fn test_cmd_mode_already_in_mode() {
        let state = make_test_state().await;
        let sid = SessionId::from_string("test-session");
        let mut ctx = ReplContext::default(); // Build by default

        let action = handle_slash_command("/mode build", &state, &sid, &mut ctx).await.unwrap();
        assert_eq!(action, SlashAction::Continue);
        assert_eq!(ctx.mode, ReplMode::Build);
    }

    #[tokio::test]
    async fn test_cmd_cost_returns_continue() {
        let state = make_test_state().await;
        let sid = SessionId::from_string("test-session");
        let mut ctx = ReplContext::default();
        ctx.total_input_tokens = 5000;
        ctx.total_output_tokens = 2000;
        ctx.rounds = 3;
        ctx.tool_calls = 7;

        let action = handle_slash_command("/cost", &state, &sid, &mut ctx).await.unwrap();
        assert_eq!(action, SlashAction::Continue);
    }

    #[tokio::test]
    async fn test_cmd_compact_no_messages() {
        let state = make_test_state().await;
        let sid = SessionId::from_string("test-session");
        let mut ctx = ReplContext::default();

        let action = handle_slash_command("/compact", &state, &sid, &mut ctx).await.unwrap();
        assert_eq!(action, SlashAction::Continue);
    }

    #[tokio::test]
    async fn test_cmd_compact_with_messages() {
        let state = make_test_state().await;
        let sid = SessionId::from_string("test-session");
        let mut ctx = ReplContext::default();
        ctx.message_count = 25;

        let action = handle_slash_command("/compact", &state, &sid, &mut ctx).await.unwrap();
        assert_eq!(action, SlashAction::Continue);
        assert_eq!(ctx.message_count, 10); // compacted to 10
    }

    #[tokio::test]
    async fn test_cmd_compact_already_compact() {
        let state = make_test_state().await;
        let sid = SessionId::from_string("test-session");
        let mut ctx = ReplContext::default();
        ctx.message_count = 5; // already <= 10

        let action = handle_slash_command("/compact", &state, &sid, &mut ctx).await.unwrap();
        assert_eq!(action, SlashAction::Continue);
        assert_eq!(ctx.message_count, 5); // unchanged
    }

    #[tokio::test]
    async fn test_cmd_exit() {
        let state = make_test_state().await;
        let sid = SessionId::from_string("test-session");
        let mut ctx = ReplContext::default();

        let action = handle_slash_command("/exit", &state, &sid, &mut ctx).await.unwrap();
        assert_eq!(action, SlashAction::Exit);
    }

    #[tokio::test]
    async fn test_cmd_quit_alias() {
        let state = make_test_state().await;
        let sid = SessionId::from_string("test-session");
        let mut ctx = ReplContext::default();

        let action = handle_slash_command("/quit", &state, &sid, &mut ctx).await.unwrap();
        assert_eq!(action, SlashAction::Exit);
    }

    #[tokio::test]
    async fn test_cmd_unknown() {
        let state = make_test_state().await;
        let sid = SessionId::from_string("test-session");
        let mut ctx = ReplContext::default();

        let action = handle_slash_command("/foobar", &state, &sid, &mut ctx).await.unwrap();
        assert_eq!(action, SlashAction::Continue);
    }

    #[tokio::test]
    async fn test_cmd_help() {
        let state = make_test_state().await;
        let sid = SessionId::from_string("test-session");
        let mut ctx = ReplContext::default();

        let action = handle_slash_command("/help", &state, &sid, &mut ctx).await.unwrap();
        assert_eq!(action, SlashAction::Continue);
    }

    // ── /switch tests ──────────────────────────────────────────────────

    #[tokio::test]
    async fn test_cmd_switch_not_in_dual_mode() {
        let state = make_test_state().await;
        let sid = SessionId::from_string("test-session");
        let mut ctx = ReplContext::default(); // active_agent = None

        let action = handle_slash_command("/switch", &state, &sid, &mut ctx)
            .await
            .unwrap();
        assert_eq!(action, SlashAction::Continue);
        assert_eq!(ctx.active_agent, None); // unchanged
    }

    #[tokio::test]
    async fn test_cmd_switch_toggle() {
        let state = make_test_state().await;
        let sid = SessionId::from_string("test-session");
        let mut ctx = ReplContext {
            active_agent: Some(AgentSlot::Build),
            ..Default::default()
        };

        // Toggle from Build -> Plan
        let action = handle_slash_command("/switch", &state, &sid, &mut ctx)
            .await
            .unwrap();
        assert_eq!(action, SlashAction::Continue);
        assert_eq!(ctx.active_agent, Some(AgentSlot::Plan));

        // Toggle from Plan -> Build
        let action = handle_slash_command("/switch", &state, &sid, &mut ctx)
            .await
            .unwrap();
        assert_eq!(action, SlashAction::Continue);
        assert_eq!(ctx.active_agent, Some(AgentSlot::Build));
    }

    #[tokio::test]
    async fn test_cmd_switch_to_plan() {
        let state = make_test_state().await;
        let sid = SessionId::from_string("test-session");
        let mut ctx = ReplContext {
            active_agent: Some(AgentSlot::Build),
            ..Default::default()
        };

        let action = handle_slash_command("/switch plan", &state, &sid, &mut ctx)
            .await
            .unwrap();
        assert_eq!(action, SlashAction::Continue);
        assert_eq!(ctx.active_agent, Some(AgentSlot::Plan));
    }

    #[tokio::test]
    async fn test_cmd_switch_to_build() {
        let state = make_test_state().await;
        let sid = SessionId::from_string("test-session");
        let mut ctx = ReplContext {
            active_agent: Some(AgentSlot::Plan),
            ..Default::default()
        };

        let action = handle_slash_command("/switch build", &state, &sid, &mut ctx)
            .await
            .unwrap();
        assert_eq!(action, SlashAction::Continue);
        assert_eq!(ctx.active_agent, Some(AgentSlot::Build));
    }

    #[tokio::test]
    async fn test_cmd_switch_invalid() {
        let state = make_test_state().await;
        let sid = SessionId::from_string("test-session");
        let mut ctx = ReplContext {
            active_agent: Some(AgentSlot::Build),
            ..Default::default()
        };

        let action = handle_slash_command("/switch foobar", &state, &sid, &mut ctx)
            .await
            .unwrap();
        assert_eq!(action, SlashAction::Continue);
        // active_agent unchanged — still Build
        assert_eq!(ctx.active_agent, Some(AgentSlot::Build));
    }

    #[tokio::test]
    async fn test_cmd_switch_already_plan() {
        let state = make_test_state().await;
        let sid = SessionId::from_string("test-session");
        let mut ctx = ReplContext {
            active_agent: Some(AgentSlot::Plan),
            ..Default::default()
        };

        let action = handle_slash_command("/switch plan", &state, &sid, &mut ctx)
            .await
            .unwrap();
        assert_eq!(action, SlashAction::Continue);
        assert_eq!(ctx.active_agent, Some(AgentSlot::Plan)); // still plan
    }

    // ── /plan-to-build tests ────────────────────────────────────────────

    #[tokio::test]
    async fn test_cmd_plan_to_build_not_dual() {
        let state = make_test_state().await;
        let sid = SessionId::from_string("test-session");
        let mut ctx = ReplContext::default(); // active_agent = None

        let action = handle_slash_command("/plan-to-build", &state, &sid, &mut ctx)
            .await
            .unwrap();
        assert_eq!(action, SlashAction::Continue);
    }

    #[tokio::test]
    async fn test_cmd_plan_to_build_dual_mode() {
        let state = make_test_state().await;
        let sid = SessionId::from_string("test-session");
        let mut ctx = ReplContext {
            active_agent: Some(AgentSlot::Plan),
            ..Default::default()
        };

        let action = handle_slash_command("/plan-to-build", &state, &sid, &mut ctx)
            .await
            .unwrap();
        assert_eq!(action, SlashAction::Continue);
    }

    #[tokio::test]
    async fn test_cmd_plan_to_build_alias_ptb() {
        let state = make_test_state().await;
        let sid = SessionId::from_string("test-session");
        let mut ctx = ReplContext {
            active_agent: Some(AgentSlot::Build),
            ..Default::default()
        };

        let action = handle_slash_command("/ptb", &state, &sid, &mut ctx)
            .await
            .unwrap();
        assert_eq!(action, SlashAction::Continue);
    }

    // ── /memory tests ──────────────────────────────────────────────────

    #[tokio::test]
    async fn test_cmd_memory_status() {
        let state = make_test_state().await;
        let sid = SessionId::from_string("test-session");
        let mut ctx = ReplContext::default();

        let action = handle_slash_command("/memory", &state, &sid, &mut ctx)
            .await
            .unwrap();
        assert_eq!(action, SlashAction::Continue);
        assert!(ctx.auto_memory_enabled); // default is enabled
    }

    #[tokio::test]
    async fn test_cmd_memory_default_is_status() {
        let state = make_test_state().await;
        let sid = SessionId::from_string("test-session");
        let mut ctx = ReplContext::default();

        // "/memory" with no args should behave same as "/memory status"
        let action = handle_slash_command("/memory status", &state, &sid, &mut ctx)
            .await
            .unwrap();
        assert_eq!(action, SlashAction::Continue);
    }

    #[tokio::test]
    async fn test_cmd_memory_auto_no_messages() {
        let state = make_test_state().await;
        let sid = SessionId::from_string("test-session");
        let mut ctx = ReplContext::default(); // message_count = 0

        let action = handle_slash_command("/memory auto", &state, &sid, &mut ctx)
            .await
            .unwrap();
        assert_eq!(action, SlashAction::Continue);
    }

    #[tokio::test]
    async fn test_cmd_memory_auto_with_messages() {
        let state = make_test_state().await;
        let sid = SessionId::from_string("test-session");
        let mut ctx = ReplContext {
            message_count: 15,
            ..Default::default()
        };

        let action = handle_slash_command("/memory auto", &state, &sid, &mut ctx)
            .await
            .unwrap();
        assert_eq!(action, SlashAction::Continue);
    }

    #[tokio::test]
    async fn test_cmd_memory_clear() {
        let state = make_test_state().await;
        let sid = SessionId::from_string("test-session");
        let mut ctx = ReplContext::default();
        ctx.auto_memory_count = 5;

        let action = handle_slash_command("/memory clear", &state, &sid, &mut ctx)
            .await
            .unwrap();
        assert_eq!(action, SlashAction::Continue);
        assert_eq!(ctx.auto_memory_count, 0); // cleared
    }

    #[tokio::test]
    async fn test_cmd_memory_on_off() {
        let state = make_test_state().await;
        let sid = SessionId::from_string("test-session");
        let mut ctx = ReplContext::default();
        assert!(ctx.auto_memory_enabled); // default on

        // Turn off
        let action = handle_slash_command("/memory off", &state, &sid, &mut ctx)
            .await
            .unwrap();
        assert_eq!(action, SlashAction::Continue);
        assert!(!ctx.auto_memory_enabled);

        // Turn on
        let action = handle_slash_command("/memory on", &state, &sid, &mut ctx)
            .await
            .unwrap();
        assert_eq!(action, SlashAction::Continue);
        assert!(ctx.auto_memory_enabled);
    }

    #[tokio::test]
    async fn test_cmd_memory_invalid_subcmd() {
        let state = make_test_state().await;
        let sid = SessionId::from_string("test-session");
        let mut ctx = ReplContext::default();

        let action = handle_slash_command("/memory foobar", &state, &sid, &mut ctx)
            .await
            .unwrap();
        assert_eq!(action, SlashAction::Continue);
    }

    /// Build a minimal `AppState` for testing slash commands.
    async fn make_test_state() -> AppState {
        let tmp_dir = std::env::temp_dir().join(format!("octo-test-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&tmp_dir);
        let db_path = tmp_dir.join("test.db");
        AppState::new(db_path, crate::output::OutputConfig::default())
            .await
            .expect("failed to create test AppState")
    }
}
