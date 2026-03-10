//! Dashboard command — launches an embedded web dashboard
//!
//! Serves a lightweight single-page app directly from the CLI binary.
//! All assets are compiled in via `include_str!()`.

use anyhow::Result;
use axum::{extract::Path, extract::Query, routing::get, Json, Router};
use std::collections::HashMap;
use std::net::SocketAddr;

// ── Embedded Assets ──────────────────────────────────────────────

const INDEX_HTML: &str = include_str!("../dashboard/assets/index.html");
const APP_JS: &str = include_str!("../dashboard/assets/app.js");
const STYLE_CSS: &str = include_str!("../dashboard/assets/style.css");

// ── Dashboard Options ────────────────────────────────────────────

/// Configuration for the dashboard server.
pub struct DashboardOptions {
    /// Port to listen on (default: 8080)
    pub port: u16,
    /// Host to bind to (default: 127.0.0.1)
    pub host: String,
    /// Open browser on start
    pub open: bool,
}

impl Default for DashboardOptions {
    fn default() -> Self {
        Self {
            port: 8080,
            host: "127.0.0.1".to_string(),
            open: false,
        }
    }
}

// ── Route Handlers ──────────────────────────────────────────────

async fn index_handler() -> axum::response::Html<&'static str> {
    axum::response::Html(INDEX_HTML)
}

async fn app_js_handler() -> (
    [(axum::http::header::HeaderName, &'static str); 1],
    &'static str,
) {
    (
        [(
            axum::http::header::CONTENT_TYPE,
            "application/javascript",
        )],
        APP_JS,
    )
}

async fn style_css_handler() -> (
    [(axum::http::header::HeaderName, &'static str); 1],
    &'static str,
) {
    (
        [(axum::http::header::CONTENT_TYPE, "text/css")],
        STYLE_CSS,
    )
}

/// API: health check
async fn api_health() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "status": "ok",
        "version": env!("CARGO_PKG_VERSION"),
    }))
}

/// D2-3: Chat message endpoint (stub — echoes back in preview mode)
async fn api_chat_send(Json(body): Json<serde_json::Value>) -> Json<serde_json::Value> {
    let user_msg = body["message"].as_str().unwrap_or("");
    Json(serde_json::json!({
        "response": format!(
            "Dashboard preview: received '{}'. Use CLI for full agent interaction.",
            user_msg
        ),
        "model": "preview",
    }))
}

/// D2-4: List sessions (stub)
async fn api_sessions_list() -> Json<serde_json::Value> {
    Json(serde_json::json!([
        {"id": "session-001", "created_at": "2026-03-10T10:00:00Z", "messages": 12, "status": "active"},
        {"id": "session-002", "created_at": "2026-03-09T15:30:00Z", "messages": 45, "status": "closed"},
    ]))
}

/// D2-4: Session detail (stub)
async fn api_session_detail(Path(id): Path<String>) -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "id": id,
        "created_at": "2026-03-10T10:00:00Z",
        "messages": 12,
        "status": "active",
        "model": "claude-sonnet-4-6",
    }))
}

/// D2-5: List memories (stub)
async fn api_memories_list() -> Json<serde_json::Value> {
    Json(serde_json::json!([
        {"id": "mem-001", "category": "project_structure", "content": "Main entry: src/main.rs", "score": 0.95},
        {"id": "mem-002", "category": "user_preference", "content": "Always use cargo test --test-threads=1", "score": 0.88},
        {"id": "mem-003", "category": "technical_decision", "content": "Use Axum for HTTP server", "score": 0.82},
    ]))
}

/// D2-5: Search memories (stub)
async fn api_memories_search(
    Query(params): Query<HashMap<String, String>>,
) -> Json<serde_json::Value> {
    let query = params.get("q").cloned().unwrap_or_default();
    Json(serde_json::json!({
        "query": query,
        "results": [
            {"id": "mem-001", "category": "project_structure", "content": "Main entry: src/main.rs", "score": 0.95},
        ],
    }))
}

/// D2-6: MCP servers (stub)
async fn api_mcp_servers() -> Json<serde_json::Value> {
    Json(serde_json::json!([
        {"name": "filesystem", "running": true, "tools": 5, "transport": "stdio"},
        {"name": "github", "running": false, "tools": 12, "transport": "sse"},
    ]))
}

/// D2-7: Available themes list
async fn api_themes_list() -> Json<serde_json::Value> {
    Json(serde_json::json!([
        "cyan", "sgcc", "blue", "indigo", "violet", "emerald",
        "amber", "coral", "rose", "teal", "sunset", "slate"
    ]))
}

// ── Router ──────────────────────────────────────────────────────

fn build_router() -> Router {
    Router::new()
        // Static assets
        .route("/", get(index_handler))
        .route("/app.js", get(app_js_handler))
        .route("/style.css", get(style_css_handler))
        // API endpoints
        .route("/api/health", get(api_health))
        // D2-3: Chat
        .route("/api/chat", axum::routing::post(api_chat_send))
        // D2-4: Sessions
        .route("/api/sessions", get(api_sessions_list))
        .route("/api/sessions/{id}", get(api_session_detail))
        // D2-5: Memory
        .route("/api/memories", get(api_memories_list))
        .route("/api/memories/search", get(api_memories_search))
        // D2-6: MCP
        .route("/api/mcp/servers", get(api_mcp_servers))
        // D2-7: Themes
        .route("/api/themes", get(api_themes_list))
}

/// Run the dashboard server.
pub async fn run_dashboard(opts: &DashboardOptions) -> Result<()> {
    let addr: SocketAddr = format!("{}:{}", opts.host, opts.port).parse()?;
    let router = build_router();

    eprintln!("Dashboard running at http://{}", addr);
    eprintln!("Press Ctrl+C to stop.\n");

    if opts.open {
        // Best-effort: try to open the browser
        let url = format!("http://{}", addr);
        let _ = open_browser(&url);
    }

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, router).await?;

    Ok(())
}

