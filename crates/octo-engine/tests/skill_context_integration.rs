use std::path::PathBuf;

use octo_engine::context::manager::{ContextManager, EstimateCounter};
use octo_engine::skills::loader::SkillLoader;
use octo_engine::skills::manager::SkillManager;
use octo_engine::skills::registry::SkillRegistry;
use octo_engine::skills::trust::TrustManager;

// -- SkillManager prompt section tests --

fn create_test_manager() -> SkillManager {
    let loader = SkillLoader::new(None, None);
    let registry = SkillRegistry::new();
    let trust_manager = TrustManager::new(vec![]);
    SkillManager::new(loader, registry, trust_manager)
}

#[test]
fn test_prompt_section_l1_empty() {
    let mgr = create_test_manager();
    // No index built yet → empty string
    assert_eq!(mgr.prompt_section_l1(), "");
}

#[test]
fn test_prompt_section_l2_empty() {
    let mgr = create_test_manager();
    let result = mgr.prompt_section_l2(&[]);
    // No index built → falls back to l1 → empty
    assert_eq!(result, "");
}

#[test]
fn test_always_active_skills_empty() {
    let mgr = create_test_manager();
    assert!(mgr.always_active_skills().is_empty());
}

// -- ContextManager with token counting --

#[test]
fn test_context_manager_budget_snapshot() {
    let counter = Box::new(EstimateCounter);
    let mgr = ContextManager::new(counter, 4096);

    let snapshot = mgr.budget_snapshot("System prompt here", &[]);
    assert!(snapshot.system_tokens > 0);
    assert_eq!(snapshot.message_tokens, 0);
    assert!(snapshot.remaining > 0);
    assert!(snapshot.total_budget == 4096);
}

#[test]
fn test_context_manager_needs_pruning() {
    let counter = Box::new(EstimateCounter);
    let mgr = ContextManager::new(counter, 100); // tiny budget

    // With a large system prompt relative to budget
    let large_prompt = "x".repeat(400); // ~100 tokens
    let snapshot = mgr.budget_snapshot(&large_prompt, &[]);
    // Usage should be high with 100 token budget
    assert!(mgr.needs_pruning(&snapshot) || snapshot.usage_pct > 0.5);
}

#[test]
fn test_context_manager_available_tokens() {
    let counter = Box::new(EstimateCounter);
    let mgr = ContextManager::new(counter, 4096);

    let snapshot = mgr.budget_snapshot("test", &[]);
    let available = mgr.available_tokens(&snapshot);
    assert!(available > 0);
    assert!(available < 4096);
}

#[test]
fn test_context_manager_custom_reserve_pct() {
    let counter = Box::new(EstimateCounter);
    let mgr = ContextManager::new(counter, 4096).with_system_reserve_pct(0.25);

    let snapshot = mgr.budget_snapshot("test", &[]);
    // 25% of 4096 = 1024 tool tokens
    assert_eq!(snapshot.tool_tokens, 1024);
}
