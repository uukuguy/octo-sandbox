use std::sync::Arc;

use axum::extract::State;
use axum::Json;
use serde::Serialize;

use octo_types::ToolSource;

use crate::state::AppState;

#[derive(Serialize)]
pub struct ToolInfo {
    pub name: String,
    pub description: String,
    pub source: ToolSource,
}

pub async fn list_tools(State(state): State<Arc<AppState>>) -> Json<Vec<ToolInfo>> {
    let specs = state.tools.specs();
    let tools: Vec<ToolInfo> = specs
        .into_iter()
        .map(|spec| {
            let source = state
                .tools
                .get(&spec.name)
                .map(|t| t.source())
                .unwrap_or(ToolSource::BuiltIn);
            ToolInfo {
                name: spec.name,
                description: spec.description,
                source,
            }
        })
        .collect();
    Json(tools)
}
