use octo_platform_server::agent_pool::{
    AgentPool, PoolConfig, IsolationStrategy,
};
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
