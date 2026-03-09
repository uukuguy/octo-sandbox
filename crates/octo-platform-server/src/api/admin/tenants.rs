use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::get,
    Json, Router,
};
use std::sync::Arc;

use crate::tenant::ResourceQuota;
use crate::{AppState, AuthExtractor, ErrorResponse};

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/admin/tenants", get(list_tenants).post(create_tenant))
        .route(
            "/api/admin/tenants/:id",
            get(get_tenant).patch(update_tenant).delete(delete_tenant),
        )
        .route(
            "/api/admin/tenants/:id/quotas",
            get(get_quotas).patch(update_quotas),
        )
}

// Admin authorization check - only admins can access these endpoints
pub(crate) fn require_admin(auth: &AuthExtractor) -> Result<(), ErrorResponse> {
    if auth.role != "admin" {
        return Err(ErrorResponse {
            error: "Admin access required".to_string(),
        });
    }
    Ok(())
}

pub async fn list_tenants(
    State(_state): State<Arc<AppState>>,
    auth: AuthExtractor,
) -> Result<Json<Vec<TenantResponse>>, ErrorResponse> {
    require_admin(&auth)?;

    // For now, just return the default tenant - full list requires DB query
    // This is a simplified implementation
    let tenants = vec![TenantResponse {
        id: "default".to_string(),
        name: "Default Tenant".to_string(),
        slug: "default".to_string(),
        plan: "free".to_string(),
        created_at: chrono::Utc::now().timestamp(),
        updated_at: chrono::Utc::now().timestamp(),
    }];

    Ok(Json(tenants))
}

pub async fn get_tenant(
    State(_state): State<Arc<AppState>>,
    auth: AuthExtractor,
    Path(id): Path<String>,
) -> Result<Json<TenantResponse>, ErrorResponse> {
    require_admin(&auth)?;

    // Simplified - return mock data
    Ok(Json(TenantResponse {
        id,
        name: "Tenant".to_string(),
        slug: "tenant".to_string(),
        plan: "free".to_string(),
        created_at: chrono::Utc::now().timestamp(),
        updated_at: chrono::Utc::now().timestamp(),
    }))
}

pub async fn create_tenant(
    State(_state): State<Arc<AppState>>,
    auth: AuthExtractor,
    Json(req): Json<CreateTenantRequest>,
) -> Result<Json<TenantResponse>, ErrorResponse> {
    require_admin(&auth)?;

    // In a full implementation, this would create the tenant in the database
    Ok(Json(TenantResponse {
        id: req.id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
        name: req.name,
        slug: req.slug,
        plan: req.plan.unwrap_or_else(|| "free".to_string()),
        created_at: chrono::Utc::now().timestamp(),
        updated_at: chrono::Utc::now().timestamp(),
    }))
}

pub async fn update_tenant(
    State(_state): State<Arc<AppState>>,
    auth: AuthExtractor,
    Path(id): Path<String>,
    Json(req): Json<UpdateTenantRequest>,
) -> Result<Json<TenantResponse>, ErrorResponse> {
    require_admin(&auth)?;

    Ok(Json(TenantResponse {
        id,
        name: req.name.unwrap_or_default(),
        slug: req.slug.unwrap_or_default(),
        plan: req.plan.unwrap_or_default(),
        created_at: chrono::Utc::now().timestamp(),
        updated_at: chrono::Utc::now().timestamp(),
    }))
}

pub async fn delete_tenant(
    State(_state): State<Arc<AppState>>,
    auth: AuthExtractor,
    Path(id): Path<String>,
) -> Result<StatusCode, ErrorResponse> {
    require_admin(&auth)?;

    // Don't allow deleting the default tenant
    if id == "default" {
        return Err(ErrorResponse {
            error: "Cannot delete default tenant".to_string(),
        });
    }

    Ok(StatusCode::NO_CONTENT)
}

pub async fn get_quotas(
    State(state): State<Arc<AppState>>,
    auth: AuthExtractor,
    Path(tenant_id): Path<String>,
) -> Result<Json<ResourceQuota>, ErrorResponse> {
    require_admin(&auth)?;

    let quota = state
        .tenant_manager
        .get_quota(&tenant_id)
        .map_err(|e| ErrorResponse {
            error: e.to_string(),
        })?;

    Ok(Json(quota))
}

pub async fn update_quotas(
    State(_state): State<Arc<AppState>>,
    auth: AuthExtractor,
    Path(_tenant_id): Path<String>,
    Json(quota): Json<ResourceQuota>,
) -> Result<Json<ResourceQuota>, ErrorResponse> {
    require_admin(&auth)?;

    // In a full implementation, this would update the quota in the database
    Ok(Json(quota))
}

#[derive(Debug, serde::Serialize)]
pub struct TenantResponse {
    pub id: String,
    pub name: String,
    pub slug: String,
    pub plan: String,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, serde::Deserialize)]
pub struct CreateTenantRequest {
    pub name: String,
    pub slug: String,
    pub plan: Option<String>,
    pub id: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
pub struct UpdateTenantRequest {
    pub name: Option<String>,
    pub slug: Option<String>,
    pub plan: Option<String>,
}
