use serde::{Deserialize, Serialize};

/// Tenant
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tenant {
    pub id: String,
    pub name: String,
    pub slug: String,
    pub plan: TenantPlan,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[derive(Default)]
pub enum TenantPlan {
    #[default]
    Free,
    Pro,
    Enterprise,
}

/// Resource quota
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceQuota {
    pub max_agents: u32,
    pub max_sessions_per_user: u32,
    pub max_api_calls_per_day: u64,
    pub max_memory_mb: u64,
    pub max_mcp_servers: u32,
}

impl Default for ResourceQuota {
    fn default() -> Self {
        Self {
            max_agents: 5,
            max_sessions_per_user: 10,
            max_api_calls_per_day: 1000,
            max_memory_mb: 1024,
            max_mcp_servers: 5,
        }
    }
}
