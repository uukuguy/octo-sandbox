use octo_types::{TenantId, UserId};

#[derive(Debug, Clone)]
pub struct TenantContext {
    pub tenant_id: TenantId,
    pub user_id: UserId,
    pub roles: Vec<Role>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Viewer,
    User,
    Admin,
    Owner,
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
        match (self.roles.as_slice(), action) {
            ([Role::Owner | Role::Admin], _) => true,
            ([Role::User], Action::RunAgent | Action::CreateSession) => true,
            ([Role::Viewer], Action::Read) => true,
            _ => false,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum Action {
    Read,
    CreateSession,
    RunAgent,
    ManageAgent,
    ManageMcp,
}
