//! User API handlers
//!
//! Provides CRUD operations for user management with role-based authorization.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use serde::Deserialize;

use crate::db::{PaginatedUsersResponse, UpdateUserRequest, UserResponse, UserRole};
use crate::{ArcAppState, AuthExtractor, ErrorResponse};

/// Custom error type that can return different status codes
type ApiError = (StatusCode, Json<ErrorResponse>);

/// Query parameters for listing users
#[derive(Debug, Deserialize)]
pub struct ListUsersQuery {
    #[serde(default = "default_page")]
    page: i64,
    #[serde(default = "default_per_page")]
    per_page: i64,
}

fn default_page() -> i64 {
    1
}

fn default_per_page() -> i64 {
    20
}

/// Request to update user role
#[derive(Debug, Deserialize)]
pub struct UpdateRoleRequest {
    pub role: String,
}

/// Check if current user is admin
fn is_admin(auth: &AuthExtractor) -> bool {
    auth.role.to_lowercase() == UserRole::Admin.to_string()
}

/// List all users (admin only)
pub async fn list_users(
    State(state): State<ArcAppState>,
    auth: AuthExtractor,
    Query(query): Query<ListUsersQuery>,
) -> Result<Json<PaginatedUsersResponse>, ApiError> {
    // Admin-only endpoint
    if !is_admin(&auth) {
        return Err((
            StatusCode::FORBIDDEN,
            Json(ErrorResponse {
                error: "Admin access required".to_string(),
            }),
        ));
    }

    let page = query.page.max(1);
    let per_page = query.per_page.clamp(1, 100);

    state
        .db
        .list_users(&auth.tenant_id, page, per_page)
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("Failed to list users: {}", e),
                }),
            )
        })
        .map(Json)
}

/// Get a specific user by ID
pub async fn get_user(
    State(state): State<ArcAppState>,
    auth: AuthExtractor,
    Path(user_id): Path<String>,
) -> Result<Json<UserResponse>, ApiError> {
    // Admin can view any user, regular users can only view themselves
    if !is_admin(&auth) && auth.user_id != user_id {
        return Err((
            StatusCode::FORBIDDEN,
            Json(ErrorResponse {
                error: "Cannot view other users".to_string(),
            }),
        ));
    }

    let user = state.db.get_user(&auth.tenant_id, &user_id).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: format!("Failed to get user: {}", e),
            }),
        )
    })?;

    match user {
        Some(u) => Ok(Json(u)),
        None => Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: "User not found".to_string(),
            }),
        )),
    }
}

/// Update a user
pub async fn update_user(
    State(state): State<ArcAppState>,
    auth: AuthExtractor,
    Path(user_id): Path<String>,
    Json(req): Json<UpdateUserRequest>,
) -> Result<Json<UserResponse>, ApiError> {
    // Validate email if provided
    if let Some(ref email) = req.email {
        let email = email.trim();
        if email.is_empty() {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: "Email cannot be empty".to_string(),
                }),
            ));
        }
        if !email.contains('@') {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: "Invalid email format".to_string(),
                }),
            ));
        }
        if email.len() > 255 {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: "Email cannot exceed 255 characters".to_string(),
                }),
            ));
        }
    }

    // Validate display_name if provided
    if let Some(ref display_name) = req.display_name {
        let display_name = display_name.trim();
        if display_name.is_empty() {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: "Display name cannot be empty".to_string(),
                }),
            ));
        }
        if display_name.len() > 100 {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: "Display name cannot exceed 100 characters".to_string(),
                }),
            ));
        }
    }

    // Admin can update any user, regular users can only update their own profile
    // Non-admins can only update display_name
    if !is_admin(&auth) {
        if auth.user_id != user_id {
            return Err((
                StatusCode::FORBIDDEN,
                Json(ErrorResponse {
                    error: "Cannot update other users".to_string(),
                }),
            ));
        }

        // Non-admins can only update display_name
        if req.email.is_some() || req.role.is_some() {
            return Err((
                StatusCode::FORBIDDEN,
                Json(ErrorResponse {
                    error: "Cannot update email or role".to_string(),
                }),
            ));
        }
    }

    // Validate role if provided
    if let Some(ref role) = req.role {
        let valid_role = matches!(role.to_lowercase().as_str(), "admin" | "member" | "viewer");
        if !valid_role {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: "Invalid role. Must be admin, member, or viewer".to_string(),
                }),
            ));
        }
    }

    let user = state
        .db
        .update_user(&auth.tenant_id, &user_id, &req)
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("Failed to update user: {}", e),
                }),
            )
        })?;

    match user {
        Some(u) => {
            tracing::info!("User updated: {}", user_id);
            Ok(Json(u))
        }
        None => Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: "User not found".to_string(),
            }),
        )),
    }
}

/// Delete a user (admin only)
pub async fn delete_user(
    State(state): State<ArcAppState>,
    auth: AuthExtractor,
    Path(user_id): Path<String>,
) -> Result<StatusCode, ApiError> {
    // Admin-only endpoint
    if !is_admin(&auth) {
        return Err((
            StatusCode::FORBIDDEN,
            Json(ErrorResponse {
                error: "Admin access required".to_string(),
            }),
        ));
    }

    // Prevent admin from deleting themselves
    if auth.user_id == user_id {
        return Err((
            StatusCode::FORBIDDEN,
            Json(ErrorResponse {
                error: "Cannot delete yourself".to_string(),
            }),
        ));
    }

    let deleted = state
        .db
        .delete_user(&auth.tenant_id, &user_id)
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("Failed to delete user: {}", e),
                }),
            )
        })?;

    if deleted {
        tracing::info!("User deleted: {}", user_id);
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: "User not found".to_string(),
            }),
        ))
    }
}

/// Update user role (admin only)
pub async fn update_user_role(
    State(state): State<ArcAppState>,
    auth: AuthExtractor,
    Path(user_id): Path<String>,
    Json(req): Json<UpdateRoleRequest>,
) -> Result<Json<UserResponse>, ApiError> {
    // Admin-only endpoint
    if !is_admin(&auth) {
        return Err((
            StatusCode::FORBIDDEN,
            Json(ErrorResponse {
                error: "Admin access required".to_string(),
            }),
        ));
    }

    // Validate role
    let valid_role = matches!(
        req.role.to_lowercase().as_str(),
        "admin" | "member" | "viewer"
    );

    if !valid_role {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "Invalid role. Must be admin, member, or viewer".to_string(),
            }),
        ));
    }

    let update_req = UpdateUserRequest {
        email: None,
        display_name: None,
        role: Some(req.role),
    };

    let user = state
        .db
        .update_user(&auth.tenant_id, &user_id, &update_req)
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("Failed to update user role: {}", e),
                }),
            )
        })?;

    match user {
        Some(u) => {
            tracing::info!("User role updated: {} -> {}", user_id, u.role);
            Ok(Json(u))
        }
        None => Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: "User not found".to_string(),
            }),
        )),
    }
}
