#[test]
fn crate_compiles() {
    assert_eq!(2 + 2, 4);
}

#[test]
fn server_type_exists() {
    use eaasp_scoped_hook_mcp::Server;
    assert!(std::any::type_name::<Server>().contains("Server"));
}
