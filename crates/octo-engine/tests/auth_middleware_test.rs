// crates/octo-engine/tests/auth_middleware_test.rs

use axum::{body::Body, extract::Request};
use octo_engine::auth::roles::Role;
use octo_engine::auth::*;

// Test get_user_context function directly
#[test]
fn test_get_user_context_none() {
    let req = Request::builder().uri("/").body(Body::empty()).unwrap();
    let ctx = get_user_context(&req);
    assert!(ctx.is_none());
}

#[test]
fn test_get_user_context_with_context() {
    let mut req = Request::builder().uri("/").body(Body::empty()).unwrap();

    req.extensions_mut().insert(UserContext {
        user_id: Some("user-001".to_string()),
        permissions: vec![Permission::Read, Permission::Write],
        role: Some(Role::User),
    });

    let ctx = get_user_context(&req).unwrap();
    assert_eq!(ctx.user_id, Some("user-001".to_string()));
    assert!(ctx.permissions.contains(&Permission::Read));
    assert!(ctx.permissions.contains(&Permission::Write));
    assert_eq!(ctx.role, Some(Role::User));
}

// Test UserContext::has_permission
#[test]
fn test_user_context_has_permission() {
    let ctx = UserContext {
        user_id: Some("user-001".to_string()),
        permissions: vec![Permission::Read, Permission::Write],
        role: Some(Role::User),
    };

    assert!(ctx.has_permission(&Permission::Read));
    assert!(ctx.has_permission(&Permission::Write));
    assert!(!ctx.has_permission(&Permission::Admin));
}

// Test UserContext::has_role
#[test]
fn test_user_context_has_role() {
    // User can access User-level actions
    let ctx_user = UserContext {
        user_id: Some("user-001".to_string()),
        permissions: vec![Permission::Read, Permission::Write],
        role: Some(Role::User),
    };

    assert!(ctx_user.has_role(Role::User));
    assert!(!ctx_user.has_role(Role::Admin));

    // Admin can access Admin-level actions
    let ctx_admin = UserContext {
        user_id: Some("admin-001".to_string()),
        permissions: vec![Permission::Read, Permission::Write, Permission::Admin],
        role: Some(Role::Admin),
    };

    assert!(ctx_admin.has_role(Role::User));
    assert!(ctx_admin.has_role(Role::Admin));

    // Owner can access all
    let ctx_owner = UserContext {
        user_id: Some("owner-001".to_string()),
        permissions: vec![Permission::Read, Permission::Write, Permission::Admin],
        role: Some(Role::Owner),
    };

    assert!(ctx_owner.has_role(Role::Viewer));
    assert!(ctx_owner.has_role(Role::User));
    assert!(ctx_owner.has_role(Role::Admin));
    assert!(ctx_owner.has_role(Role::Owner));

    // No role = no access
    let ctx_anon = UserContext::anonymous();
    assert!(!ctx_anon.has_role(Role::User));
}

// Test UserContext::anonymous
#[test]
fn test_user_context_anonymous() {
    let ctx = UserContext::anonymous();

    assert_eq!(ctx.user_id, None);
    assert!(ctx.permissions.is_empty());
    assert_eq!(ctx.role, None);
}

// Test that AuthConfig::validate_key works correctly
#[test]
fn test_auth_config_none_mode_allows_any_key() {
    let mut config = AuthConfig::new();
    config.mode = AuthMode::None;

    // In None mode, any key should be valid (backward compatibility)
    assert!(config.validate_key("any-key"));
    assert!(config.validate_key(""));
}

#[test]
fn test_auth_config_api_key_mode_requires_valid_key() {
    let mut config = AuthConfig::new();
    config.mode = AuthMode::ApiKey;

    // No keys added, should reject
    assert!(!config.validate_key("any-key"));

    // Add a valid key
    config.add_api_key("valid-key", None, vec![]);
    assert!(config.validate_key("valid-key"));
    assert!(!config.validate_key("invalid-key"));
}

#[test]
fn test_auth_config_requires_user_id() {
    let mut config = AuthConfig::new();
    config.require_user_id = true;

    // With require_user_id, API key must have user_id
    config.mode = AuthMode::ApiKey;
    config.add_api_key("key-without-user", None, vec![]);

    // This key has no user_id but config requires it
    // The validation logic doesn't check require_user_id in validate_key
    // That would be done at the middleware level
    assert!(config.validate_key("key-without-user"));
}

// Test AuthConfig with role
#[test]
fn test_auth_config_with_role() {
    let mut config = AuthConfig::new();
    config.mode = AuthMode::ApiKey;

    // Add API key with role
    config.add_api_key_with_role(
        "admin-key",
        Some("admin-001".to_string()),
        vec![Permission::Read, Permission::Write, Permission::Admin],
        Some(Role::Admin),
    );

    // Validate the key
    assert!(config.validate_key("admin-key"));

    // Get role
    let role = config.get_role("admin-key");
    assert_eq!(role, Some(Role::Admin));

    // Get user_id
    let user_id = config.get_user_id("admin-key");
    assert_eq!(user_id, Some("admin-001".to_string()));

    // Key without role
    config.add_api_key(
        "user-key",
        Some("user-001".to_string()),
        vec![Permission::Read],
    );
    let role = config.get_role("user-key");
    assert_eq!(role, None);
}
