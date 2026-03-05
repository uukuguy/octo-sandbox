use dashmap::DashMap;

use super::models::ResourceQuota;
use super::quota::QuotaManager;

/// Tenant runtime - isolated per tenant
pub struct TenantRuntime {
    pub tenant_id: String,
    pub quota: ResourceQuota,
    pub quota_manager: QuotaManager,
    pub mcp_servers: DashMap<String, serde_json::Value>,
}

impl TenantRuntime {
    pub fn new(tenant_id: String, quota: ResourceQuota) -> Self {
        Self {
            tenant_id,
            quota: quota.clone(),
            quota_manager: QuotaManager::new(quota),
            mcp_servers: DashMap::new(),
        }
    }
}
