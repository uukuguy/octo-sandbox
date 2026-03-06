use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEvent {
    pub tenant_id: String,
    pub user_id: Option<String>,
    pub action: AuditAction,
    pub resource_type: Option<String>,
    pub resource_id: Option<String>,
    pub details: Option<serde_json::Value>,
    pub ip_address: Option<String>,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum AuditAction {
    Login,
    LoginFailed,
    Logout,
    CreateAgent,
    DeleteAgent,
    CreateSession,
    DeleteSession,
    UpdateQuota,
    CreateMcp,
    DeleteMcp,
    UpdateUser,
    DeleteUser,
    CreateTenant,
    UpdateTenant,
    DeleteTenant,
}

impl std::fmt::Display for AuditAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Login => write!(f, "login"),
            Self::LoginFailed => write!(f, "login_failed"),
            Self::Logout => write!(f, "logout"),
            Self::CreateAgent => write!(f, "create_agent"),
            Self::DeleteAgent => write!(f, "delete_agent"),
            Self::CreateSession => write!(f, "create_session"),
            Self::DeleteSession => write!(f, "delete_session"),
            Self::UpdateQuota => write!(f, "update_quota"),
            Self::CreateMcp => write!(f, "create_mcp"),
            Self::DeleteMcp => write!(f, "delete_mcp"),
            Self::UpdateUser => write!(f, "update_user"),
            Self::DeleteUser => write!(f, "delete_user"),
            Self::CreateTenant => write!(f, "create_tenant"),
            Self::UpdateTenant => write!(f, "update_tenant"),
            Self::DeleteTenant => write!(f, "delete_tenant"),
        }
    }
}

/// Audit event builder for convenient event creation
pub struct AuditEventBuilder {
    tenant_id: String,
    user_id: Option<String>,
    action: AuditAction,
    resource_type: Option<String>,
    resource_id: Option<String>,
    details: Option<serde_json::Value>,
    ip_address: Option<String>,
}

impl AuditEventBuilder {
    pub fn new(tenant_id: String, action: AuditAction) -> Self {
        Self {
            tenant_id,
            user_id: None,
            action,
            resource_type: None,
            resource_id: None,
            details: None,
            ip_address: None,
        }
    }

    pub fn user_id(mut self, user_id: String) -> Self {
        self.user_id = Some(user_id);
        self
    }

    pub fn resource(mut self, resource_type: String, resource_id: String) -> Self {
        self.resource_type = Some(resource_type);
        self.resource_id = Some(resource_id);
        self
    }

    pub fn details(mut self, details: serde_json::Value) -> Self {
        self.details = Some(details);
        self
    }

    pub fn ip_address(mut self, ip_address: String) -> Self {
        self.ip_address = Some(ip_address);
        self
    }

    pub fn build(self) -> AuditEvent {
        AuditEvent {
            tenant_id: self.tenant_id,
            user_id: self.user_id,
            action: self.action,
            resource_type: self.resource_type,
            resource_id: self.resource_id,
            details: self.details,
            ip_address: self.ip_address,
            timestamp: Utc::now(),
        }
    }
}
