use tempfile::TempDir;

use octo_engine::skills::trust::TrustManager;
use octo_engine::skills::{SkillLoader, SkillManager, SkillRegistry};

/// Helper: create a SKILL.md inside `<temp>/.octo/skills/<name>/SKILL.md`.
fn create_skill(temp: &TempDir, name: &str, desc: &str, always: bool, tags: &[&str]) {
    let skill_dir = temp.path().join(".octo").join("skills").join(name);
    std::fs::create_dir_all(&skill_dir).unwrap();

    let tags_yaml = if tags.is_empty() {
        String::new()
    } else {
        let items: Vec<String> = tags.iter().map(|t| format!("  - {}", t)).collect();
        format!("tags:\n{}\n", items.join("\n"))
    };

    let always_yaml = if always {
        "always: true\n".to_string()
    } else {
        String::new()
    };

    let content = format!(
        r#"---
name: {name}
description: {desc}
{tags_yaml}{always_yaml}---

# {name} body
This is the body of {name}.
"#
    );

    std::fs::write(skill_dir.join("SKILL.md"), content).unwrap();
}

fn make_manager(temp: &TempDir) -> SkillManager {
    let loader = SkillLoader::new(Some(temp.path()), None);
    let registry = SkillRegistry::new();
    let trust_manager = TrustManager::default();
    SkillManager::new(loader, registry, trust_manager)
}

// ---------- build_index ----------

#[test]
fn test_build_index_returns_correct_entries() {
    let temp = TempDir::new().unwrap();
    create_skill(&temp, "alpha", "Alpha skill", false, &["code"]);
    create_skill(&temp, "beta", "Beta skill", true, &["test", "ci"]);

    let mut mgr = make_manager(&temp);
    let idx = mgr.build_index();

    assert_eq!(idx.len(), 2);

    // Sorted by name.
    assert_eq!(idx[0].name, "alpha");
    assert_eq!(idx[0].description, "Alpha skill");
    assert_eq!(idx[0].tags, vec!["code".to_string()]);
    assert!(!idx[0].always);

    assert_eq!(idx[1].name, "beta");
    assert_eq!(idx[1].description, "Beta skill");
    assert_eq!(idx[1].tags, vec!["test".to_string(), "ci".to_string()]);
    assert!(idx[1].always);
}

#[test]
fn test_build_index_empty_registry() {
    let temp = TempDir::new().unwrap();
    // Create the .octo/skills directory but no skills.
    let skills_dir = temp.path().join(".octo").join("skills");
    std::fs::create_dir_all(&skills_dir).unwrap();

    let mut mgr = make_manager(&temp);
    let idx = mgr.build_index();

    assert!(idx.is_empty());
}

// ---------- activate_skill ----------

#[test]
fn test_activate_skill_by_name() {
    let temp = TempDir::new().unwrap();
    create_skill(&temp, "my-skill", "My skill description", false, &[]);

    let mut mgr = make_manager(&temp);
    mgr.build_index();

    let skill = mgr.activate_skill("my-skill");
    assert!(skill.is_some());
    let skill = skill.unwrap();
    assert_eq!(skill.name, "my-skill");
    assert!(!skill.body.is_empty());
    assert!(skill.body.contains("body of my-skill"));
}

#[test]
fn test_activate_skill_not_found() {
    let temp = TempDir::new().unwrap();
    let skills_dir = temp.path().join(".octo").join("skills");
    std::fs::create_dir_all(&skills_dir).unwrap();

    let mut mgr = make_manager(&temp);
    mgr.build_index();

    let skill = mgr.activate_skill("nonexistent");
    assert!(skill.is_none());
}

// ---------- prompt_section_l1 ----------

#[test]
fn test_prompt_section_l1_format() {
    let temp = TempDir::new().unwrap();
    create_skill(&temp, "alpha", "Alpha desc", false, &["code"]);
    create_skill(&temp, "beta", "Beta desc", false, &[]);

    let mut mgr = make_manager(&temp);
    mgr.build_index();

    let prompt = mgr.prompt_section_l1();
    assert!(prompt.starts_with("<available_skills>"));
    assert!(prompt.ends_with("</available_skills>"));
    assert!(prompt.contains("- alpha: Alpha desc [code]"));
    assert!(prompt.contains("- beta: Beta desc\n"));
}

#[test]
fn test_prompt_section_l1_empty() {
    let temp = TempDir::new().unwrap();
    let skills_dir = temp.path().join(".octo").join("skills");
    std::fs::create_dir_all(&skills_dir).unwrap();

    let mut mgr = make_manager(&temp);
    mgr.build_index();

    let prompt = mgr.prompt_section_l1();
    assert!(prompt.is_empty());
}

// ---------- prompt_section_l2 ----------

#[test]
fn test_prompt_section_l2_includes_body() {
    let temp = TempDir::new().unwrap();
    create_skill(&temp, "alpha", "Alpha desc", false, &[]);

    let mut mgr = make_manager(&temp);
    mgr.build_index();

    let prompt = mgr.prompt_section_l2(&["alpha".to_string()]);
    assert!(prompt.contains("<active_skills>"));
    assert!(prompt.contains("## alpha"));
    assert!(prompt.contains("body of alpha"));
    assert!(prompt.contains("</active_skills>"));
}

#[test]
fn test_prompt_section_l2_fallback_to_l1_when_no_active() {
    let temp = TempDir::new().unwrap();
    create_skill(&temp, "alpha", "Alpha desc", false, &[]);

    let mut mgr = make_manager(&temp);
    mgr.build_index();

    // No active skills specified => falls back to L1.
    let prompt = mgr.prompt_section_l2(&[]);
    assert!(prompt.contains("<available_skills>"));
}

// ---------- always_active_skills ----------

#[test]
fn test_always_active_skills_filters_correctly() {
    let temp = TempDir::new().unwrap();
    create_skill(&temp, "always-on", "Always on", true, &[]);
    create_skill(&temp, "optional", "Optional", false, &[]);
    create_skill(&temp, "also-always", "Also always", true, &[]);

    let mut mgr = make_manager(&temp);
    mgr.build_index();

    let mut always = mgr.always_active_skills();
    always.sort();

    assert_eq!(
        always,
        vec!["also-always".to_string(), "always-on".to_string()]
    );
}

#[test]
fn test_always_active_skills_empty_when_none() {
    let temp = TempDir::new().unwrap();
    create_skill(&temp, "opt1", "Optional 1", false, &[]);
    create_skill(&temp, "opt2", "Optional 2", false, &[]);

    let mut mgr = make_manager(&temp);
    mgr.build_index();

    let always = mgr.always_active_skills();
    assert!(always.is_empty());
}

// ---------- index accessor ----------

#[test]
fn test_index_accessor_returns_cached() {
    let temp = TempDir::new().unwrap();
    create_skill(&temp, "cached", "Cached skill", false, &[]);

    let mut mgr = make_manager(&temp);

    // Before build_index, index is empty.
    assert!(mgr.index().is_empty());

    mgr.build_index();

    // After build_index, index is populated.
    assert_eq!(mgr.index().len(), 1);
    assert_eq!(mgr.index()[0].name, "cached");
}
