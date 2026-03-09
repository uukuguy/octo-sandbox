use octo_engine::agent::TurnGate;

#[tokio::test]
async fn test_turn_gate_mutual_exclusion() {
    let gate = TurnGate::new();
    assert!(!gate.is_busy());

    let guard1 = gate.acquire().await;
    assert!(gate.is_busy());
    assert!(gate.try_acquire().is_none());

    drop(guard1);
    assert!(!gate.is_busy());
    assert!(gate.try_acquire().is_some());
}

#[tokio::test]
async fn test_turn_gate_try_acquire() {
    let gate = TurnGate::new();
    let guard = gate.try_acquire();
    assert!(guard.is_some());
    assert!(gate.try_acquire().is_none());
    drop(guard);
    assert!(gate.try_acquire().is_some());
}

#[tokio::test]
async fn test_turn_gate_default() {
    let gate = TurnGate::default();
    assert!(!gate.is_busy());
}
