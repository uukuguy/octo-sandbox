use octo_types::{SkillDefinition, SkillSourceType, TrustLevel};

#[test]
fn test_skill_definition_enhanced_fields() {
    let yaml = r#"
name: test-skill
description: A test skill
version: "1.0.0"
model: claude-sonnet-4-6
context-fork: true
always: true
trust-level: trusted
denied-tools:
  - http_request
tags:
  - devops
"#;
    let skill: SkillDefinition = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(skill.model, Some("claude-sonnet-4-6".into()));
    assert!(skill.context_fork);
    assert!(skill.always);
    assert_eq!(skill.trust_level, TrustLevel::Trusted);
    assert_eq!(skill.denied_tools, Some(vec!["http_request".into()]));
    assert_eq!(skill.tags, vec!["devops".to_string()]);
}

#[test]
fn test_skill_definition_defaults() {
    let yaml = r#"
name: minimal
description: Minimal skill
"#;
    let skill: SkillDefinition = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(skill.model, None);
    assert!(!skill.context_fork);
    assert!(!skill.always);
    assert_eq!(skill.trust_level, TrustLevel::Installed);
    assert!(skill.denied_tools.is_none());
    assert!(skill.tags.is_empty());
    assert_eq!(skill.source_type, SkillSourceType::ProjectLocal);
}

#[test]
fn test_trust_level_deserialization() {
    let yaml = "trusted";
    let level: TrustLevel = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(level, TrustLevel::Trusted);

    let yaml = "unknown";
    let level: TrustLevel = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(level, TrustLevel::Unknown);
}
