//! Docker container sandbox adapter using bollard
//!
//! This adapter provides Docker-based sandbox execution using the bollard Docker API.
//! It is feature-gated behind the `sandbox-docker` feature flag.

use super::{ExecResult, RuntimeAdapter, SandboxConfig, SandboxError, SandboxId, SandboxType};
use std::collections::HashMap;
#[cfg(not(feature = "sandbox-docker"))]
use std::marker::PhantomData;

#[cfg(feature = "sandbox-docker")]
use std::sync::Arc;
#[cfg(feature = "sandbox-docker")]
use tokio::sync::RwLock;

/// Default image used when no language-specific image is configured.
pub const DEFAULT_SANDBOX_IMAGE: &str = "octo-sandbox:base";

/// Preset image registry for language-based Docker image selection.
///
/// Supports custom overrides via `with_custom_images()`.
pub struct ImageRegistry {
    images: HashMap<String, String>,
    default_image: String,
}

impl ImageRegistry {
    /// Create a registry with built-in defaults pointing to `octo-sandbox:base`.
    pub fn default_registry() -> Self {
        let mut images = HashMap::new();
        // All languages default to the same base image.
        // Users can override per-language via with_custom_images().
        for lang in &[
            "python",
            "rust",
            "node",
            "javascript",
            "typescript",
            "bash",
            "sh",
            "general",
            "swebench",
        ] {
            images.insert((*lang).to_string(), DEFAULT_SANDBOX_IMAGE.to_string());
        }
        Self {
            images,
            default_image: DEFAULT_SANDBOX_IMAGE.to_string(),
        }
    }

    /// Apply custom image overrides (e.g., from config file).
    pub fn with_custom_images(mut self, overrides: HashMap<String, String>) -> Self {
        for (lang, image) in overrides {
            self.images.insert(lang, image);
        }
        self
    }

    /// Override the default fallback image.
    pub fn with_default_image(mut self, image: impl Into<String>) -> Self {
        self.default_image = image.into();
        self
    }

    /// Resolve a language to a Docker image name.
    pub fn resolve(&self, language: &str) -> &str {
        self.images
            .get(language)
            .map(|s| s.as_str())
            .unwrap_or(&self.default_image)
    }

    /// Get the default image name.
    pub fn default_image(&self) -> &str {
        &self.default_image
    }
}

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

    /// Preset image registry for language-based image selection
    image_registry: ImageRegistry,

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
            image_registry: ImageRegistry::default_registry(),
            #[cfg(feature = "sandbox-docker")]
            client,
        }
    }

    /// Create a new DockerAdapter with default image `octo-sandbox:base`.
    pub fn with_default_image() -> Self {
        Self::new(DEFAULT_SANDBOX_IMAGE)
    }

    /// Create a new DockerAdapter with a custom image registry.
    pub fn with_registry(registry: ImageRegistry) -> Self {
        let default_image = registry.default_image().to_string();
        let mut adapter = Self::new(default_image);
        adapter.image_registry = registry;
        adapter
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

    /// Get the image registry
    pub fn image_registry(&self) -> &ImageRegistry {
        &self.image_registry
    }

    /// Check if a Docker image is available locally.
    #[cfg(feature = "sandbox-docker")]
    pub async fn is_image_available(&self, image: &str) -> bool {
        if let Some(client) = &self.client {
            client.inspect_image(image).await.is_ok()
        } else {
            false
        }
    }

    /// Check if a Docker image is available locally (stub without feature).
    #[cfg(not(feature = "sandbox-docker"))]
    pub async fn is_image_available(&self, _image: &str) -> bool {
        false
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
        language: &str,
    ) -> Result<ExecResult, SandboxError> {
        #[cfg(not(feature = "sandbox-docker"))]
        {
            let _ = (id, code, language);
            return Err(SandboxError::UnsupportedType(
                "Docker support not enabled. Enable sandbox-docker feature".to_string(),
            ));
        }

        #[cfg(feature = "sandbox-docker")]
        {
            let resolved_image = self.image_registry.resolve(language);
            tracing::debug!(
                "Resolved language '{}' to Docker image '{}'",
                language,
                resolved_image
            );

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

    // Build bind mounts from config
    let binds: Vec<String> = config.bind_mounts.iter()
        .map(|(host, container)| format!("{}:{}", host, container))
        .collect();

    // Resource limits from config env (set by session sandbox manager via SandboxProfile)
    let memory_limit = config
        .env
        .get("OCTO_MEMORY_LIMIT")
        .and_then(|v| v.parse::<i64>().ok());
    let cpu_quota = config
        .env
        .get("OCTO_CPU_QUOTA")
        .and_then(|v| v.parse::<i64>().ok());
    let network_mode = config.env.get("OCTO_NETWORK_MODE").cloned();

    // Container config - keep the container alive with a sleeping shell
    // Alpine exits immediately without a blocking process; `sh` + tty keeps it running
    let container_config = Config {
        image: Some(image.to_string()),
        env: Some(env_vars),
        labels: Some(labels),
        cmd: Some(vec!["sh".to_string()]),
        tty: Some(true),
        open_stdin: Some(true),
        host_config: Some(bollard::models::HostConfig {
            binds: if binds.is_empty() { None } else { Some(binds) },
            memory: memory_limit,
            cpu_quota,
            cpu_period: if cpu_quota.is_some() {
                Some(100_000)
            } else {
                None
            },
            network_mode,
            ..Default::default()
        }),
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
