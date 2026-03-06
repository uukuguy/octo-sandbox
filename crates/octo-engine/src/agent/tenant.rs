use octo_types::{TenantId, UserId};

use crate::auth::{Action, Role};

#[derive(Debug, Clone)]
pub struct TenantContext {
    pub tenant_id: TenantId,
    pub user_id: UserId,
    pub roles: Vec<Role>,
}

impl TenantContext {
    /// 单用户场景 (octo-workbench)
    pub fn for_single_user(tenant_id: TenantId, user_id: UserId) -> Self {
        Self {
            tenant_id,
            user_id,
            roles: vec![Role::Owner],
        }
    }

    /// 验证用户有权限执行操作
    pub fn can(&self, action: Action) -> bool {
        self.roles.iter().any(|role| role.can(action))
    }
}
