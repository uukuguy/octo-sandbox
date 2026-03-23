//! Sandbox diagnostic commands for inspecting execution environment.

use anyhow::Result;

use octo_engine::sandbox::{
    ExecutionTargetResolver, OctoRunMode, SandboxProfile, SandboxRouter, ToolCategory,
};

use super::types::SandboxCommands;
use super::AppState;

pub async fn handle_sandbox(action: SandboxCommands, _state: &AppState) -> Result<()> {
    match action {
        SandboxCommands::Status => show_status(),
        SandboxCommands::DryRun => show_dry_run(),
        SandboxCommands::ListBackends => list_backends(),
    }
}

fn show_status() -> Result<()> {
    let profile = SandboxProfile::resolve(false, None, None);
    let run_mode = OctoRunMode::detect();

    println!("Sandbox Status");
    println!("{}", "─".repeat(40));
    println!("  Profile:    {}", profile_display(&profile));
    println!("  Run Mode:   {}", run_mode_display(&run_mode));
    println!("  Policy:     {:?}", profile.policy());
    println!("  Timeout:    {}s", profile.timeout_secs());
    println!("  Env Pass:   {}", if profile.env_passthrough() { "full" } else { "restricted" });
    println!("  Approval:   {:?}", profile.approval_gate());
    println!("  Audit:      {:?}", profile.audit_level());

    Ok(())
}

fn show_dry_run() -> Result<()> {
    let profile = SandboxProfile::resolve(false, None, None);
    let run_mode = OctoRunMode::detect();
    let router = SandboxRouter::with_policy(profile.policy());
    let available = router.registered_backends();
    let resolver = ExecutionTargetResolver::new(run_mode.clone(), profile.clone(), available);

    println!("Sandbox Routing Dry-Run");
    println!("{}", "─".repeat(60));
    println!("  Profile: {}  |  Mode: {}", profile_display(&profile), run_mode_display(&run_mode));
    println!();

    let categories = [
        ("Shell",      ToolCategory::Shell),
        ("Compute",    ToolCategory::Compute),
        ("FileSystem", ToolCategory::FileSystem),
        ("Network",    ToolCategory::Network),
        ("Script",     ToolCategory::Script),
        ("Gpu",        ToolCategory::Gpu),
        ("Untrusted",  ToolCategory::Untrusted),
    ];

    println!("  {:<12} {:<25} {}", "Category", "Target", "Reason");
    println!("  {:<12} {:<25} {}", "────────", "──────", "──────");

    for (label, cat) in &categories {
        let preview = resolver.dry_run(cat.clone());
        println!("  {:<12} {:<25} {}", label, format!("{}", preview.target), preview.reason);
    }

    Ok(())
}

fn list_backends() -> Result<()> {
    let profile = SandboxProfile::resolve(false, None, None);
    let router = SandboxRouter::with_policy(profile.policy());
    let backends = router.registered_backends();

    println!("Registered Sandbox Backends");
    println!("{}", "─".repeat(40));

    if backends.is_empty() {
        println!("  (none — all execution is local)");
    } else {
        for backend in &backends {
            println!("  - {:?}", backend);
        }
    }

    println!();
    println!("Profile: {}", profile_display(&profile));
    println!("Policy:  {:?}", profile.policy());

    Ok(())
}

fn profile_display(profile: &SandboxProfile) -> &'static str {
    match profile {
        SandboxProfile::Development => "development",
        SandboxProfile::Staging => "staging",
        SandboxProfile::Production => "production",
        SandboxProfile::Custom(_) => "custom",
    }
}

fn run_mode_display(mode: &OctoRunMode) -> &'static str {
    match mode {
        OctoRunMode::Host => "host",
        OctoRunMode::Sandboxed => "sandboxed (container)",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_profile_display() {
        assert_eq!(profile_display(&SandboxProfile::Development), "development");
        assert_eq!(profile_display(&SandboxProfile::Staging), "staging");
        assert_eq!(profile_display(&SandboxProfile::Production), "production");
    }

    #[test]
    fn test_run_mode_display() {
        assert_eq!(run_mode_display(&OctoRunMode::Host), "host");
        assert_eq!(run_mode_display(&OctoRunMode::Sandboxed), "sandboxed (container)");
    }

    #[test]
    fn test_show_status_runs() {
        let result = show_status();
        assert!(result.is_ok());
    }

    #[test]
    fn test_show_dry_run_runs() {
        let result = show_dry_run();
        assert!(result.is_ok());
    }

    #[test]
    fn test_list_backends_runs() {
        let result = list_backends();
        assert!(result.is_ok());
    }
}
