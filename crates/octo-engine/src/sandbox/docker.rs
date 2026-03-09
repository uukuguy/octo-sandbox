//! Docker container sandbox adapter using bollard
//!
//! This adapter provides Docker-based sandbox execution using the bollard Docker API.
//! It is feature-gated behind the `sandbox-docker` feature flag.

use super::{ExecResult, RuntimeAdapter, SandboxConfig, SandboxError, SandboxId, SandboxType};
use std::collections::HashMap;

#[cfg(feature = "sandbox-docker")]
use std::sync::Arc;
#[cfg(feature = "sandbox-docker")]
use tokio::sync::RwLock;

/// Docker container sandbox adapter
///
/// This adapter executes code inside Docker containers for isolation.
/// It requires the `sandbox-docker` feature to be enabled.
pub struct DockerAdapter {
    /// Active Docker container instances
    #[cfg(feature = "sandbox-docker")]
    instances: Arc<RwLock<HashMap<SandboxId, DockerInstance>>>,

    /// Stub instances for when feature is not enabled
    #[cfg(not(feature = "sandbox-docker"))]
    _instances: PhantomData<HashMap<SandboxId, ()>>,

    /// Default Docker image to use
    image: String,

    /// Docker client (only available with sandbox-docker feature)
    #[cfg(feature = "sandbox-docker")]
    client: Option<bollard::Docker>,
}

/// Internal representation of a Docker sandbox instance
#[cfg(feature = "sandbox-docker")]
struct DockerInstance {
    /// Sandbox configuration (stored for future use)
    #[allow(dead_code)]
    config: SandboxConfig,
    /// Docker container ID
    container_id: String,
}

impl DockerAdapter {
    /// Create a new DockerAdapter with the specified default image
    pub fn new(image: impl Into<String>) -> Self {
        #[cfg(feature = "sandbox-docker")]
        let client = DockerAdapter::create_client().ok();

        Self {
            #[cfg(feature = "sandbox-docker")]
            instances: Arc::new(RwLock::new(HashMap::new())),
            #[cfg(not(feature = "sandbox-docker"))]
            _instances: PhantomData,
            image: image.into(),
            #[cfg(feature = "sandbox-docker")]
            client,
        }
    }

    /// Create a new DockerAdapter with default image "alpine:latest"
    pub fn with_default_image() -> Self {
        Self::new("alpine:latest")
    }

    /// Create Docker client
    #[cfg(feature = "sandbox-docker")]
    fn create_client() -> Result<bollard::Docker, SandboxError> {
        use bollard::Docker;

        // Try to connect to Docker daemon
        Docker::connect_with_local_defaults().map_err(|e| {
            SandboxError::ExecutionFailed(format!("Failed to connect to Docker: {}", e))
        })
    }

    /// Check if Docker support is available
    pub fn is_available(&self) -> bool {
        #[cfg(feature = "sandbox-docker")]
        return self.client.is_some();

        #[cfg(not(feature = "sandbox-docker"))]
        return false;
    }

    /// Get the default image
    pub fn image(&self) -> &str {
        &self.image
    }

    /// Pull an image from Docker Hub (helper method)
    #[cfg(feature = "sandbox-docker")]
    pub async fn pull_image(&self) -> Result<(), SandboxError> {
        use bollard::image::CreateImageOptions;
        use futures_util::StreamExt;

        let client = self
            .client
            .as_ref()
            .ok_or_else(|| SandboxError::ExecutionFailed("Docker client not initialized".into()))?;

        tracing::info!("Pulling Docker image: {}", self.image);

        let mut stream = client.create_image(
            Some(CreateImageOptions {
                from_image: self.image.as_str(),
                ..Default::default()
            }),
            None,
            None,
        );

        while let Some(result) = stream.next().await {
            match result {
                Ok(info) => {
                    if let Some(status) = info.status {
                        tracing::debug!("Pull status: {}", status);
                    }
                }
                Err(e) => {
                    return Err(SandboxError::ExecutionFailed(format!(
                        "Failed to pull image: {}",
                        e
                    )));
                }
            }
        }

        tracing::info!("Successfully pulled image: {}", self.image);
        Ok(())
    }

    /// Pull an image (stub without feature)
    #[cfg(not(feature = "sandbox-docker"))]
    pub async fn pull_image(&self) -> Result<(), SandboxError> {
        let _ = self;
        Err(SandboxError::UnsupportedType(
            "Docker support not enabled. Enable sandbox-docker feature".to_string(),
        ))
    }
}

impl Default for DockerAdapter {
    fn default() -> Self {
        Self::with_default_image()
    }
}

impl RuntimeAdapter for DockerAdapter {
    /// Get the sandbox type
    fn sandbox_type(&self) -> SandboxType {
        SandboxType::Docker
    }

