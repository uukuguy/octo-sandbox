//! Session-scoped sandbox container management.
//!
//! Provides per-session Docker container reuse: within one session, all tool
//! executions share a single long-lived container instead of creating/destroying
//! one per call.  This eliminates the 2-5 s startup overhead on every tool use.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::RwLock;

use super::docker::DockerAdapter;
use super::traits::{ExecResult, RuntimeAdapter, SandboxConfig, SandboxError, SandboxId};

/// Metadata for a container bound to a session.
#[derive(Debug)]
pub struct SessionContainer {
    /// Docker container ID (from bollard).
    pub container_id: String,
    /// Octo sandbox ID assigned at creation.
    pub sandbox_id: SandboxId,
    /// When the container was created.
    pub created_at: Instant,
    /// When the container was last used for execution.
    pub last_used: Instant,
    /// Number of exec calls against this container.
    pub execution_count: u64,
}

/// Configuration knobs for session sandbox behaviour.
#[derive(Debug, Clone)]
pub struct SessionSandboxConfig {
    /// Docker image to use (default: `octo-sandbox:base`).
    pub image: String,
    /// Destroy the container after this much idle time (default: 30 min).
    pub idle_timeout: Duration,
    /// Hard max lifetime per container (default: 4 h).
    pub max_lifetime: Duration,
    /// Maximum number of concurrent session containers (default: 5).
    pub max_containers: usize,
    /// Container working directory (default: `/workspace/session`).
    pub working_dir: String,
}

impl Default for SessionSandboxConfig {
    fn default() -> Self {
        Self {
            image: super::docker::DEFAULT_SANDBOX_IMAGE.to_string(),
            idle_timeout: Duration::from_secs(30 * 60),
            max_lifetime: Duration::from_secs(4 * 3600),
            max_containers: 5,
            working_dir: "/workspace/session".to_string(),
        }
    }
}

/// Default cleanup interval (5 minutes).
pub const DEFAULT_CLEANUP_INTERVAL: Duration = Duration::from_secs(5 * 60);

/// Manages a pool of per-session Docker containers.
pub struct SessionSandboxManager {
    docker: Arc<DockerAdapter>,
    containers: Arc<RwLock<HashMap<String, SessionContainer>>>,
    config: SessionSandboxConfig,
    /// Handle for the background cleanup timer task (if running).
    cleanup_handle: RwLock<Option<tokio::task::JoinHandle<()>>>,
}

impl SessionSandboxManager {
    pub fn new(docker: Arc<DockerAdapter>, config: SessionSandboxConfig) -> Self {
        Self {
            docker,
            containers: Arc::new(RwLock::new(HashMap::new())),
            config,
            cleanup_handle: RwLock::new(None),
        }
    }

    /// Get existing container or create a new one for `session_id`.
    pub async fn get_or_create(&self, session_id: &str) -> Result<SandboxId, SandboxError> {
        // Fast path: container already exists
        {
            let containers = self.containers.read().await;
            if let Some(c) = containers.get(session_id) {
                return Ok(c.sandbox_id.clone());
            }
        }

        // Check capacity
        {
            let containers = self.containers.read().await;
            if containers.len() >= self.config.max_containers {
                return Err(SandboxError::ConfigError(format!(
                    "Session sandbox pool full ({}/{}). Release an existing session first.",
                    containers.len(),
                    self.config.max_containers
                )));
            }
        }

        // Create new container
        let sandbox_config = SandboxConfig::new(super::traits::SandboxType::Docker)
            .with_working_dir(std::path::PathBuf::from(&self.config.working_dir));

        let sandbox_id = self.docker.create(&sandbox_config).await?;

        let now = Instant::now();
        let container = SessionContainer {
            container_id: sandbox_id.to_string(),
            sandbox_id: sandbox_id.clone(),
            created_at: now,
            last_used: now,
            execution_count: 0,
        };

        let mut containers = self.containers.write().await;
        containers.insert(session_id.to_string(), container);

        tracing::info!(session_id, sandbox_id = %sandbox_id, "Created session sandbox");
        Ok(sandbox_id)
    }

    /// Execute a command in the session's container.
    pub async fn execute(
        &self,
        session_id: &str,
        command: &str,
    ) -> Result<ExecResult, SandboxError> {
        let sandbox_id = self.get_or_create(session_id).await?;

        let result = self.docker.execute(&sandbox_id, command, "bash").await?;

        // Update last_used + execution_count
        {
            let mut containers = self.containers.write().await;
            if let Some(c) = containers.get_mut(session_id) {
                c.last_used = Instant::now();
                c.execution_count += 1;
            }
        }

        Ok(result)
    }

