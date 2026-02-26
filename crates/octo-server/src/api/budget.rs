use std::sync::Arc;

use axum::extract::State;
use axum::Json;

use octo_types::TokenBudgetSnapshot;

use crate::state::AppState;

pub async fn get_budget(State(_state): State<Arc<AppState>>) -> Json<TokenBudgetSnapshot> {
    Json(TokenBudgetSnapshot {
        total: 200_000,
        system_prompt: 0,
        dynamic_context: 0,
        history: 0,
        free: 200_000,
        usage_percent: 0.0,
        degradation_level: 0,
    })
}
