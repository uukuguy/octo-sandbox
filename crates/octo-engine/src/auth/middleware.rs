// crates/octo-engine/src/auth/middleware.rs

use crate::auth::{roles::Role, Permission};

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

/// RBAC: 需要的动作
#[derive(Clone, Debug)]
pub struct RequiredAction(pub crate::auth::roles::Action);

/// RBAC: 需要的角色
#[derive(Clone, Debug)]
pub struct RequiredRole(pub Role);