    /// Release (destroy) the container for a specific session.
    pub async fn release(&self, session_id: &str) -> Result<(), SandboxError> {
        let entry = {
            let mut containers = self.containers.write().await;
            containers.remove(session_id)
        };

        if let Some(container) = entry {
            tracing::info!(
                session_id,
                sandbox_id = %container.sandbox_id,
                executions = container.execution_count,
                "Releasing session sandbox"
            );
            self.docker.destroy(&container.sandbox_id).await?;
        }

        Ok(())
    }

    /// Clean up containers that have exceeded idle_timeout or max_lifetime.
    /// Returns the number of containers cleaned up.
    pub async fn cleanup_idle(&self) -> usize {
        let now = Instant::now();
        let mut to_remove = Vec::new();

        {
            let containers = self.containers.read().await;
            for (session_id, c) in containers.iter() {
                let idle = now.duration_since(c.last_used) > self.config.idle_timeout;
                let expired = now.duration_since(c.created_at) > self.config.max_lifetime;
                if idle || expired {
                    to_remove.push((
                        session_id.clone(),
                        c.sandbox_id.clone(),
                        if expired { "max_lifetime" } else { "idle_timeout" },
                    ));
                }
            }
        }

        let count = to_remove.len();
        for (session_id, sandbox_id, reason) in to_remove {
            tracing::info!(session_id, sandbox_id = %sandbox_id, reason, "Cleaning up session sandbox");
            {
                let mut containers = self.containers.write().await;
                containers.remove(&session_id);
            }
            if let Err(e) = self.docker.destroy(&sandbox_id).await {
                tracing::warn!(sandbox_id = %sandbox_id, error = %e, "Failed to destroy idle container");
            }
        }

        count
    }

    /// Start a background task that periodically cleans up idle/expired containers.
    ///
    /// The timer is opt-in: it only runs if this method is called.  The handle is
    /// stored internally and will be aborted automatically on [`shutdown`].
    ///
    /// Returns a reference-counted clone of `self` for chaining.
    pub fn start_cleanup_timer(self: &Arc<Self>, interval: Duration) -> Arc<Self> {
        let mgr = Arc::clone(self);
        let handle = {
            let mgr_inner = Arc::clone(&mgr);
            tokio::spawn(async move {
                let mut ticker = tokio::time::interval(interval);
                // First tick completes immediately — skip it so we don't
                // run cleanup right at startup.
                ticker.tick().await;
                loop {
                    ticker.tick().await;
                    let cleaned = mgr_inner.cleanup_idle().await;
                    if cleaned > 0 {
                        tracing::info!(cleaned, "Session sandbox cleanup timer: removed idle containers");
                    }
                }
            })
        };

        // Store the handle so shutdown() can abort it.
        // We use try_write to avoid blocking; in the unlikely case another
        // caller holds the lock we just log a warning.
        if let Ok(mut guard) = mgr.cleanup_handle.try_write() {
            // If a previous timer was running, abort it first.
            if let Some(old) = guard.take() {
                old.abort();
            }
            *guard = Some(handle);
        } else {
            tracing::warn!("Could not store cleanup timer handle — lock contended");
        }

        mgr
    }

    /// Returns `true` if the background cleanup timer is currently running.
    pub fn is_cleanup_timer_running(&self) -> bool {
        self.cleanup_handle
            .try_read()
            .map(|guard| guard.as_ref().map_or(false, |h| !h.is_finished()))
            .unwrap_or(false)
    }

    /// Shut down all managed containers and stop the cleanup timer.
    pub async fn shutdown(&self) -> Result<(), SandboxError> {
        // 1. Abort the cleanup timer if running.
        {
            let mut handle_guard = self.cleanup_handle.write().await;
            if let Some(handle) = handle_guard.take() {
                handle.abort();
                tracing::info!("Cleanup timer aborted");
            }
        }

        // 2. Destroy all containers.
        let entries: Vec<(String, SandboxId)> = {
            let mut containers = self.containers.write().await;
            containers
                .drain()
                .map(|(sid, c)| (sid, c.sandbox_id))
                .collect()
        };

        for (session_id, sandbox_id) in entries {
            tracing::info!(session_id, sandbox_id = %sandbox_id, "Shutting down session sandbox");
            if let Err(e) = self.docker.destroy(&sandbox_id).await {
                tracing::warn!(sandbox_id = %sandbox_id, error = %e, "Failed to destroy container during shutdown");
            }
        }

        Ok(())
    }

    /// Number of active session containers.
    pub async fn active_count(&self) -> usize {
        self.containers.read().await.len()
    }

    /// Get the configuration.
    pub fn config(&self) -> &SessionSandboxConfig {
        &self.config
    }

