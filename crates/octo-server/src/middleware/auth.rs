use axum::{body::Body, extract::Request, http::StatusCode, middleware::Next, response::Response};
use octo_engine::auth::{AuthConfig, AuthMode, Permission};
use octo_engine::auth::middleware::{RequiredAction, UserContext};
use octo_engine::auth::roles::Role;

/// 从请求中提取用户上下文
#[allow(dead_code)]
pub fn get_user_context<B>(req: &Request<B>) -> Option<UserContext> {
    req.extensions().get::<UserContext>().cloned()
}

/// 认证中间件 - 验证 API Key 并提取角色信息
pub async fn auth_middleware_with_role(
    req: Request<Body>,
    next: Next,
    config: &AuthConfig,
) -> Result<Response, StatusCode> {
    // Health check is always public regardless of auth mode
    if req.uri().path() == "/api/health" {
        let mut req = req;
        req.extensions_mut().insert(UserContext::anonymous());
        return Ok(next.run(req).await);
    }

    match config.mode {
        AuthMode::None => {
            let mut req = req;
            req.extensions_mut().insert(UserContext::anonymous());
            Ok(next.run(req).await)
        }
        AuthMode::ApiKey => {
            let key = req.headers().get("x-api-key").and_then(|v| v.to_str().ok());

            match key {
                Some(k) if config.validate_key(k) => {
                    let user_id = config.get_user_id(k);
                    let permissions = config.get_permissions(k);
                    let role = config.get_role(k);

                    let mut req = req;
                    req.extensions_mut()
                        .insert(UserContext::new(user_id, permissions, role));
                    Ok(next.run(req).await)
                }
                _ => Err(StatusCode::UNAUTHORIZED),
            }
        }
        AuthMode::Full => {
            let auth_header = req.headers().get("authorization");
            let token = auth_header
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.strip_prefix("Bearer "));

            match token {
                Some(t) => {
                    if let Some(claims) = config.validate_jwt(t) {
                        let (permissions, role) = match claims.role.as_str() {
                            "admin" => (vec![Permission::Admin], Some(Role::Admin)),
                            "member" => (
                                vec![Permission::Read, Permission::Write],
                                Some(Role::User),
                            ),
                            "viewer" => (vec![Permission::Read], Some(Role::Viewer)),
                            "owner" => (vec![Permission::Admin], Some(Role::Owner)),
                            _ => (vec![], None),
                        };

                        let mut req = req;
                        req.extensions_mut().insert(UserContext {
                            user_id: Some(claims.sub),
                            permissions,
                            role,
                        });
                        Ok(next.run(req).await)
                    } else {
                        Err(StatusCode::UNAUTHORIZED)
                    }
                }
                _ => Err(StatusCode::UNAUTHORIZED),
            }
        }
    }
}

/// RBAC 中间件: 检查是否具有执行特定动作的权限
#[allow(dead_code)]
pub async fn require_action_middleware(
    req: Request<Body>,
    next: Next,
    required_action: RequiredAction,
) -> Result<Response, StatusCode> {
    let user_ctx = get_user_context(&req).ok_or(StatusCode::UNAUTHORIZED)?;

    let required_permission = match required_action.0 {
        octo_engine::auth::roles::Action::Read => Some(Permission::Read),
        octo_engine::auth::roles::Action::CreateSession => Some(Permission::Write),
        octo_engine::auth::roles::Action::RunAgent => Some(Permission::Write),
        octo_engine::auth::roles::Action::ManageMcp => Some(Permission::Admin),
        octo_engine::auth::roles::Action::ManageSkills => Some(Permission::Admin),
        octo_engine::auth::roles::Action::ManageUsers => Some(Permission::Admin),
        octo_engine::auth::roles::Action::ManageConfig => Some(Permission::Admin),
    };

    if let Some(permission) = required_permission {
        if user_ctx.has_permission(&permission) {
            Ok(next.run(req).await)
        } else {
            Err(StatusCode::FORBIDDEN)
        }
    } else {
        Err(StatusCode::FORBIDDEN)
    }
}

/// RBAC 中间件: 检查是否具有最低角色权限
#[allow(dead_code)]
pub async fn require_role_middleware(
    req: Request<Body>,
    next: Next,
    required_role: octo_engine::auth::middleware::RequiredRole,
) -> Result<Response, StatusCode> {
    let user_ctx = get_user_context(&req).ok_or(StatusCode::UNAUTHORIZED)?;

    if user_ctx.has_role(required_role.0) {
        Ok(next.run(req).await)
    } else {
        Err(StatusCode::FORBIDDEN)
    }
}
