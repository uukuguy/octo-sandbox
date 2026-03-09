use octo_engine::agent::{DeferredActionDetector, DeferredCategory, DeferredPattern};

#[test]
fn test_detect_postponed_action() {
    let detector = DeferredActionDetector::new();
    let matches = detector.detect("I'll handle that later when we have more time.");
    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0].category, DeferredCategory::PostponedAction);
    assert_eq!(matches[0].text, "I'll handle that later");
}

#[test]
fn test_detect_deferred_return() {
    let detector = DeferredActionDetector::new();
    let matches = detector.detect("Let me come back to this after we finish the main task.");
    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0].category, DeferredCategory::DeferredReturn);
}

#[test]
fn test_detect_scope_defer() {
    let detector = DeferredActionDetector::new();
    let matches = detector.detect("Let's defer that to a follow-up PR.");
    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0].category, DeferredCategory::ScopeDefer);
}

#[test]
fn test_no_deferred_in_normal_text() {
    let detector = DeferredActionDetector::new();
    let matches =
        detector.detect("Here is the implementation of the authentication module as requested.");
    assert!(matches.is_empty());
}

#[test]
fn test_multiple_deferrals() {
    let detector = DeferredActionDetector::new();
    let text = "I'll handle that later. Also, let me come back to the error handling. \
                Finally, let's defer that to phase 2.";
    let matches = detector.detect(text);
    assert!(matches.len() >= 3, "expected at least 3, got {}", matches.len());

    let categories: Vec<&DeferredCategory> = matches.iter().map(|m| &m.category).collect();
    assert!(categories.contains(&&DeferredCategory::PostponedAction));
    assert!(categories.contains(&&DeferredCategory::DeferredReturn));
    assert!(categories.contains(&&DeferredCategory::ScopeDefer));
}

#[test]
fn test_case_insensitive() {
    let detector = DeferredActionDetector::new();
    let matches = detector.detect("I'LL HANDLE THAT LATER");
    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0].category, DeferredCategory::PostponedAction);
}

#[test]
fn test_has_deferred_convenience() {
    let detector = DeferredActionDetector::new();
    assert!(detector.has_deferred("I'll handle that later."));
    assert!(!detector.has_deferred("Everything is done."));
}

#[test]
fn test_custom_patterns() {
    let patterns = vec![DeferredPattern {
        pattern: "todo later".to_string(),
        category: DeferredCategory::PostponedAction,
    }];
    let detector = DeferredActionDetector::with_patterns(patterns);

    assert!(detector.has_deferred("This is a todo later item."));
    // Default patterns should NOT be present
    assert!(!detector.has_deferred("I'll handle that later."));
}