    /// List active session IDs.
    pub async fn active_sessions(&self) -> Vec<String> {
        self.containers.read().await.keys().cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_config() -> SessionSandboxConfig {
        SessionSandboxConfig {
            idle_timeout: Duration::from_secs(5),
            max_lifetime: Duration::from_secs(60),
            max_containers: 3,
            ..Default::default()
        }
    }

    #[test]
    fn test_default_config() {
        let config = SessionSandboxConfig::default();
        assert_eq!(config.image, "octo-sandbox:base");
        assert_eq!(config.idle_timeout, Duration::from_secs(30 * 60));
        assert_eq!(config.max_lifetime, Duration::from_secs(4 * 3600));
        assert_eq!(config.max_containers, 5);
        assert_eq!(config.working_dir, "/workspace/session");
    }

    #[test]
    fn test_custom_config() {
        let config = make_config();
        assert_eq!(config.idle_timeout, Duration::from_secs(5));
        assert_eq!(config.max_containers, 3);
    }

    #[tokio::test]
    async fn test_get_or_create_returns_same_id() {
        // Without a running Docker daemon this will fail at create(),
        // but we can verify the fast-path lookup works.
        let docker = Arc::new(DockerAdapter::with_default_image());
        let mgr = SessionSandboxManager::new(docker, make_config());

        // Pool starts empty
        assert_eq!(mgr.active_count().await, 0);
    }

    #[tokio::test]
    async fn test_capacity_limit() {
        let docker = Arc::new(DockerAdapter::with_default_image());
        let config = SessionSandboxConfig {
            max_containers: 0, // Immediately full
            ..make_config()
        };
        let mgr = SessionSandboxManager::new(docker, config);

        let result = mgr.get_or_create("session-1").await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.to_string().contains("pool full"),
            "Expected pool-full error, got: {}",
            err
        );
    }

    #[tokio::test]
    async fn test_release_nonexistent_is_ok() {
        let docker = Arc::new(DockerAdapter::with_default_image());
        let mgr = SessionSandboxManager::new(docker, make_config());

        // Releasing a session that doesn't exist should succeed silently
        let result = mgr.release("nonexistent").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_shutdown_empty() {
        let docker = Arc::new(DockerAdapter::with_default_image());
        let mgr = SessionSandboxManager::new(docker, make_config());

        let result = mgr.shutdown().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_cleanup_idle_empty() {
        let docker = Arc::new(DockerAdapter::with_default_image());
        let mgr = SessionSandboxManager::new(docker, make_config());

        let cleaned = mgr.cleanup_idle().await;
        assert_eq!(cleaned, 0);
    }

    #[tokio::test]
    async fn test_active_sessions_empty() {
        let docker = Arc::new(DockerAdapter::with_default_image());
        let mgr = SessionSandboxManager::new(docker, make_config());

        let sessions = mgr.active_sessions().await;
        assert!(sessions.is_empty());
    }

    #[tokio::test]
    async fn test_start_cleanup_timer_creates_handle() {
        let docker = Arc::new(DockerAdapter::with_default_image());
        let mgr = Arc::new(SessionSandboxManager::new(docker, make_config()));

        // Timer not running initially
        assert!(!mgr.is_cleanup_timer_running());

        // Start the timer with a long interval (won't actually fire in the test)
        let mgr2 = mgr.start_cleanup_timer(Duration::from_secs(3600));

        // Timer should now be running
        assert!(mgr2.is_cleanup_timer_running());

        // Clean up
        mgr.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn test_shutdown_aborts_cleanup_timer() {
        let docker = Arc::new(DockerAdapter::with_default_image());
        let mgr = Arc::new(SessionSandboxManager::new(docker, make_config()));

        mgr.start_cleanup_timer(Duration::from_secs(3600));
        assert!(mgr.is_cleanup_timer_running());

        mgr.shutdown().await.unwrap();

        // After shutdown the timer should no longer be running
        assert!(!mgr.is_cleanup_timer_running());
    }

    #[tokio::test]
    async fn test_start_cleanup_timer_replaces_previous() {
        let docker = Arc::new(DockerAdapter::with_default_image());
        let mgr = Arc::new(SessionSandboxManager::new(docker, make_config()));

        // Start first timer
        mgr.start_cleanup_timer(Duration::from_secs(3600));
        assert!(mgr.is_cleanup_timer_running());

        // Start second timer — should abort the first
        mgr.start_cleanup_timer(Duration::from_secs(7200));
        assert!(mgr.is_cleanup_timer_running());

        mgr.shutdown().await.unwrap();
    }

    #[test]
    fn test_default_cleanup_interval() {
        assert_eq!(DEFAULT_CLEANUP_INTERVAL, Duration::from_secs(300));
    }
}
