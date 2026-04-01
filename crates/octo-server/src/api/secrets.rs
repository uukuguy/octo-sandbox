//! Secret Vault API — encrypted credential management (AO-T6)
//!
//! SECURITY: This module NEVER returns secret values. Only names are exposed.

use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{delete, get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};

use crate::state::AppState;

// ── Response / Request types ────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct SecretListResponse {
    pub secrets: Vec<String>,
    pub count: usize,
}

#[derive(Debug, Deserialize)]
pub struct StoreSecretRequest {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Serialize)]
pub struct StoreSecretResponse {
    pub name: String,
    pub stored: bool,
}

#[derive(Debug, Serialize)]
pub struct VaultStatusResponse {
    pub unlocked: bool,
    pub secret_count: usize,
}

#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub error: String,
}

// ── Router ──────────────────────────────────────────────────────────

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        // Put /secrets/verify BEFORE /secrets/{name} to avoid path param capture
        .route("/secrets/verify", post(verify_vault))
        .route("/secrets", get(list_secrets).post(store_secret))
        .route("/secrets/{name}", delete(delete_secret))
}

// ── Handlers ────────────────────────────────────────────────────────

async fn list_secrets(
    State(state): State<Arc<AppState>>,
) -> Json<SecretListResponse> {
    let resolver = state.agent_supervisor.credential_resolver();
    match resolver.vault() {
        Some(vault) => {
            let names = vault.list();
            let count = names.len();
            Json(SecretListResponse {
                secrets: names,
                count,
            })
        }
        None => Json(SecretListResponse {
            secrets: vec![],
            count: 0,
        }),
    }
}

async fn store_secret(
    State(state): State<Arc<AppState>>,
    Json(req): Json<StoreSecretRequest>,
) -> Result<Json<StoreSecretResponse>, (StatusCode, Json<ErrorResponse>)> {
    let resolver = state.agent_supervisor.credential_resolver();
    match resolver.vault() {
        Some(vault) => {
            vault
                .set(&req.name, &req.value)
                .map_err(|e| {
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(ErrorResponse { error: e }),
                    )
                })?;
            Ok(Json(StoreSecretResponse {
                name: req.name,
                stored: true,
            }))
        }
        None => Err((
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ErrorResponse {
                error: "Vault not initialized".into(),
            }),
        )),
    }
}

async fn delete_secret(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    let resolver = state.agent_supervisor.credential_resolver();
    match resolver.vault() {
        Some(vault) => {
            vault.delete(&name).map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse { error: e }),
                )
            })?;
            Ok(StatusCode::NO_CONTENT)
        }
        None => Err((
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ErrorResponse {
                error: "Vault not initialized".into(),
            }),
        )),
    }
}

async fn verify_vault(
    State(state): State<Arc<AppState>>,
) -> Json<VaultStatusResponse> {
    let resolver = state.agent_supervisor.credential_resolver();
    match resolver.vault() {
        Some(vault) => Json(VaultStatusResponse {
            unlocked: true,
            secret_count: vault.list().len(),
        }),
        None => Json(VaultStatusResponse {
            unlocked: false,
            secret_count: 0,
        }),
    }
}
