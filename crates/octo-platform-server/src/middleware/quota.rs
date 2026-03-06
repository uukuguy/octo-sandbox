use axum::{
    body::Body,
    extract::Request,
    http::{Response, StatusCode},
    middleware::Next,
};
use std::sync::Arc;

use crate::tenant::TenantRuntime;

pub async fn quota_middleware(
    tenant: Option<Arc<TenantRuntime>>,
    request: Request<Body>,
    next: Next,
) -> Response<Body> {
    if let Some(tenant) = tenant {
        // Check API call quota
        if let Err(e) = tenant.quota_manager.check_api_call() {
            let retry_after = 60; // seconds
            return Response::builder()
                .status(StatusCode::TOO_MANY_REQUESTS)
                .header("Retry-After", retry_after.to_string())
                .body(Body::from(e.to_string()))
                .unwrap();
        }
    }

    next.run(request).await
}
