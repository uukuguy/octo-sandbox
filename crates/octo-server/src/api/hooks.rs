//! Hooks Management API — hook registry and WASM plugin management (AO-T3)

use std::sync::Arc;

use axum::{
    extract::State,
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use serde::Serialize;

use crate::state::AppState;

// ── Response types ───────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct HookPointInfo {
    pub point: String,
    pub handler_count: usize,
}

#[derive(Debug, Serialize)]
pub struct HookPointsResponse {
    pub points: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct ReloadResponse {
    pub reloaded: bool,
    pub message: String,
}

#[derive(Debug, Serialize)]
pub struct WasmPluginInfo {
    pub name: String,
    pub status: String,
}

// ── Router ───────────────────────────────────────────────────────────

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/hooks", get(list_hooks))
        .route("/hooks/points", get(list_points))
        .route("/hooks/reload", post(reload_declarative))
        .route("/hooks/wasm", get(list_wasm_plugins))
        .route("/hooks/wasm/{name}/reload", post(reload_wasm_plugin))
}

// ── Handlers ─────────────────────────────────────────────────────────

/// GET /api/v1/hooks — registered hook list grouped by HookPoint
async fn list_hooks(State(state): State<Arc<AppState>>) -> Json<Vec<HookPointInfo>> {
    let registry = state.agent_supervisor.hook_registry();
    let entries = registry.list_all().await;
    let result = entries
        .into_iter()
        .map(|(point, count)| HookPointInfo {
            point: format!("{:?}", point),
            handler_count: count,
        })
        .collect();
    Json(result)
}

/// GET /api/v1/hooks/points — all HookPoint enum values
async fn list_points() -> Json<HookPointsResponse> {
    // All HookPoint variants — kept in sync with octo_engine::hooks::HookPoint
    let points = vec![
        "PreToolUse",
        "PostToolUse",
        "PreTask",
        "PostTask",
        "SessionStart",
        "SessionEnd",
        "ContextDegraded",
        "LoopTurnStart",
        "LoopTurnEnd",
        "AgentRoute",
        "SkillsActivated",
        "SkillDeactivated",
        "SkillScriptStarted",
        "ToolConstraintViolated",
        "Stop",
        "SubagentStop",
    ];
    Json(HookPointsResponse {
        points: points.into_iter().map(String::from).collect(),
    })
}

/// POST /api/v1/hooks/reload — reload declarative hooks.yaml (hot-reload parse check)
async fn reload_declarative() -> Json<ReloadResponse> {
    let config = octo_engine::hooks::declarative::loader::load_hooks_config_auto(None);
    match config {
        Some(cfg) => Json(ReloadResponse {
            reloaded: true,
            message: format!(
                "Parsed hooks.yaml successfully ({} hook points configured)",
                cfg.hooks.len()
            ),
        }),
        None => Json(ReloadResponse {
            reloaded: false,
            message: "No hooks.yaml found or failed to parse".to_string(),
        }),
    }
}

/// GET /api/v1/hooks/wasm — WASM plugin list
async fn list_wasm_plugins() -> Json<Vec<WasmPluginInfo>> {
    #[cfg(feature = "sandbox-wasm")]
    {
        // Discover plugins from default paths
        let plugins = octo_engine::hooks::wasm::loader::discover_plugins(&[]);
        let result = plugins
            .into_iter()
            .map(|p| WasmPluginInfo {
                name: p.name.clone(),
                status: "discovered".to_string(),
            })
            .collect();
        return Json(result);
    }

    #[cfg(not(feature = "sandbox-wasm"))]
    {
        Json(vec![])
    }
}

/// POST /api/v1/hooks/wasm/:name/reload — reload a single WASM plugin
async fn reload_wasm_plugin(
    axum::extract::Path(_name): axum::extract::Path<String>,
) -> StatusCode {
    #[cfg(feature = "sandbox-wasm")]
    {
        // Individual WASM plugin reload requires engine-level support not yet wired
        return StatusCode::NOT_IMPLEMENTED;
    }

    #[cfg(not(feature = "sandbox-wasm"))]
    {
        StatusCode::NOT_FOUND
    }
}
