mod api;
mod config;
mod middleware;
mod router;
mod session;
mod state;
mod ws;

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use tracing_subscriber::EnvFilter;

use octo_engine::{
    scheduler::{Scheduler, SqliteSchedulerStorage},
    AgentCatalog, AgentRuntime, AgentRuntimeConfig, AgentStore, Database, TenantContext,
};
use octo_types::{TenantId, UserId};
use state::AppState;

fn print_default_config() {
    println!("{}", config::Config::generate_default_yaml());
}

#[tokio::main]
async fn main() -> Result<()> {
    // Handle CLI arguments
    let args: Vec<String> = std::env::args().collect();
    let mut cli_port: Option<u16> = None;
    let mut cli_host: Option<&str> = None;
    let mut config_path: Option<PathBuf> = None;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "config-gen" | "gen-config" => {
                print_default_config();
                return Ok(());
            }
            "--port" | "-p" => {
                if i + 1 < args.len() {
                    cli_port = args[i + 1].parse().ok();
                    i += 2;
                    continue;
                }
            }
            "--host" | "-h" => {
                if i + 1 < args.len() {
                    cli_host = Some(&args[i + 1]);
                    i += 2;
                    continue;
                }
            }
            "--config" | "-c" => {
                if i + 1 < args.len() {
                    config_path = Some(PathBuf::from(&args[i + 1]));
                    i += 2;
                    continue;
                }
            }
            _ => {}
        }
        i += 1;
    }

    // Load .env FIRST (before config loading)
    dotenvy::dotenv_override().ok();

    // Load configuration: config.yaml < CLI args < .env
    let cfg = config::Config::load(config_path.as_ref(), cli_port, cli_host);

    // Apply logging config: OCTO_LOG > config.yaml > fallback info
    // Note: RUST_LOG from .env is intentionally ignored to avoid SSE debug noise.
    // Use OCTO_LOG=octo_engine=debug for explicit server-level debug logging.
    let log_filter = std::env::var("OCTO_LOG").unwrap_or_else(|_| cfg.logging.level.clone());
    let log_format = std::env::var("OCTO_LOG_FORMAT").unwrap_or_default();
    let env_filter = EnvFilter::new(&log_filter);

    if log_format == "json" {
        tracing_subscriber::fmt()
            .json()
            .with_env_filter(env_filter)
            .init();
    } else {
        tracing_subscriber::fmt()
            .with_env_filter(env_filter)
            .init();
    }

    let addr = format!("{}:{}", cfg.server.host, cfg.server.port);
    tracing::info!("Using provider: {}", cfg.provider.name);
    tracing::info!(
        auth_mode = ?cfg.auth.mode,
        api_key_count = cfg.auth.api_keys.as_ref().map(|k| k.len()).unwrap_or(0),
        "Auth configuration loaded"
    );

    // Discover OctoRoot for unified path management
    let octo_root = octo_engine::OctoRoot::discover()
        .unwrap_or_else(|e| {
            tracing::warn!("Failed to discover OctoRoot: {}, using defaults", e);
            let wd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
            octo_engine::OctoRoot::with_working_dir(&wd).expect("OctoRoot fallback failed")
        });
    if let Err(e) = octo_root.ensure_dirs() {
        tracing::warn!("Failed to ensure OctoRoot directories: {}", e);
    }

    // Database: SQLite with WAL mode
    // If config has non-empty path, use it; otherwise resolve from OctoRoot
    let db_path = if cfg.database.path.is_empty() {
        octo_root.resolve_db_path().to_string_lossy().to_string()
    } else {
        cfg.database.path.clone()
    };

    // Agent system: catalog + supervisor
    // AgentStore uses a synchronous rusqlite connection (separate from the async tokio-rusqlite conn)
    let agent_conn = {
        use std::sync::Mutex;
        let raw = rusqlite::Connection::open(&db_path).expect("failed to open agent DB connection");
        Arc::new(Mutex::new(raw))
    };
    let agent_store = Arc::new(AgentStore::new(agent_conn).expect("failed to init AgentStore"));
    let agent_catalog = Arc::new(AgentCatalog::new().with_store(agent_store));
    let loaded = agent_catalog.load_from_store().unwrap_or(0);
    tracing::info!("Loaded {loaded} persisted agents");

    // Create AgentRuntime with all components internalized
    // AgentRuntime::new() creates: WorkingMemory, SessionStore, MemoryStore,
    // ToolRegistry, SkillRegistry, Provider, ProviderChain internally
    let runtime_config = AgentRuntimeConfig::from_parts(
        db_path.clone(),
        cfg.provider.clone(),
        cfg.skills.dirs.clone(),
        cfg.provider_chain.clone(),
        cfg.working_dir.clone().map(PathBuf::from),
        cfg.enable_event_bus,
    )
    .with_octo_root(octo_root);

    // Initialize provider chain if configured (before creating AgentRuntime)
    // Note: instances need to be added separately after AgentRuntime creation
    if let Some(ref pc_config) = cfg.provider_chain {
        let chain = Arc::new(octo_engine::providers::ProviderChain::new(
            pc_config.failover_policy,
        ));
        let chain_clone = Arc::clone(&chain);

        // Add instances to the chain
        for instance_config in &pc_config.instances {
            // Resolve API key from env var if needed
            let api_key = if instance_config.api_key.starts_with("${")
                && instance_config.api_key.ends_with("}")
            {
                let env_var = &instance_config.api_key[2..instance_config.api_key.len() - 1];
                std::env::var(env_var).unwrap_or_else(|_| instance_config.api_key.clone())
            } else {
                instance_config.api_key.clone()
            };

            let instance = octo_engine::providers::LlmInstance {
                id: instance_config.id.clone(),
                provider: instance_config.provider.clone(),
                api_key,
                base_url: instance_config.base_url.clone(),
                model: instance_config.model.clone(),
                priority: instance_config.priority,
                max_rpm: instance_config.max_rpm,
                enabled: instance_config.enabled,
            };
            // Note: add_instance takes &self, so we need Arc<ProviderChain>
            chain_clone.add_instance(instance).await;
        }

        // Start health checker if configured
        chain
            .start_health_checker(octo_engine::providers::HealthCheckConfig {
                interval: std::time::Duration::from_secs(pc_config.health_check_interval_sec),
                timeout: std::time::Duration::from_secs(10),
            })
            .await;

        tracing::info!(
            "Provider chain initialized with {} instances",
            pc_config.instances.len()
        );
    }

    // Create tenant context for single-user scenario (octo-workbench)
    let tenant_context = TenantContext::for_single_user(
        TenantId::from_string("default"),
        UserId::from_string("local-user"),
    );

    let agent_runtime = Arc::new(
        AgentRuntime::new(agent_catalog.clone(), runtime_config, Some(tenant_context)).await?,
    );

    // Get session store from AgentRuntime for creating primary session
    let session_store = agent_runtime.session_store();

    // 创建主 session 并预热主 Runtime
    let primary_session = session_store.create_session().await;
    let primary_history = session_store
        .get_messages(&primary_session.session_id)
        .await
        .unwrap_or_default();
    let primary_agent_id = agent_catalog.list_all().into_iter().next().map(|e| e.id);
    let agent_handle = agent_runtime
        .start_primary(
            primary_session.session_id.clone(),
            primary_session.user_id.clone(),
            primary_session.sandbox_id.clone(),
            primary_history,
            primary_agent_id.as_ref(),
        )
        .await;
    tracing::info!(
        session_id = %primary_session.session_id.as_str(),
        "Primary AgentExecutor started"
    );

    // Open a separate database connection for scheduler (it needs its own connection)
    let db = Database::open(&db_path).await?;
    let conn = db.conn().clone();

    // Create scheduler with required dependencies from agent_runtime
    let scheduler = if cfg.scheduler.enabled {
        tracing::info!(
            "Scheduler enabled: interval={}s, max_concurrent={}",
            cfg.scheduler.check_interval_secs,
            cfg.scheduler.max_concurrent
        );
        let storage = SqliteSchedulerStorage::new(conn.clone());
        let s = Scheduler::new(
            cfg.scheduler.clone(),
            Arc::new(storage),
            agent_runtime.provider().clone(),
            agent_runtime.tools().clone(),
            agent_runtime.memory().clone(),
            agent_runtime.session_store().clone(),
            Some(agent_runtime.security_policy().clone()
                as std::sync::Arc<dyn octo_types::PathValidator>),
        );
        Some(Arc::new(s))
    } else {
        None
    };

    let state = Arc::new(AppState::new(
        std::path::PathBuf::from(&db_path),
        scheduler.clone(),
        cfg.clone(),
        agent_runtime,
        agent_handle,
    ));

    // Start scheduler loop
    if let Some(ref sched) = scheduler {
        let sched = sched.clone();
        tokio::spawn(async move {
            sched.start().await;
        });
    }

    // Spawn memory TTL cleanup task (hourly)
    {
        let runtime_for_ttl = state.agent_supervisor.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(3600));
            loop {
                interval.tick().await;
                match runtime_for_ttl.cleanup_expired_memories().await {
                    Ok(n) if n > 0 => {
                        tracing::info!(deleted = n, "TTL cleanup: removed expired memories")
                    }
                    Ok(_) => tracing::debug!("TTL cleanup: no expired memories"),
                    Err(e) => tracing::warn!(error = %e, "TTL cleanup failed"),
                }
            }
        });
    }

    let app = router::build_router(state.clone());

    #[cfg(feature = "tls")]
    if cfg.tls.enabled {
        use axum_server::tls_rustls::RustlsConfig;

        let (cert_path, key_path) = if cfg.tls.self_signed {
            let tls_dir = cfg
                .tls
                .self_signed_dir
                .clone()
                .unwrap_or_else(|| PathBuf::from("./data/tls"));
            tracing::info!("Generating self-signed TLS certificate in {:?}", tls_dir);
            octo_engine::tls::generate_self_signed_cert("localhost", &tls_dir)?
        } else {
            (
                cfg.tls
                    .cert_path
                    .clone()
                    .expect("TLS cert_path required when tls.enabled=true and self_signed=false"),
                cfg.tls
                    .key_path
                    .clone()
                    .expect("TLS key_path required when tls.enabled=true and self_signed=false"),
            )
        };

        let tls_config = RustlsConfig::from_pem_file(&cert_path, &key_path).await?;
        tracing::info!("octo-server listening on https://{addr} (TLS enabled)");
        axum_server::bind_rustls(addr.parse()?, tls_config)
            .serve(app.into_make_service())
            .await?;
    } else {
        let listener = tokio::net::TcpListener::bind(&addr).await?;
        tracing::info!("octo-server listening on {addr}");
        axum::serve(listener, app)
            .with_graceful_shutdown(shutdown_signal(state.clone()))
            .await?;
    }

    #[cfg(not(feature = "tls"))]
    {
        if cfg.tls.enabled {
            tracing::warn!(
                "TLS is enabled in config but the 'tls' feature is not compiled in. \
                 Rebuild with: cargo build -p octo-server --features tls"
            );
        }
        let listener = tokio::net::TcpListener::bind(&addr).await?;
        tracing::info!("octo-server listening on {addr}");
        axum::serve(listener, app)
            .with_graceful_shutdown(shutdown_signal(state.clone()))
            .await?;
    }

    // Graceful shutdown: clean up MCP servers with timeout
    tracing::info!("Shutting down MCP servers...");
    let cleanup = async {
        let mcp_manager = state.agent_supervisor.mcp_manager();
        let mut guard = mcp_manager.lock().await;
        let _ = guard.shutdown_all().await;
    };
    tokio::select! {
        _ = cleanup => tracing::info!("MCP servers shut down"),
        _ = tokio::time::sleep(std::time::Duration::from_secs(3)) => {
            tracing::warn!("MCP shutdown timed out, forcing exit");
        }
    }

    Ok(())
}

async fn shutdown_signal(state: Arc<AppState>) {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install CTRL+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }

    tracing::info!("shutdown signal received, starting graceful shutdown...");
    tracing::info!("Press Ctrl+C again to force quit immediately.");

    // Spawn a force-exit guard: if graceful shutdown takes too long or user presses Ctrl+C again
    tokio::spawn(async {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                tracing::warn!("Force quit requested");
            }
            _ = tokio::time::sleep(std::time::Duration::from_secs(5)) => {
                tracing::warn!("Graceful shutdown timed out after 5s");
            }
        }
        std::process::exit(0);
    });

    // Stop scheduler if running
    if let Some(ref sched) = state.scheduler {
        tracing::info!("Stopping scheduler...");
        sched.stop();
    }
}
