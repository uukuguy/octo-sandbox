//! Application state for Octo CLI

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use anyhow::Result;
use octo_engine::{AgentCatalog, AgentRuntime, AgentRuntimeConfig, AgentStore, OctoRoot, TenantContext};
use octo_types::{TenantId, UserId};
use rusqlite::Connection;

use crate::output::OutputConfig;

/// Application state shared across commands
pub struct AppState {
    /// Database path
    #[allow(dead_code)]
    pub db_path: PathBuf,
    /// Agent catalog
    pub agent_catalog: Arc<AgentCatalog>,
    /// Agent runtime
    pub agent_runtime: Arc<AgentRuntime>,
    /// Output configuration
    pub output_config: OutputConfig,
    /// Working directory
    pub working_dir: PathBuf,
    /// OctoRoot for unified path management
    pub octo_root: OctoRoot,
}

impl AppState {
    /// Create new app state with OctoRoot
    pub async fn new(db_path: PathBuf, output_config: OutputConfig) -> Result<Self> {
        // Discover OctoRoot (caller may have already done this, but it's cheap)
        let octo_root = OctoRoot::discover()?;
        Self::with_octo_root(db_path, output_config, octo_root).await
    }

    /// Create new app state with an explicit OctoRoot
    pub async fn with_octo_root(
        db_path: PathBuf,
        output_config: OutputConfig,
        octo_root: OctoRoot,
    ) -> Result<Self> {
        let working_dir = octo_root.working_dir().to_path_buf();

        // Initialize AgentStore
        let agent_conn = {
            let raw = Connection::open(&db_path)?;
            Arc::new(Mutex::new(raw))
        };
        let agent_store = Arc::new(AgentStore::new(agent_conn)?);
        let agent_catalog = Arc::new(AgentCatalog::new().with_store(agent_store));
        let loaded = agent_catalog.load_from_store()?;
        tracing::info!("Loaded {loaded} persisted agents");

        // Create runtime config with OctoRoot — fixes skills_dirs: vec![] BUG
        let skills_dirs: Vec<String> = octo_root
            .skills_dirs()
            .iter()
            .map(|p| p.to_string_lossy().to_string())
            .collect();

        let runtime_config = AgentRuntimeConfig::from_parts(
            db_path.to_string_lossy().to_string(),
            octo_engine::providers::ProviderConfig::default(),
            skills_dirs,
            None,   // provider chain
            Some(working_dir.clone()),
            false,  // enable event bus
        )
        .with_octo_root(octo_root.clone());

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
            output_config,
            working_dir,
            octo_root,
        })
    }
}
