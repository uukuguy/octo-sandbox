use eaasp_goose_runtime::goose_adapter::{GooseAdapter, SessionConfig};

#[tokio::test]
async fn adapter_start_and_close_session() {
    // Requires GOOSE_BIN env set, or skip if goose not installed on CI runner.
    if std::env::var("GOOSE_BIN").is_err() && which::which("goose").is_err() {
        eprintln!("skip: goose binary not available");
        return;
    }
    let adapter = GooseAdapter::new();
    let sid = adapter.start_session(SessionConfig::default()).await.unwrap();
    assert!(!sid.is_empty());
    adapter.close_session(&sid).await.unwrap();
}

#[tokio::test]
async fn adapter_creates_without_goose_binary() {
    // Unconditional smoke test: GooseAdapter::new() must construct even when
    // goose binary is absent. Proves the struct + default impl are well-formed.
    let _adapter = GooseAdapter::new();
    let _cfg = SessionConfig::default();
}
