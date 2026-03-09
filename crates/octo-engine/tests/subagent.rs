use octo_engine::agent::{
    SubAgentManager, SubAgentStatus, SubAgentTask,
};
use octo_types::ChatMessage;

#[tokio::test]
async fn test_subagent_manager_creation() {
    let mgr = SubAgentManager::new(4, 3);
    assert_eq!(mgr.depth(), 0);
    assert_eq!(mgr.max_depth(), 3);
    assert!(mgr.can_spawn().await);
    assert!(mgr.list().await.is_empty());
}

#[tokio::test]
async fn test_subagent_register_and_list() {
    let mgr = SubAgentManager::new(4, 3);
    mgr.register("sa-1".into(), "first task".into()).await.unwrap();
    mgr.register("sa-2".into(), "second task".into()).await.unwrap();

    let agents = mgr.list().await;
    assert_eq!(agents.len(), 2);

    let ids: Vec<&str> = agents.iter().map(|h| h.id.as_str()).collect();
    assert!(ids.contains(&"sa-1"));
    assert!(ids.contains(&"sa-2"));

    for h in &agents {
        assert_eq!(h.status, SubAgentStatus::Running);
    }
}

#[tokio::test]
async fn test_subagent_complete() {
    let mgr = SubAgentManager::new(4, 3);
    mgr.register("sa-1".into(), "do something".into()).await.unwrap();

    let result = mgr.complete("sa-1", Some("done!".into())).await.unwrap();
    assert_eq!(result.id, "sa-1");
    assert_eq!(result.status, SubAgentStatus::Completed);
    assert_eq!(result.output.as_deref(), Some("done!"));

    let agents = mgr.list().await;
    assert_eq!(agents[0].status, SubAgentStatus::Completed);
}

#[tokio::test]
async fn test_subagent_fail() {
    let mgr = SubAgentManager::new(4, 3);
    mgr.register("sa-1".into(), "failing task".into()).await.unwrap();

    mgr.fail("sa-1", "something broke".into()).await.unwrap();

    let agents = mgr.list().await;
    assert_eq!(
        agents[0].status,
        SubAgentStatus::Failed("something broke".into())
    );
}

#[tokio::test]
async fn test_subagent_cancel() {
    let mgr = SubAgentManager::new(4, 3);
    mgr.register("sa-1".into(), "cancellable task".into()).await.unwrap();

    mgr.cancel("sa-1").await.unwrap();

    let agents = mgr.list().await;
    assert_eq!(agents[0].status, SubAgentStatus::Cancelled);
}

#[tokio::test]
async fn test_subagent_cancel_all() {
    let mgr = SubAgentManager::new(4, 3);
    mgr.register("sa-1".into(), "task 1".into()).await.unwrap();
    mgr.register("sa-2".into(), "task 2".into()).await.unwrap();
    mgr.register("sa-3".into(), "task 3".into()).await.unwrap();

    // Complete one before cancel_all so it stays completed
    mgr.complete("sa-2", None).await.unwrap();

    mgr.cancel_all().await;

    let agents = mgr.list().await;
    for h in &agents {
        match h.id.as_str() {
            "sa-2" => assert_eq!(h.status, SubAgentStatus::Completed),
            _ => assert_eq!(h.status, SubAgentStatus::Cancelled),
        }
    }
}

#[tokio::test]
async fn test_subagent_max_concurrent() {
    let mgr = SubAgentManager::new(2, 5);
    mgr.register("sa-1".into(), "task 1".into()).await.unwrap();
    mgr.register("sa-2".into(), "task 2".into()).await.unwrap();

    // Third should fail — max concurrent is 2
    let err = mgr.register("sa-3".into(), "task 3".into()).await;
    assert!(err.is_err());
    assert!(
        err.unwrap_err().to_string().contains("Maximum concurrent sub-agents reached")
    );

    // Complete one, then we can register again
    mgr.complete("sa-1", None).await.unwrap();
    assert!(mgr.can_spawn().await);
    mgr.register("sa-3".into(), "task 3".into()).await.unwrap();
}

#[tokio::test]
async fn test_subagent_depth_limit() {
    let mgr = SubAgentManager::new(4, 3);
    assert_eq!(mgr.depth(), 0);

    let child1 = mgr.child().unwrap();
    assert_eq!(child1.depth(), 1);

    let child2 = child1.child().unwrap();
    assert_eq!(child2.depth(), 2);

    // depth 2 + 1 = 3 which equals max_depth(3), so it should fail
    let err = child2.child();
    assert!(err.is_err());
    assert!(
        err.unwrap_err().to_string().contains("recursion depth limit reached")
    );
}

#[tokio::test]
async fn test_subagent_child_manager() {
    let mgr = SubAgentManager::new(4, 5);
    mgr.register("parent-sa".into(), "parent task".into()).await.unwrap();

    let child = mgr.child().unwrap();
    assert_eq!(child.depth(), 1);
    assert_eq!(child.max_depth(), 5);

    // Child has its own independent active_agents
    assert!(child.list().await.is_empty());

    child.register("child-sa".into(), "child task".into()).await.unwrap();
    assert_eq!(child.list().await.len(), 1);

    // Parent still has only its own agent
    assert_eq!(mgr.list().await.len(), 1);
}

#[tokio::test]
async fn test_subagent_task_serialization() {
    let task = SubAgentTask {
        description: "Summarize the document".into(),
        context: vec![ChatMessage::user("Hello, summarize this.")],
        tools: Some(vec!["file_read".into(), "bash".into()]),
        max_iterations: 10,
    };

    let json = serde_json::to_string(&task).unwrap();
    let deserialized: SubAgentTask = serde_json::from_str(&json).unwrap();

    assert_eq!(deserialized.description, task.description);
    assert_eq!(deserialized.max_iterations, task.max_iterations);
    assert_eq!(deserialized.tools, task.tools);
    assert_eq!(deserialized.context.len(), 1);
}

#[tokio::test]
async fn test_subagent_not_found_errors() {
    let mgr = SubAgentManager::new(4, 3);

    let err = mgr.complete("nonexistent", None).await;
    assert!(err.is_err());
    assert!(err.unwrap_err().to_string().contains("SubAgent not found"));

    let err = mgr.fail("nonexistent", "oops".into()).await;
    assert!(err.is_err());

    let err = mgr.cancel("nonexistent").await;
    assert!(err.is_err());
}
