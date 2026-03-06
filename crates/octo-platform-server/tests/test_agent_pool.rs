use octo_platform_server::agent_pool::{
    AgentPool, PoolConfig, IsolationStrategy,
};
use std::sync::Arc;
use std::time::Duration;

#[tokio::test]
async fn test_pool_creation() {
    let config = PoolConfig {
        soft_max_total: 3,
        hard_max_total: 5,
        min_idle: 0,
        max_idle: 2,
        idle_timeout: Duration::from_secs(60),
        strategy: IsolationStrategy::Memory,
        data_dir: None,
    };

    let pool = AgentPool::with_config(config);
    assert_eq!(pool.total_count(), 0);
}

#[tokio::test]
async fn test_pool_stats() {
    let config = PoolConfig::default();
    let pool = AgentPool::with_config(config);

    let stats = pool.stats().await;
    assert_eq!(stats.total, 0);
    assert_eq!(stats.idle, 0);
    assert_eq!(stats.busy, 0);
}

#[tokio::test]
async fn test_concurrent_users() {
    let config = PoolConfig {
        soft_max_total: 2,
        hard_max_total: 3,
        min_idle: 0,
        max_idle: 2,
        idle_timeout: Duration::from_secs(60),
        strategy: IsolationStrategy::Memory,
        data_dir: None,
    };

    // Wrap pool in Arc for sharing across tasks
    let pool = Arc::new(AgentPool::with_config(config));

    // Simulate multiple users getting instances concurrently
    let mut handles = vec![];
    for i in 0..3 {
        let pool = Arc::clone(&pool);
        let handle = tokio::spawn(async move {
            pool.get_instance(&format!("user_{}", i)).await
        });
        handles.push(handle);
    }

    // Wait for all results
    let results = futures_util::future::join_all(handles).await;

    // Verify results
    let mut success_count = 0;
    for result in results {
        if result.is_ok() {
            success_count += 1;
        }
    }

    // Hard limit is 3, all should succeed
    assert_eq!(success_count, 3);
}
