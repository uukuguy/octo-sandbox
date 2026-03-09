use octo_engine::context::ContextFork;
use octo_types::ChatMessage;

#[test]
fn test_fork_from_parent() {
    let parent = vec![
        ChatMessage::user("hello"),
        ChatMessage::assistant("hi there"),
        ChatMessage::user("how are you?"),
    ];

    let fork = ContextFork::from_parent(&parent, None, None);

    assert_eq!(fork.len(), 3);
    assert_eq!(fork.messages()[0].text_content(), "hello");
    assert_eq!(fork.messages()[1].text_content(), "hi there");
    assert_eq!(fork.messages()[2].text_content(), "how are you?");
}

#[test]
fn test_fork_with_max_messages() {
    let parent = vec![
        ChatMessage::user("msg1"),
        ChatMessage::assistant("msg2"),
        ChatMessage::user("msg3"),
        ChatMessage::assistant("msg4"),
        ChatMessage::user("msg5"),
    ];

    // Only take last 2 messages
    let fork = ContextFork::from_parent(&parent, None, Some(2));

    assert_eq!(fork.len(), 2);
    assert_eq!(fork.messages()[0].text_content(), "msg4");
    assert_eq!(fork.messages()[1].text_content(), "msg5");
}

#[test]
fn test_fork_empty() {
    let fork = ContextFork::empty();

    assert!(fork.is_empty());
    assert_eq!(fork.len(), 0);
    assert!(fork.messages().is_empty());
    assert!(fork.system_prompt().is_none());
}

#[test]
fn test_fork_push_message() {
    let mut fork = ContextFork::empty();

    fork.push_message(ChatMessage::user("new message"));
    assert_eq!(fork.len(), 1);
    assert_eq!(fork.messages()[0].text_content(), "new message");

    fork.push_message(ChatMessage::assistant("response"));
    assert_eq!(fork.len(), 2);
}

#[test]
fn test_fork_isolation() {
    let parent = vec![
        ChatMessage::user("original1"),
        ChatMessage::assistant("original2"),
    ];

    let mut fork = ContextFork::from_parent(&parent, None, None);

    // Mutating the fork should not affect the parent vec
    fork.push_message(ChatMessage::user("fork-only message"));
    assert_eq!(fork.len(), 3);

    // Parent is unchanged (it was cloned into the fork)
    assert_eq!(parent.len(), 2);
}

#[test]
fn test_fork_new_messages() {
    let parent = vec![ChatMessage::user("msg1"), ChatMessage::assistant("msg2")];

    let original_count = parent.len();
    let mut fork = ContextFork::from_parent(&parent, None, None);

    // No new messages yet
    assert!(fork.new_messages(original_count).is_empty());

    // Add messages after fork
    fork.push_message(ChatMessage::user("new1"));
    fork.push_message(ChatMessage::assistant("new2"));

    let new_msgs = fork.new_messages(original_count);
    assert_eq!(new_msgs.len(), 2);
    assert_eq!(new_msgs[0].text_content(), "new1");
    assert_eq!(new_msgs[1].text_content(), "new2");
}

#[test]
fn test_fork_with_system_prompt() {
    let parent = vec![ChatMessage::user("hello")];

    let fork = ContextFork::from_parent(
        &parent,
        Some("You are a helpful assistant.".to_string()),
        None,
    );

    assert_eq!(fork.system_prompt(), Some("You are a helpful assistant."));
    assert_eq!(fork.len(), 1);
}
