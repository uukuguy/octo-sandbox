// crates/octo-engine/src/auth/middleware.rs

use crate::auth::{roles::Role, AuthConfig, AuthMode, Permission};
use axum::{body::Body, extract::Request, http::StatusCode, middleware::Next, response::Response};

/// 用户上下文
#[derive(Debug, Clone)]
pub struct UserContext {
    pub user_id: Option<String>,
    pub permissions: Vec<Permission>,
    pub role: Option<Role>,
}

impl UserContext {
    /// 创建一个新的用户上下文
    pub fn new(user_id: Option<String>, permissions: Vec<Permission>, role: Option<Role>) -> Self {
        Self {
            user_id,
            permissions,
            role,
        }
    }

    /// 创建一个匿名用户上下文
    pub fn anonymous() -> Self {
        Self {
            user_id: None,
            permissions: vec![],
            role: None,
        }
    }

    /// 检查是否具有特定权限
    pub fn has_permission(&self, permission: &Permission) -> bool {
        self.permissions.iter().any(|p| p == permission)
    }

    /// 检查是否具有特定角色（如果角色已设置）
    pub fn has_role(&self, required_role: Role) -> bool {
        self.role
            .map(|r| r.has_at_least(&required_role))
            .unwrap_or(false)
    }
}

/// 从请求中提取用户上下文
pub fn get_user_context<B>(req: &Request<B>) -> Option<UserContext> {
    req.extensions().get::<UserContext>().cloned()
}

/// RBAC: 需要的动作
#[derive(Clone, Debug)]
pub struct RequiredAction(pub crate::auth::roles::Action);

/// RBAC: 需要的角色
#[derive(Clone, Debug)]
pub struct RequiredRole(pub Role);

/// 认证中间件 - 验证 API Key 并提取角色信息
pub async fn auth_middleware_with_role(
    req: Request<Body>,
    next: Next,
    config: &AuthConfig,
) -> Result<Response, StatusCode> {
    match config.mode {
        AuthMode::None => {
            // 无认证模式，直接放行，注入匿名用户
            let mut req = req;
            req.extensions_mut().insert(UserContext::anonymous());
            Ok(next.run(req).await)
        }
        AuthMode::ApiKey => {
            // 验证 API Key
            let key = req.headers().get("x-api-key").and_then(|v| v.to_str().ok());

            match key {
                Some(k) if config.validate_key(k) => {
                    let user_id = config.get_user_id(k);
                    let permissions = config.get_permissions(k);
                    // 从 AuthConfig 获取角色信息（需要扩展 AuthConfig）
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
            // 完整认证仅在 octo-platform (多租户) 实现
            // octo-workbench (单用户) 使用 ApiKey 或 None 模式
            Err(StatusCode::NOT_IMPLEMENTED)
        }
    }
}

/// RBAC 中间件: 检查是否具有执行特定动作的权限
pub async fn require_action_middleware<B>(
    req: Request<Body>,
    next: Next,
    required_action: RequiredAction,
) -> Result<Response, StatusCode> {
    let user_ctx = get_user_context(&req).ok_or(StatusCode::UNAUTHORIZED)?;

    // 将 Action 转换为 Permission 进行检查
    let required_permission = match required_action.0 {
        crate::auth::roles::Action::Read => Some(Permission::Read),
        crate::auth::roles::Action::CreateSession => Some(Permission::Write),
        crate::auth::roles::Action::RunAgent => Some(Permission::Write),
        crate::auth::roles::Action::ManageMcp => Some(Permission::Admin),
        crate::auth::roles::Action::ManageSkills => Some(Permission::Admin),
        crate::auth::roles::Action::ManageUsers => Some(Permission::Admin),
        crate::auth::roles::Action::ManageConfig => Some(Permission::Admin),
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
pub async fn require_role_middleware<B>(
    req: Request<Body>,
    next: Next,
    required_role: RequiredRole,
) -> Result<Response, StatusCode> {
    let user_ctx = get_user_context(&req).ok_or(StatusCode::UNAUTHORIZED)?;

    if user_ctx.has_role(required_role.0) {
        Ok(next.run(req).await)
    } else {
        Err(StatusCode::FORBIDDEN)
    }
}
