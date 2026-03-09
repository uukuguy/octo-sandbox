use octo_types::ToolProgress;

#[test]
fn test_indeterminate_progress() {
    let p = ToolProgress::indeterminate("loading...");
    assert!(p.fraction.is_none());
    assert_eq!(p.message, "loading...");
    assert!(p.bytes_processed.is_none());
    assert!(p.bytes_total.is_none());
    assert_eq!(p.elapsed_ms, 0);
}

#[test]
fn test_percent_progress() {
    let p = ToolProgress::percent(0.5, "halfway");
    assert_eq!(p.fraction, Some(0.5));
    assert_eq!(p.message, "halfway");
}

#[test]
fn test_with_bytes() {
    let p = ToolProgress::percent(0.25, "downloading").with_bytes(250, 1000);
    assert_eq!(p.bytes_processed, Some(250));
    assert_eq!(p.bytes_total, Some(1000));
}

#[test]
fn test_is_complete() {
    assert!(ToolProgress::percent(1.0, "done").is_complete());
    assert!(ToolProgress::percent(1.5, "over").is_complete());
    assert!(!ToolProgress::percent(0.5, "half").is_complete());
    assert!(!ToolProgress::indeterminate("unknown").is_complete());
}

#[test]
fn test_progress_serialization() {
    let original = ToolProgress::percent(0.75, "processing")
        .with_bytes(750, 1000)
        .with_elapsed(1234);

    let json = serde_json::to_string(&original).expect("serialize");
    let restored: ToolProgress = serde_json::from_str(&json).expect("deserialize");

    assert_eq!(restored.fraction, original.fraction);
    assert_eq!(restored.message, original.message);
    assert_eq!(restored.bytes_processed, original.bytes_processed);
    assert_eq!(restored.bytes_total, original.bytes_total);
    assert_eq!(restored.elapsed_ms, original.elapsed_ms);
}

#[test]
fn test_builder_chain() {
    let p = ToolProgress::percent(0.5, "half")
        .with_bytes(50, 100)
        .with_elapsed(500);

    assert_eq!(p.fraction, Some(0.5));
    assert_eq!(p.message, "half");
    assert_eq!(p.bytes_processed, Some(50));
    assert_eq!(p.bytes_total, Some(100));
    assert_eq!(p.elapsed_ms, 500);
}
