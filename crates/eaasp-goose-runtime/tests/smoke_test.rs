#[test]
fn crate_compiles() {
    assert_eq!(2 + 2, 4);
}

#[test]
fn goose_adapter_module_exists() {
    use eaasp_goose_runtime::goose_adapter;
    assert!(std::any::type_name::<goose_adapter::GooseAdapter>().contains("GooseAdapter"));
}