    /// Create a new Docker sandbox instance
    async fn create(&self, config: &SandboxConfig) -> Result<SandboxId, SandboxError> {
        #[cfg(not(feature = "sandbox-docker"))]
        {
            let _ = config;
            return Err(SandboxError::UnsupportedType(
                "Docker support not enabled. Enable sandbox-docker feature".to_string(),
            ));
        }

        #[cfg(feature = "sandbox-docker")]
        {
            let client = self.client.as_ref().ok_or_else(|| {
                SandboxError::ExecutionFailed("Docker client not initialized".into())
            })?;

            let id = SandboxId::new(uuid::Uuid::new_v4().to_string());

            // Use configured image or default
            let image = config
                .env
                .get("DOCKER_IMAGE")
                .map(|s| s.as_str())
                .unwrap_or(&self.image);

            // Create container
            let container_id = create_container(client, image, &id, config).await?;

            let instance = DockerInstance {
                config: config.clone(),
                container_id,
            };

            let mut instances = self.instances.write().await;
            instances.insert(id.clone(), instance);

            tracing::debug!("Created Docker sandbox: {}", id);
            Ok(id)
        }
    }

    /// Execute code in the Docker sandbox
    async fn execute(
        &self,
        id: &SandboxId,
        code: &str,
        _language: &str,
    ) -> Result<ExecResult, SandboxError> {
        #[cfg(not(feature = "sandbox-docker"))]
        {
            let _ = (id, code);
            return Err(SandboxError::UnsupportedType(
                "Docker support not enabled. Enable sandbox-docker feature".to_string(),
            ));
        }

        #[cfg(feature = "sandbox-docker")]
        {
            let instances = self.instances.read().await;

            // Check if sandbox exists
            let instance = instances
                .get(id)
                .ok_or_else(|| SandboxError::NotFound(id.clone()))?;

            let container_id = instance.container_id.clone();
            drop(instances);

            let client = self.client.as_ref().ok_or_else(|| {
                SandboxError::ExecutionFailed("Docker client not initialized".into())
            })?;

            execute_in_container(client, &container_id, code).await
        }
    }

    /// Destroy a Docker sandbox instance
    async fn destroy(&self, id: &SandboxId) -> Result<(), SandboxError> {
        #[cfg(not(feature = "sandbox-docker"))]
        {
            let _ = id;
            return Err(SandboxError::UnsupportedType(
                "Docker support not enabled. Enable sandbox-docker feature".to_string(),
            ));
        }

        #[cfg(feature = "sandbox-docker")]
        {
            let mut instances = self.instances.write().await;

            if let Some(instance) = instances.remove(id) {
                let client = match &self.client {
                    Some(c) => c,
                    None => {
                        return Err(SandboxError::ExecutionFailed(
                            "Docker client not initialized".to_string(),
                        ));
                    }
                };

                // Stop and remove container
                stop_container(client, &instance.container_id).await?;
                remove_container(client, &instance.container_id).await?;

                tracing::debug!("Destroyed Docker sandbox: {}", id);
            }

            Ok(())
        }
    }

    /// Check if the sandbox is ready
    async fn is_ready(&self) -> bool {
        #[cfg(feature = "sandbox-docker")]
        return self.client.is_some();

        #[cfg(not(feature = "sandbox-docker"))]
        return false;
    }
}

/// Create a Docker container
#[cfg(feature = "sandbox-docker")]
async fn create_container(
    client: &bollard::Docker,
    image: &str,
    id: &SandboxId,
    config: &SandboxConfig,
) -> Result<String, SandboxError> {
    use bollard::container::{Config, CreateContainerOptions};
    use bollard::image::CreateImageOptions;
    use futures_util::StreamExt;
    use std::collections::HashMap as StdHashMap;

    // Try to pull image first (will skip if already exists)
    let mut stream = client.create_image(
        Some(CreateImageOptions {
            from_image: image,
            ..Default::default()
        }),
        None,
        None,
    );

    while let Some(result) = stream.next().await {
        if let Err(e) = result {
            tracing::warn!("Failed to pull image (may already exist): {}", e);
            break;
        }
    }

    // Container configuration - build environment variables
    let mut env_vars: Vec<String> = Vec::new();
    for (key, value) in &config.env {
        env_vars.push(format!("{}={}", key, value));
    }

    // Build labels
    let mut labels = StdHashMap::new();
    labels.insert("octo-sandbox".to_string(), "true".to_string());
    labels.insert("sandbox-id".to_string(), id.to_string());

    // Container config - keep the container alive with a sleeping shell
    // Alpine exits immediately without a blocking process; `sh` + tty keeps it running
    let container_config = Config {
        image: Some(image.to_string()),
        env: Some(env_vars),
        labels: Some(labels),
        cmd: Some(vec!["sh".to_string()]),
        tty: Some(true),
        open_stdin: Some(true),
        ..Default::default()
    };

    let options = CreateContainerOptions {
        name: format!("octo-sandbox-{}", id),
        platform: None,
    };

    let response = client
        .create_container(Some(options), container_config)
        .await
        .map_err(|e| SandboxError::ExecutionFailed(format!("Failed to create container: {}", e)))?;

    // Start the container
    client
        .start_container::<String>(&response.id, None)
        .await
        .map_err(|e| SandboxError::ExecutionFailed(format!("Failed to start container: {}", e)))?;

    tracing::debug!("Created and started Docker container: {}", response.id);
    Ok(response.id)
}

