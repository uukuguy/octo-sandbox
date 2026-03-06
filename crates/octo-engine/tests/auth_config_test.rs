// crates/octo-engine/tests/auth_config_test.rs

use octo_engine::auth::*;

#[test]
fn test_auth_mode_default() {
    let config = AuthConfig::default();
    assert_eq!(config.mode, AuthMode::ApiKey);
    assert!(config.api_keys.is_empty());
}

#[test]
fn test_api_key_validation() {
    let mut config = AuthConfig::new();
    config.mode = AuthMode::ApiKey;
    config.add_api_key(
        "test-key-123",
        Some("user-001".to_string()),
        vec![Permission::Read, Permission::Write],
    );

    // 正确的 key 应该通过
    assert!(config.validate_key("test-key-123"));

    // 错误的 key 应该失败
    assert!(!config.validate_key("wrong-key"));

    // 无认证模式下，任何 key 都应该通过（向后兼容）
    let mut config_none = AuthConfig::new();
    config_none.mode = AuthMode::None;
    assert!(config_none.validate_key("any-key"));
}

#[test]
fn test_api_key_expiry() {
    use chrono::Utc;

    let mut config = AuthConfig::new();
    config.mode = AuthMode::ApiKey;
    config.add_api_key("expired-key", None, vec![]);

    // 修改为已过期
    if let Some(key) = config.api_keys.values_mut().next() {
        key.expires_at = Some(Utc::now() - chrono::Duration::hours(1));
    }

    assert!(!config.validate_key("expired-key"));
}

#[test]
fn test_get_user_id() {
    let mut config = AuthConfig::new();
    config.mode = AuthMode::ApiKey;
    config.add_api_key(
        "key-001",
        Some("user-001".to_string()),
        vec![Permission::Read],
    );

    assert_eq!(config.get_user_id("key-001"), Some("user-001".to_string()));
    assert_eq!(config.get_user_id("key-002"), None);
}

#[test]
fn test_permissions() {
    let mut config = AuthConfig::new();
    config.mode = AuthMode::ApiKey;
    config.add_api_key(
        "key-admin",
        Some("user-001".to_string()),
        vec![Permission::Read, Permission::Write, Permission::Admin],
    );

    let perms = config.get_permissions("key-admin");
    assert!(perms.contains(&Permission::Read));
    assert!(perms.contains(&Permission::Write));
    assert!(perms.contains(&Permission::Admin));
}
