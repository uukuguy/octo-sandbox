use octo_engine::skills::{resolve_model, resolve_provider, SkillModelOverride};

#[test]
fn test_model_override_creation() {
    let ov = SkillModelOverride::new("gpt-4o");

    assert_eq!(ov.model, "gpt-4o");
    assert!(ov.provider.is_none());
    assert!(ov.max_tokens.is_none());
    assert!(ov.temperature.is_none());
}

#[test]
fn test_model_override_with_provider() {
    let ov = SkillModelOverride::new("gpt-4o").with_provider("openai");

    assert_eq!(ov.model, "gpt-4o");
    assert_eq!(ov.provider, Some("openai".to_string()));
}

#[test]
fn test_model_override_with_max_tokens() {
    let ov = SkillModelOverride::new("gpt-4o").with_max_tokens(4096);

    assert_eq!(ov.max_tokens, Some(4096));
}

#[test]
fn test_model_override_with_temperature() {
    let ov = SkillModelOverride::new("gpt-4o").with_temperature(0.7);

    assert_eq!(ov.temperature, Some(0.7));
}

#[test]
fn test_resolve_model_with_override() {
    let ov = SkillModelOverride::new("gpt-4o");
    let resolved = resolve_model(Some(&ov), "claude-sonnet-4-6");

    assert_eq!(resolved, "gpt-4o");
}

#[test]
fn test_resolve_model_without_override() {
    let resolved = resolve_model(None, "claude-sonnet-4-6");

    assert_eq!(resolved, "claude-sonnet-4-6");
}

#[test]
fn test_resolve_provider_with_override() {
    let ov = SkillModelOverride::new("gpt-4o").with_provider("openai");
    let resolved = resolve_provider(Some(&ov), "anthropic");

    assert_eq!(resolved, "openai");
}

#[test]
fn test_resolve_provider_without_override() {
    let resolved = resolve_provider(None, "anthropic");

    assert_eq!(resolved, "anthropic");
}

#[test]
fn test_model_override_serialization() {
    let ov = SkillModelOverride::new("gpt-4o")
        .with_provider("openai")
        .with_max_tokens(4096)
        .with_temperature(0.7);

    let json = serde_json::to_string(&ov).expect("serialize");
    let deserialized: SkillModelOverride = serde_json::from_str(&json).expect("deserialize");

    assert_eq!(ov, deserialized);
}

#[test]
fn test_resolve_provider_without_provider_in_override() {
    // Override has model but no provider — should fall back to default provider
    let ov = SkillModelOverride::new("gpt-4o");
    assert!(ov.provider.is_none());

    let resolved = resolve_provider(Some(&ov), "anthropic");
    assert_eq!(resolved, "anthropic");
}