/// Execute a command in a running Docker container using exec API
#[cfg(feature = "sandbox-docker")]
async fn execute_in_container(
    client: &bollard::Docker,
    container_id: &str,
    code: &str,
) -> Result<ExecResult, SandboxError> {
    use bollard::exec::{CreateExecOptions, StartExecResults};
    use futures_util::StreamExt;

    let start = std::time::Instant::now();

    let exec_options = CreateExecOptions {
        attach_stdout: Some(true),
        attach_stderr: Some(true),
        attach_stdin: Some(false),
        tty: Some(false),
        cmd: Some(vec!["sh", "-c", code]),
        working_dir: Some("/"),
        ..Default::default()
    };

    let exec_result = client
        .create_exec(container_id, exec_options)
        .await
        .map_err(|e| SandboxError::ExecutionFailed(format!("Failed to create exec: {}", e)))?;

    let output = client
        .start_exec(&exec_result.id, None)
        .await
        .map_err(|e| SandboxError::ExecutionFailed(format!("Failed to start exec: {}", e)))?;

    let mut stdout = String::new();
    let mut stderr = String::new();

    if let StartExecResults::Attached { mut output, .. } = output {
        while let Some(chunk) = output.next().await {
            match chunk {
                Ok(bollard::container::LogOutput::StdOut { message }) => {
                    stdout.push_str(&String::from_utf8_lossy(&message));
                }
                Ok(bollard::container::LogOutput::StdErr { message }) => {
                    stderr.push_str(&String::from_utf8_lossy(&message));
                }
                Ok(_) => {}
                Err(e) => {
                    tracing::warn!("Error reading exec output: {}", e);
                }
            }
        }
    }

    let exit_status = wait_for_exec_completion(client, &exec_result.id).await?;
    let duration_ms = start.elapsed().as_millis() as u64;
    let exit_code = exit_status as i32;

    tracing::debug!(
        "Executed in container {}: exit_code={}, duration_ms={}",
        container_id,
        exit_code,
        duration_ms
    );

    Ok(ExecResult {
        stdout,
        stderr,
        exit_code,
        execution_time_ms: duration_ms,
        success: exit_code == 0,
    })
}

/// Wait for exec to complete and get exit code
#[cfg(feature = "sandbox-docker")]
async fn wait_for_exec_completion(
    client: &bollard::Docker,
    exec_id: &str,
) -> Result<u32, SandboxError> {
    // Poll until exec completes
    for _ in 0..300 {
        // 30 second timeout
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        let info = client
            .inspect_exec(exec_id)
            .await
            .map_err(|e| SandboxError::ExecutionFailed(format!("Failed to inspect exec: {}", e)))?;

        // Check if running - it's Option<bool> in bollard 0.18
        if !info.running.unwrap_or(false) {
            return Ok(info.exit_code.unwrap_or(1) as u32);
        }
    }

    Err(SandboxError::ExecutionFailed("Exec timed out".to_string()))
}

/// Stop a Docker container
#[cfg(feature = "sandbox-docker")]
async fn stop_container(client: &bollard::Docker, container_id: &str) -> Result<(), SandboxError> {
    use bollard::container::StopContainerOptions;

    let options = StopContainerOptions { t: 10 };

    match client.stop_container(container_id, Some(options)).await {
        Ok(()) => {
            tracing::debug!("Stopped container: {}", container_id);
            Ok(())
        }
        Err(e) => {
            let err_str = e.to_string();
            if err_str.contains("not running") || err_str.contains("ContainerNotRunning") {
                tracing::debug!("Container already stopped: {}", container_id);
                return Ok(());
            }
            Err(SandboxError::ExecutionFailed(format!(
                "Failed to stop container: {}",
                e
            )))
        }
    }
}

/// Remove a Docker container
#[cfg(feature = "sandbox-docker")]
async fn remove_container(
    client: &bollard::Docker,
    container_id: &str,
) -> Result<(), SandboxError> {
    use bollard::container::RemoveContainerOptions;

    let options = RemoveContainerOptions {
        force: true,
        ..Default::default()
    };

    match client.remove_container(container_id, Some(options)).await {
        Ok(()) => {
            tracing::debug!("Removed container: {}", container_id);
            Ok(())
        }
        Err(e) => {
            let err_str = e.to_string();
            if err_str.contains("No such container") || err_str.contains("ContainerNotFound") {
                tracing::debug!("Container already removed: {}", container_id);
                return Ok(());
            }
            Err(SandboxError::ExecutionFailed(format!(
                "Failed to remove container: {}",
                e
            )))
        }
    }
}
