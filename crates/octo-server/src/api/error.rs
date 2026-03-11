//! Unified API error response type.
//!
//! All API handlers can return [`ApiError`] which is automatically serialized
//! into a consistent JSON error body with the structure:
//!
//! ```json
//! { "error": { "code": "NOT_FOUND", "message": "..." } }
//! ```

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Serialize;

/// Envelope for a JSON error response.
#[allow(dead_code)]
#[derive(Debug, Serialize)]
pub struct ApiErrorBody {
    pub error: ApiErrorDetail,
}

/// Inner detail of the error response.
#[allow(dead_code)]
#[derive(Debug, Serialize)]
pub struct ApiErrorDetail {
    pub code: String,
    pub message: String,
}

/// Unified error type that implements [`IntoResponse`].
#[allow(dead_code)]
#[derive(Debug)]
pub enum ApiError {
    NotFound(String),
    BadRequest(String),
    Internal(String),
    Unauthorized(String),
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ApiError::NotFound(msg) => write!(f, "Not found: {msg}"),
            ApiError::BadRequest(msg) => write!(f, "Bad request: {msg}"),
            ApiError::Internal(msg) => write!(f, "Internal error: {msg}"),
            ApiError::Unauthorized(msg) => write!(f, "Unauthorized: {msg}"),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, code, message) = match self {
            ApiError::NotFound(msg) => (StatusCode::NOT_FOUND, "NOT_FOUND", msg),
            ApiError::BadRequest(msg) => (StatusCode::BAD_REQUEST, "BAD_REQUEST", msg),
            ApiError::Internal(msg) => {
                (StatusCode::INTERNAL_SERVER_ERROR, "INTERNAL_ERROR", msg)
            }
            ApiError::Unauthorized(msg) => (StatusCode::UNAUTHORIZED, "UNAUTHORIZED", msg),
        };
        let body = ApiErrorBody {
            error: ApiErrorDetail {
                code: code.to_string(),
                message,
            },
        };
        (status, axum::Json(body)).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_not_found_response_status() {
        let err = ApiError::NotFound("agent xyz not found".to_string());
        let response = err.into_response();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[test]
    fn test_bad_request_response_status() {
        let err = ApiError::BadRequest("invalid input".to_string());
        let response = err.into_response();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[test]
    fn test_internal_response_status() {
        let err = ApiError::Internal("something broke".to_string());
        let response = err.into_response();
        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[test]
    fn test_unauthorized_response_status() {
        let err = ApiError::Unauthorized("no token".to_string());
        let response = err.into_response();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[test]
    fn test_display() {
        let err = ApiError::NotFound("test".to_string());
        assert_eq!(format!("{err}"), "Not found: test");
    }

    #[test]
    fn test_error_body_serialization() {
        let body = ApiErrorBody {
            error: ApiErrorDetail {
                code: "NOT_FOUND".to_string(),
                message: "agent xyz not found".to_string(),
            },
        };
        let json = serde_json::to_value(&body).unwrap();
        assert_eq!(json["error"]["code"], "NOT_FOUND");
        assert_eq!(json["error"]["message"], "agent xyz not found");
    }
}