/// Attempt to open a URL in the default browser.
fn open_browser(url: &str) -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open").arg(url).spawn()?;
    }
    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open").arg(url).spawn()?;
    }
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("cmd")
            .args(["/c", "start", url])
            .spawn()?;
    }
    Ok(())
}

// ── Tests ────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request;
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    /// Helper: GET a URI, assert 200, parse JSON body.
    async fn get_json(uri: &str) -> serde_json::Value {
        let app = build_router();
        let req = Request::builder().uri(uri).body(Body::empty()).unwrap();
        let res = app.oneshot(req).await.unwrap();
        assert_eq!(res.status(), 200);
        let body = res.into_body().collect().await.unwrap().to_bytes();
        serde_json::from_slice(&body).unwrap()
    }

    /// Helper: GET a URI, return status code.
    async fn get_status(uri: &str) -> u16 {
        let app = build_router();
        let req = Request::builder().uri(uri).body(Body::empty()).unwrap();
        let res = app.oneshot(req).await.unwrap();
        res.status().as_u16()
    }

    #[test]
    fn test_dashboard_options_default() {
        let opts = DashboardOptions::default();
        assert_eq!(opts.port, 8080);
        assert_eq!(opts.host, "127.0.0.1");
        assert!(!opts.open);
    }

    #[test]
    fn test_embedded_assets_not_empty() {
        assert!(!INDEX_HTML.is_empty());
        assert!(!APP_JS.is_empty());
        assert!(!STYLE_CSS.is_empty());
    }

    #[test]
    fn test_index_html_contains_alpine_directives() {
        assert!(INDEX_HTML.contains("x-data"));
        assert!(INDEX_HTML.contains("x-init"));
        assert!(INDEX_HTML.contains("x-show"));
    }

    #[test]
    fn test_index_html_contains_all_tabs() {
        for tab in ["Chat", "Sessions", "Memory", "MCP"] {
            assert!(INDEX_HTML.contains(tab), "Missing tab: {tab}");
        }
    }

    #[test]
    fn test_style_css_has_root_variables() {
        for var in [":root", "--accent", "--bg"] {
            assert!(STYLE_CSS.contains(var), "Missing CSS var: {var}");
        }
    }

    #[test]
    fn test_app_js_has_app_function() {
        for token in ["function app()", "checkHealth", "sendMessage"] {
            assert!(APP_JS.contains(token), "Missing JS token: {token}");
        }
    }

    #[test]
    fn test_router_builds() {
        let _ = build_router();
    }

    #[tokio::test]
    async fn test_health_endpoint() {
        assert_eq!(get_status("/api/health").await, 200);
    }

    #[tokio::test]
    async fn test_index_endpoint() {
        assert_eq!(get_status("/").await, 200);
    }

    #[tokio::test]
    async fn test_js_endpoint() {
        let app = build_router();
        let req = Request::builder().uri("/app.js").body(Body::empty()).unwrap();
        let res = app.oneshot(req).await.unwrap();
        assert_eq!(res.status(), 200);
        let ct = res.headers().get("content-type").unwrap().to_str().unwrap();
        assert_eq!(ct, "application/javascript");
    }

    #[tokio::test]
    async fn test_css_endpoint() {
        let app = build_router();
        let req = Request::builder().uri("/style.css").body(Body::empty()).unwrap();
        let res = app.oneshot(req).await.unwrap();
        assert_eq!(res.status(), 200);
        let ct = res.headers().get("content-type").unwrap().to_str().unwrap();
        assert_eq!(ct, "text/css");
    }

    #[tokio::test]
    async fn test_chat_endpoint() {
        let app = build_router();
        let req = Request::builder()
            .method("POST")
            .uri("/api/chat")
            .header("content-type", "application/json")
            .body(Body::from(r#"{"message":"hello"}"#))
            .unwrap();
        let res = app.oneshot(req).await.unwrap();
        assert_eq!(res.status(), 200);
        let body = res.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(json["response"].as_str().unwrap().contains("hello"));
        assert_eq!(json["model"], "preview");
    }

    #[tokio::test]
    async fn test_sessions_list_endpoint() {
        let json = get_json("/api/sessions").await;
        assert!(json.as_array().unwrap().len() >= 2);
    }

    #[tokio::test]
    async fn test_session_detail_endpoint() {
        let json = get_json("/api/sessions/test-123").await;
        assert_eq!(json["id"], "test-123");
        assert_eq!(json["status"], "active");
    }

    #[tokio::test]
    async fn test_memories_list_endpoint() {
        let json = get_json("/api/memories").await;
        assert!(json.as_array().unwrap().len() >= 3);
    }

    #[tokio::test]
    async fn test_memories_search_endpoint() {
        let json = get_json("/api/memories/search?q=main").await;
        assert_eq!(json["query"], "main");
        assert!(json["results"].as_array().unwrap().len() >= 1);
    }

    #[tokio::test]
    async fn test_mcp_servers_endpoint() {
        let json = get_json("/api/mcp/servers").await;
        let servers = json.as_array().unwrap();
        assert_eq!(servers.len(), 2);
        assert_eq!(servers[0]["name"], "filesystem");
    }

    #[tokio::test]
    async fn test_themes_endpoint() {
        let json = get_json("/api/themes").await;
        let themes = json.as_array().unwrap();
        assert_eq!(themes.len(), 12);
        assert!(themes.contains(&serde_json::json!("cyan")));
        assert!(themes.contains(&serde_json::json!("slate")));
    }
}
