use octo_types::ChatMessage;

// We test the principle: provider errors must not be added to messages.
// The function takes &[ChatMessage] (immutable), enforcing non-persistence at the type level.

#[test]
fn test_provider_error_not_added_to_messages() {
    let messages: Vec<ChatMessage> = vec![];
    let original_len = messages.len();

    // Simulate what the handler does — it takes &[ChatMessage], not &mut Vec
    let is_retryable = octo_engine::agent::loop_::handle_provider_error_non_persistent(
        "503 Service Unavailable",
        &messages,
    );

    // Messages length must NOT change — error is not persisted
    assert_eq!(messages.len(), original_len);
    assert!(is_retryable); // 503 is retryable
}

#[test]
fn test_auth_error_not_retryable() {
    let messages: Vec<ChatMessage> = vec![];
    let is_retryable = octo_engine::agent::loop_::handle_provider_error_non_persistent(
        "401 unauthorized",
        &messages,
    );
    assert!(!is_retryable);
    assert!(messages.is_empty()); // Still not persisted
}

#[test]
fn test_rate_limit_retryable_not_persisted() {
    let messages: Vec<ChatMessage> = vec![];
    let is_retryable = octo_engine::agent::loop_::handle_provider_error_non_persistent(
        "429 rate limit exceeded",
        &messages,
    );
    assert!(is_retryable);
    assert!(messages.is_empty());
}

#[test]
fn test_timeout_retryable_not_persisted() {
    let messages: Vec<ChatMessage> = vec![];
    let is_retryable = octo_engine::agent::loop_::handle_provider_error_non_persistent(
        "connection timeout",
        &messages,
    );
    assert!(is_retryable);
    assert!(messages.is_empty());
}

#[test]
fn test_billing_error_not_retryable() {
    let messages: Vec<ChatMessage> = vec![];
    let is_retryable = octo_engine::agent::loop_::handle_provider_error_non_persistent(
        "402 credit_balance too low",
        &messages,
    );
    assert!(!is_retryable);
    assert!(messages.is_empty());
}

#[test]
fn test_context_overflow_not_retryable() {
    let messages: Vec<ChatMessage> = vec![];
    let is_retryable = octo_engine::agent::loop_::handle_provider_error_non_persistent(
        "context_length exceeded",
        &messages,
    );
    assert!(!is_retryable);
    assert!(messages.is_empty());
}

#[test]
fn test_overloaded_retryable_not_persisted() {
    let messages: Vec<ChatMessage> = vec![];
    let is_retryable = octo_engine::agent::loop_::handle_provider_error_non_persistent(
        "529 overloaded",
        &messages,
    );
    assert!(is_retryable);
    assert!(messages.is_empty());
}

#[test]
fn test_with_existing_messages_not_modified() {
    let messages = vec![ChatMessage::user("Hello, world!")];
    let original_len = messages.len();

    let is_retryable = octo_engine::agent::loop_::handle_provider_error_non_persistent(
        "500 internal server error",
        &messages,
    );

    assert!(is_retryable);
    assert_eq!(messages.len(), original_len);
    // Verify the existing message is untouched
    assert_eq!(messages[0].role, octo_types::MessageRole::User);
}
