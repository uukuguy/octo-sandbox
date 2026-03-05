// crates/octo-engine/src/auth/roles.rs

use serde::{Deserialize, Serialize};

/// 操作/动作枚举 - 用于细粒度的 RBAC
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    /// 读取资源
    Read,
    /// 创建会话
    CreateSession,
    /// 运行 Agent
    RunAgent,
    /// 管理 MCP 服务器
    ManageMcp,
    /// 管理 Skills
    ManageSkills,
    /// 管理用户
    ManageUsers,
    /// 管理配置
    ManageConfig,
}

impl Action {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "read" => Some(Action::Read),
            "create_session" => Some(Action::CreateSession),
            "run_agent" => Some(Action::RunAgent),
            "manage_mcp" => Some(Action::ManageMcp),
            "manage_skills" => Some(Action::ManageSkills),
            "manage_users" => Some(Action::ManageUsers),
            "manage_config" => Some(Action::ManageConfig),
            _ => None,
        }
    }
}

/// 用户角色 - 扩展为 5 级角色
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    /// 查看者 - 仅能读取资源
    Viewer,
    /// 普通用户 - 可以创建会话和运行 Agent
    User,
    /// 管理员 - 可以管理 MCP 和 Skills
    Admin,
    /// 所有者 - 可以管理用户和配置
    Owner,
}

impl Default for Role {
    fn default() -> Self {
        Self::User
    }
}

impl Role {
    /// 从字符串解析角色
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "viewer" => Some(Role::Viewer),
            "user" => Some(Role::User),
            "admin" => Some(Role::Admin),
            "owner" => Some(Role::Owner),
            _ => None,
        }
    }

    /// 检查角色是否具有执行某个动作的权限
    pub fn can(&self, action: Action) -> bool {
        match (self, action) {
            // Viewer: 仅能读取
            (Role::Viewer, Action::Read) => true,

            // User: 可以读取、创建会话、运行 Agent
            (Role::User, Action::Read) => true,
            (Role::User, Action::CreateSession) => true,
            (Role::User, Action::RunAgent) => true,

            // Admin: 除了用户的权限外，还可以管理 MCP 和 Skills
            (Role::Admin, Action::Read) => true,
            (Role::Admin, Action::CreateSession) => true,
            (Role::Admin, Action::RunAgent) => true,
            (Role::Admin, Action::ManageMcp) => true,
            (Role::Admin, Action::ManageSkills) => true,

            // Owner: 拥有所有权限
            (Role::Owner, _) => true,

            // 其他组合不允许
            _ => false,
        }
    }

    /// 获取角色的优先级（数值越高权限越大）
    pub fn priority(&self) -> u8 {
        match self {
            Role::Viewer => 1,
            Role::User => 2,
            Role::Admin => 3,
            Role::Owner => 4,
        }
    }

    /// 检查是否具有最低权限要求
    pub fn has_at_least(&self, required: &Role) -> bool {
        self.priority() >= required.priority()
    }
}

