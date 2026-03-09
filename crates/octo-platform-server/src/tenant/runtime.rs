use dashmap::DashMap;

use super::models::ResourceQuota;
use super::quota::QuotaManager;
use crate::audit::AuditEvent;

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

    /// Publish an audit event (currently just logs, can be extended to emit to event bus)
    pub fn publish_audit_event(&self, event: AuditEvent) {
        // For now, just log the event
        // In the future, this could emit to an event bus or write to a database
        tracing::info!(
            "Audit event: {} - {} - {:?}",
            event.tenant_id,
            event.action,
            event.details
        );
    }
}
