use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use super::models::ResourceQuota;

const WINDOW_SECONDS: u64 = 86400; // 24 hours

pub struct QuotaManager {
    quota: ResourceQuota,
    daily_api_calls: AtomicU64,
    active_sessions: AtomicU32,
    active_agents: AtomicU32,
    window_start: AtomicU64,
}

impl QuotaManager {
    pub fn new(quota: ResourceQuota) -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        Self {
            quota,
            daily_api_calls: AtomicU64::new(0),
            active_sessions: AtomicU32::new(0),
            active_agents: AtomicU32::new(0),
            window_start: AtomicU64::new(now),
        }
    }

    fn check_window(&self) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let window = self.window_start.load(Ordering::Relaxed);
        if now - window >= WINDOW_SECONDS {
            // Reset window
            self.window_start.store(now, Ordering::Relaxed);
            self.daily_api_calls.store(0, Ordering::Relaxed);
        }
    }

    pub fn check_api_call(&self) -> Result<(), QuotaExceeded> {
        self.check_window();

        let used = self.daily_api_calls.load(Ordering::Relaxed);
        let limit = self.quota.max_api_calls_per_day;

        if used >= limit {
            return Err(QuotaExceeded::DailyApiCalls { limit, used });
        }
        Ok(())
    }

    pub fn consume_api_call(&self) -> Result<(), QuotaExceeded> {
        self.check_api_call()?;
        self.daily_api_calls.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }

    pub fn check_active_sessions(&self) -> Result<(), QuotaExceeded> {
        let used = self.active_sessions.load(Ordering::Relaxed);
        let limit = self.quota.max_sessions_per_user;

        if used >= limit {
            return Err(QuotaExceeded::ActiveSessions { limit, used });
        }
        Ok(())
    }

    pub fn acquire_session(&self) -> Result<SessionGuard<'_>, QuotaExceeded> {
        self.check_active_sessions()?;
        self.active_sessions.fetch_add(1, Ordering::Relaxed);
        Ok(SessionGuard { manager: self })
    }

    pub fn check_active_agents(&self) -> Result<(), QuotaExceeded> {
        let used = self.active_agents.load(Ordering::Relaxed);
        let limit = self.quota.max_agents;

        if used >= limit {
            return Err(QuotaExceeded::ActiveAgents { limit, used });
        }
        Ok(())
    }

    pub fn acquire_agent(&self) -> Result<AgentGuard<'_>, QuotaExceeded> {
        self.check_active_agents()?;
        self.active_agents.fetch_add(1, Ordering::Relaxed);
        Ok(AgentGuard { manager: self })
    }
}

pub struct SessionGuard<'a> {
    manager: &'a QuotaManager,
}

impl<'a> Drop for SessionGuard<'a> {
    fn drop(&mut self) {
        self.manager.active_sessions.fetch_sub(1, Ordering::Relaxed);
    }
}

pub struct AgentGuard<'a> {
    manager: &'a QuotaManager,
}

impl<'a> Drop for AgentGuard<'a> {
    fn drop(&mut self) {
        self.manager.active_agents.fetch_sub(1, Ordering::Relaxed);
    }
}

#[derive(Debug)]
pub enum QuotaExceeded {
    DailyApiCalls { limit: u64, used: u64 },
    ActiveSessions { limit: u32, used: u32 },
    ActiveAgents { limit: u32, used: u32 },
    McpServers { limit: u32, used: u32 },
}

impl std::fmt::Display for QuotaExceeded {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DailyApiCalls { limit, used } => {
                write!(f, "Daily API call limit exceeded: {}/{}", used, limit)
            }
            Self::ActiveSessions { limit, used } => {
                write!(f, "Active session limit exceeded: {}/{}", used, limit)
            }
            Self::ActiveAgents { limit, used } => {
                write!(f, "Active agent limit exceeded: {}/{}", used, limit)
            }
            Self::McpServers { limit, used } => {
                write!(f, "MCP server limit exceeded: {}/{}", used, limit)
            }
        }
    }
}

impl std::error::Error for QuotaExceeded {}
