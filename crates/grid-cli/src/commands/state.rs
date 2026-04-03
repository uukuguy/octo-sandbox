//! Application state for Grid CLI

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use anyhow::Result;
use grid_engine::{AgentCatalog, AgentRuntime, AgentRuntimeConfig, AgentStore, GridRoot, TenantContext};
use grid_types::{TenantId, UserId};
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
    /// GridRoot for unified path management
    pub grid_root: GridRoot,
}

impl AppState {
    /// Create new app state with GridRoot
    pub async fn new(db_path: PathBuf, output_config: OutputConfig) -> Result<Self> {
        let grid_root = GridRoot::discover()?;
        Self::with_grid_root(db_path, output_config, grid_root).await
    }

    /// Create new app state with an explicit GridRoot
    pub async fn with_grid_root(
        db_path: PathBuf,
        output_config: OutputConfig,
        grid_root: GridRoot,
    ) -> Result<Self> {
        let working_dir = grid_root.working_dir().to_path_buf();

        // Initialize AgentStore
        let agent_conn = {
            let raw = Connection::open(&db_path)?;
            Arc::new(Mutex::new(raw))
        };
        let agent_store = Arc::new(AgentStore::new(agent_conn)?);
        let agent_catalog = Arc::new(AgentCatalog::new().with_store(agent_store));
        let loaded = agent_catalog.load_from_store()?;
        tracing::info!("Loaded {loaded} persisted agents");

        let skills_dirs: Vec<String> = grid_root
            .skills_dirs()
            .iter()
            .map(|p| p.to_string_lossy().to_string())
            .collect();

        let runtime_config = AgentRuntimeConfig::from_parts(
            db_path.to_string_lossy().to_string(),
            grid_engine::providers::ProviderConfig::default(),
            skills_dirs,
            None,
            Some(working_dir.clone()),
            false,
        )
        .with_grid_root(grid_root.clone());

        let tenant_context = TenantContext::for_single_user(
            TenantId::from_string("default"),
            UserId::from_string(grid_types::id::DEFAULT_USER_ID),
        );

        let agent_runtime = Arc::new(
            AgentRuntime::new(agent_catalog.clone(), runtime_config, Some(tenant_context)).await?,
        );
        agent_runtime.register_session_create_tool();

        Ok(Self {
            db_path,
            agent_catalog,
            agent_runtime,
            output_config,
            working_dir,
            grid_root,
        })
    }
}
