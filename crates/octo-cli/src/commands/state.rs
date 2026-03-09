//! Application state for Octo CLI

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use anyhow::Result;
use octo_engine::{AgentCatalog, AgentRuntime, AgentRuntimeConfig, AgentStore, TenantContext};
use octo_types::{TenantId, UserId};
use rusqlite::Connection;

/// Application state shared across commands
pub struct AppState {
    /// Database path
    #[allow(dead_code)]
    pub db_path: PathBuf,
    /// Agent catalog
    pub agent_catalog: Arc<AgentCatalog>,
    /// Agent runtime
    pub agent_runtime: Arc<AgentRuntime>,
}

impl AppState {
    /// Create new app state
    pub async fn new(db_path: PathBuf) -> Result<Self> {
        // Initialize AgentStore
        let agent_conn = {
            let raw = Connection::open(&db_path)?;
            Arc::new(Mutex::new(raw))
        };
        let agent_store = Arc::new(AgentStore::new(agent_conn)?);
        let agent_catalog = Arc::new(AgentCatalog::new().with_store(agent_store));
        let loaded = agent_catalog.load_from_store()?;
        tracing::info!("Loaded {loaded} persisted agents");

        // Create runtime config
        let runtime_config = AgentRuntimeConfig::from_parts(
            db_path.to_string_lossy().to_string(),
            octo_engine::providers::ProviderConfig::default(),
            vec![], // skills dirs
            None,   // provider chain
            None,   // working dir
            false,  // enable event bus
        );

        // Create tenant context
        let tenant_context = TenantContext::for_single_user(
            TenantId::from_string("default"),
            UserId::from_string("cli-user"),
        );

        // Create runtime
        let agent_runtime = Arc::new(
            AgentRuntime::new(agent_catalog.clone(), runtime_config, Some(tenant_context)).await?,
        );

        Ok(Self {
            db_path,
            agent_catalog,
            agent_runtime,
        })
    }
}
