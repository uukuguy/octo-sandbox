use std::sync::Arc;

use dashmap::DashMap;

use super::models::ResourceQuota;

/// Tenant runtime - isolated per tenant
pub struct TenantRuntime {
    pub tenant_id: String,
    pub quota: ResourceQuota,
    pub mcp_servers: DashMap<String, serde_json::Value>,
}

impl TenantRuntime {
    pub fn new(tenant_id: String, quota: ResourceQuota) -> Self {
        Self {
            tenant_id,
            quota,
            mcp_servers: DashMap::new(),
        }
    }
}
