/**
 * Frontend Configuration Module
 *
 * Fetches runtime configuration from the backend API.
 * This enables unified configuration management where
 * config.yaml is the single source of truth.
 */

export interface BackendConfig {
  /** Server host */
  host: string;
  /** Server port */
  port: number;
  /** Backend API base URL */
  api_url: string;
  /** WebSocket URL for real-time communication */
  ws_url: string;
  /** MCP servers directory (if configured) */
  mcp_servers_dir: string | null;
  /** Provider name (e.g., "anthropic", "openai") */
  provider: string;
  /** Model being used (if set) */
  model: string | null;
}

let backendConfig: BackendConfig | null = null;
let configPromise: Promise<BackendConfig> | null = null;

/**
 * Initialize configuration by fetching from backend
 * Should be called before rendering the app
 */
export async function initConfig(): Promise<BackendConfig> {
  if (backendConfig) {
    return backendConfig;
  }

  if (configPromise) {
    return configPromise;
  }

  configPromise = fetch('/api/v1/config')
    .then((res) => {
      if (!res.ok) {
        throw new Error(`Failed to fetch config: ${res.status}`);
      }
      return res.json();
    })
    .then((config) => {
      backendConfig = config as BackendConfig;
      console.log('[Config] Initialized:', backendConfig);
      return backendConfig;
    })
    .catch((err) => {
      console.error('[Config] Failed to initialize:', err);
      // Return fallback config
      const fallback: BackendConfig = {
        host: '127.0.0.1',
        port: 3001,
        api_url: 'http://127.0.0.1:3001',
        ws_url: 'ws://127.0.0.1:3001',
        mcp_servers_dir: null,
        provider: 'anthropic',
        model: null,
      };
      backendConfig = fallback;
      return fallback;
    });

  return configPromise;
}

/**
 * Get the current configuration
 * Throws if initConfig() hasn't been called yet
 */
export function getConfig(): BackendConfig {
  if (!backendConfig) {
    throw new Error('Config not initialized. Call initConfig() first.');
  }
  return backendConfig;
}

/**
 * Check if config has been initialized
 */
export function isConfigReady(): boolean {
  return backendConfig !== null;
}

/**
 * Get WebSocket URL for real-time communication
 */
export function getWsUrl(): string {
  return getConfig().ws_url;
}

/**
 * Get API base URL
 */
export function getApiUrl(): string {
  return getConfig().api_url;
}
