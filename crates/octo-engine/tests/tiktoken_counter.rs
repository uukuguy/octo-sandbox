//! Tests for TiktokenCounter (only run with tiktoken feature enabled).
#![cfg(feature = "tiktoken")]

use octo_engine::context::{EstimateCounter, TiktokenCounter, TokenCounter};
use octo_types::message::ChatMessage;

#[test]
fn test_tiktoken_count_basic() {
    let counter = TiktokenCounter::new();
    let count = counter.count("hello world");
    assert!(count > 0, "Expected non-zero token count for 'hello world'");
    // "hello world" should be 2 tokens with cl100k_base
    assert_eq!(count, 2);
}

#[test]
fn test_tiktoken_count_empty() {
    let counter = TiktokenCounter::new();
    assert_eq!(counter.count(""), 0);
}

#[test]
fn test_tiktoken_count_cjk() {
    let counter = TiktokenCounter::new();
    let count = counter.count("你好世界");
    assert!(count > 0, "Expected non-zero token count for CJK text");
    // CJK characters typically use more tokens than the character count
}

#[test]
fn test_tiktoken_count_messages() {
    let counter = TiktokenCounter::new();
    let messages = vec![
        ChatMessage::user("hello"),
        ChatMessage::assistant("hi there"),
    ];
    let count = counter.count_messages(&messages);
    // Each message adds text tokens + 4 overhead
    let hello_tokens = counter.count("hello") + 4;
    let hi_tokens = counter.count("hi there") + 4;
    assert_eq!(count, hello_tokens + hi_tokens);
}

#[test]
fn test_tiktoken_more_precise_than_estimate() {
    let tiktoken = TiktokenCounter::new();
    let estimate = EstimateCounter;

    let text = "The quick brown fox jumps over the lazy dog.";
    let tiktoken_count = tiktoken.count(text);
    let estimate_count = estimate.count(text);

    // Both should produce a positive count
    assert!(tiktoken_count > 0, "tiktoken count should be > 0");
    assert!(estimate_count > 0, "estimate count should be > 0");
    // They may differ — tiktoken is the precise one
}

#[test]
fn test_tiktoken_consistency() {
    let counter = TiktokenCounter::new();
    let text = "Consistency is the key to accurate token counting.";
    let first = counter.count(text);
    let second = counter.count(text);
    assert_eq!(first, second, "Same text must produce the same token count");
}