/// 角色层级检查辅助函数
pub fn is_higher_or_equal(role: Role, required: Role) -> bool {
    role.has_at_least(&required)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_viewer_permissions() {
        let viewer = Role::Viewer;

        // Viewer 可以读取
        assert!(viewer.can(Action::Read));

        // Viewer 不能创建会话
        assert!(!viewer.can(Action::CreateSession));

        // Viewer 不能运行 Agent
        assert!(!viewer.can(Action::RunAgent));

        // Viewer 不能管理 MCP
        assert!(!viewer.can(Action::ManageMcp));

        // Viewer 不能管理 Skills
        assert!(!viewer.can(Action::ManageSkills));

        // Viewer 不能管理用户
        assert!(!viewer.can(Action::ManageUsers));

        // Viewer 不能管理配置
        assert!(!viewer.can(Action::ManageConfig));
    }

    #[test]
    fn test_user_permissions() {
        let user = Role::User;

        // User 可以读取
        assert!(user.can(Action::Read));

        // User 可以创建会话
        assert!(user.can(Action::CreateSession));

        // User 可以运行 Agent
        assert!(user.can(Action::RunAgent));

        // User 不能管理 MCP
        assert!(!user.can(Action::ManageMcp));

        // User 不能管理 Skills
        assert!(!user.can(Action::ManageSkills));

        // User 不能管理用户
        assert!(!user.can(Action::ManageUsers));

        // User 不能管理配置
        assert!(!user.can(Action::ManageConfig));
    }

    #[test]
    fn test_admin_permissions() {
        let admin = Role::Admin;

        // Admin 可以读取
        assert!(admin.can(Action::Read));

        // Admin 可以创建会话
        assert!(admin.can(Action::CreateSession));

        // Admin 可以运行 Agent
        assert!(admin.can(Action::RunAgent));

        // Admin 可以管理 MCP
        assert!(admin.can(Action::ManageMcp));

        // Admin 可以管理 Skills
        assert!(admin.can(Action::ManageSkills));

        // Admin 不能管理用户
        assert!(!admin.can(Action::ManageUsers));

        // Admin 不能管理配置
        assert!(!admin.can(Action::ManageConfig));
    }

    #[test]
    fn test_owner_permissions() {
        let owner = Role::Owner;

        // Owner 可以执行所有操作
        assert!(owner.can(Action::Read));
        assert!(owner.can(Action::CreateSession));
        assert!(owner.can(Action::RunAgent));
        assert!(owner.can(Action::ManageMcp));
        assert!(owner.can(Action::ManageSkills));
        assert!(owner.can(Action::ManageUsers));
        assert!(owner.can(Action::ManageConfig));
    }

    #[test]
    fn test_role_priority() {
        assert!(Role::Viewer.priority() < Role::User.priority());
        assert!(Role::User.priority() < Role::Admin.priority());
        assert!(Role::Admin.priority() < Role::Owner.priority());
    }

    #[test]
    fn test_has_at_least() {
        // Viewer 满足 Viewer
        assert!(Role::Viewer.has_at_least(&Role::Viewer));
        // Viewer 不满足 User
        assert!(!Role::Viewer.has_at_least(&Role::User));

        // Owner 满足所有
        assert!(Role::Owner.has_at_least(&Role::Viewer));
        assert!(Role::Owner.has_at_least(&Role::User));
        assert!(Role::Owner.has_at_least(&Role::Admin));
    }

    #[test]
    fn test_is_higher_or_equal() {
        assert!(is_higher_or_equal(Role::Admin, Role::User));
        assert!(is_higher_or_equal(Role::Owner, Role::Admin));
        assert!(!is_higher_or_equal(Role::User, Role::Admin));
    }

    #[test]
    fn test_role_from_str() {
        assert_eq!(Role::from_str("viewer"), Some(Role::Viewer));
        assert_eq!(Role::from_str("USER"), Some(Role::User));
        assert_eq!(Role::from_str("Admin"), Some(Role::Admin));
        assert_eq!(Role::from_str("OWNER"), Some(Role::Owner));
        assert_eq!(Role::from_str("unknown"), None);
    }

    #[test]
    fn test_action_from_str() {
        assert_eq!(Action::from_str("read"), Some(Action::Read));
        assert_eq!(
            Action::from_str("create_session"),
            Some(Action::CreateSession)
        );
        assert_eq!(Action::from_str("run_agent"), Some(Action::RunAgent));
        assert_eq!(Action::from_str("manage_mcp"), Some(Action::ManageMcp));
        assert_eq!(
            Action::from_str("manage_skills"),
            Some(Action::ManageSkills)
        );
        assert_eq!(Action::from_str("manage_users"), Some(Action::ManageUsers));
        assert_eq!(
            Action::from_str("manage_config"),
            Some(Action::ManageConfig)
        );
        assert_eq!(Action::from_str("unknown"), None);
    }
}
