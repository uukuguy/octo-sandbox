//! Comprehensive tests for offline sync (D6): HLC, ChangeTracker, LWW, protocol, server.

use std::sync::Arc;
use octo_engine::db::Database;
use octo_engine::sync::changelog::ChangeTracker;
use octo_engine::sync::hlc::{HlcTimestamp, HybridClock};
use octo_engine::sync::lww::LwwResolver;
use octo_engine::sync::protocol::*;
use octo_engine::sync::server::SyncServer;
use serde_json::json;

async fn setup_db() -> tokio_rusqlite::Connection {
    Database::open_in_memory().await.unwrap().conn().clone()
}

async fn setup_tracker(dev: &str) -> (tokio_rusqlite::Connection, Arc<HybridClock>, ChangeTracker) {
    let conn = setup_db().await;
    let clock = Arc::new(HybridClock::new(dev.to_string()));
    let tracker = ChangeTracker::new(conn.clone(), clock.clone(), dev.to_string());
    (conn, clock, tracker)
}

fn ts(phys: i64, log: u32, node: &str) -> HlcTimestamp {
    HlcTimestamp { physical_ms: phys, logical: log, node_id: node.to_string() }
}

fn mk(table: &str, row: &str, op: SyncOperation, data: serde_json::Value, t: HlcTimestamp, dev: &str) -> SyncChange {
    SyncChange {
        id: None, table_name: table.into(), row_id: row.into(),
        operation: op, data, hlc_timestamp: t, device_id: dev.into(), sync_version: 0,
    }
}

// ─── HLC ───

mod hlc_tests {
    use super::*;

    #[test]
    fn monotonic() {
        let clock = HybridClock::new("n1".into());
        let mut prev = clock.now();
        for _ in 0..100 {
            let next = clock.now();
            assert!(next > prev, "HLC must be strictly monotonic");
            prev = next;
        }
    }

    #[test]
    fn causality() {
        let ca = HybridClock::new("a".into());
        let cb = HybridClock::new("b".into());
        let ta = ca.now();
        cb.update(&ta);
        assert!(cb.now() > ta, "After update, b must exceed a");
    }

    #[test]
    fn ordering_physical_dominates() {
        assert!(ts(2000, 0, "z") > ts(1000, 99, "a"));
    }

    #[test]
    fn ordering_logical_tiebreaker() {
        assert!(ts(5000, 2, "a") > ts(5000, 1, "a"));
    }

    #[test]
    fn ordering_node_id_tiebreaker() {
        let a = ts(5000, 1, "a");
        let b = ts(5000, 1, "b");
        assert_ne!(a, b);
        assert!(a < b || b < a, "Total ordering required");
    }

    #[test]
    fn display_fromstr_roundtrip() {
        let t = ts(1_700_000_000_000, 42, "dev-xyz");
        let parsed: HlcTimestamp = t.to_string().parse().unwrap();
        assert_eq!(parsed.physical_ms, t.physical_ms);
        assert_eq!(parsed.logical, t.logical);
        assert_eq!(parsed.node_id, t.node_id);
    }

    #[test]
    fn json_roundtrip() {
        let t = ts(1_700_000_000_000, 7, "m1");
        let back: HlcTimestamp = serde_json::from_str(&serde_json::to_string(&t).unwrap()).unwrap();
        assert_eq!(back, t);
    }

    #[test]
    fn node_id_accessor() {
        assert_eq!(HybridClock::new("dev".into()).node_id(), "dev");
    }
}

// ─── ChangeTracker ───

mod changelog_tests {
    use super::*;

    #[tokio::test]
    async fn record_insert() {
        let (_, _, tr) = setup_tracker("d1").await;
        let id = tr.record_change("sessions", "r1", SyncOperation::Insert, None, Some(json!({"n":"S"}))).await.unwrap();
        assert!(id > 0);
        let c = tr.get_unsynced_changes(10).await.unwrap();
        assert_eq!(c.len(), 1);
        assert_eq!(c[0].table_name, "sessions");
        assert_eq!(c[0].device_id, "d1");
    }

    #[tokio::test]
    async fn record_update() {
        let (_, _, tr) = setup_tracker("d2").await;
        tr.record_change("mem", "m5", SyncOperation::Update, Some(json!({"c":"old"})), Some(json!({"c":"new"}))).await.unwrap();
        let c = tr.get_unsynced_changes(10).await.unwrap();
        assert_eq!(c.len(), 1);
        assert_eq!(c[0].row_id, "m5");
    }

    #[tokio::test]
    async fn record_delete() {
        let (_, _, tr) = setup_tracker("d3").await;
        tr.record_change("sessions", "r99", SyncOperation::Delete, Some(json!({"n":"X"})), None).await.unwrap();
        assert_eq!(tr.get_unsynced_changes(10).await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn unsynced_respects_limit() {
        let (_, _, tr) = setup_tracker("d4").await;
        for i in 0..5 {
            tr.record_change("t", &format!("r{i}"), SyncOperation::Insert, None, Some(json!({"i":i}))).await.unwrap();
        }
        assert_eq!(tr.get_unsynced_changes(3).await.unwrap().len(), 3);
        assert_eq!(tr.get_unsynced_changes(100).await.unwrap().len(), 5);
    }

    #[tokio::test]
    async fn mark_synced() {
        let (_, _, tr) = setup_tracker("d5").await;
        let id1 = tr.record_change("t", "r1", SyncOperation::Insert, None, Some(json!({}))).await.unwrap();
        let id2 = tr.record_change("t", "r2", SyncOperation::Insert, None, Some(json!({}))).await.unwrap();
        tr.record_change("t", "r3", SyncOperation::Insert, None, Some(json!({}))).await.unwrap();
        tr.mark_synced(&[id1, id2]).await.unwrap();
        let rem = tr.get_unsynced_changes(100).await.unwrap();
        assert_eq!(rem.len(), 1);
        assert_eq!(rem[0].row_id, "r3");
    }

    #[tokio::test]
    async fn changes_since_version() {
        let (_, _, tr) = setup_tracker("d6").await;
        for i in 0..4 {
            tr.record_change("s", &format!("r{i}"), SyncOperation::Insert, None, Some(json!({"i":i}))).await.unwrap();
        }
        assert_eq!(tr.get_changes_since(0).await.unwrap().len(), 4);
        assert!(tr.get_changes_since(2).await.unwrap().len() <= 2);
    }

    #[tokio::test]
    async fn cleanup_old_changes() {
        let (conn, _, tr) = setup_tracker("d7").await;
        let id1 = tr.record_change("t", "r1", SyncOperation::Insert, None, Some(json!({}))).await.unwrap();
        tr.record_change("t", "r2", SyncOperation::Insert, None, Some(json!({}))).await.unwrap();
        tr.mark_synced(&[id1]).await.unwrap();
        // Backdate the synced entry so cleanup_old_changes(0) can find it
        conn.call(move |c| {
            c.execute(
                "UPDATE sync_changelog SET hlc_physical_ms = hlc_physical_ms - 86400001 WHERE id = ?1",
                rusqlite::params![id1],
            )?;
            Ok(())
        }).await.unwrap();
        assert!(tr.cleanup_old_changes(0).await.unwrap() >= 1);
        assert!(tr.pending_count().await.unwrap() >= 1, "Unsynced must survive cleanup");
    }

    #[tokio::test]
    async fn pending_count() {
        let (_, _, tr) = setup_tracker("d8").await;
        assert_eq!(tr.pending_count().await.unwrap(), 0);
        for i in 0..3 {
            tr.record_change("t", &format!("r{i}"), SyncOperation::Insert, None, Some(json!({}))).await.unwrap();
        }
        assert_eq!(tr.pending_count().await.unwrap(), 3);
        let first = tr.get_unsynced_changes(1).await.unwrap()[0].id.unwrap();
        tr.mark_synced(&[first]).await.unwrap();
        assert_eq!(tr.pending_count().await.unwrap(), 2);
    }
}

// ─── LWW ───

mod lww_tests {
    use super::*;

    #[test]
    fn client_wins_newer() {
        let c = mk("s", "r1", SyncOperation::Update, json!({"v":"c"}), ts(2000,0,"c"), "c");
        let s = mk("s", "r1", SyncOperation::Update, json!({"v":"s"}), ts(1000,0,"s"), "s");
        assert!(matches!(LwwResolver::resolve(&c, &s), ConflictResolution::ClientWins));
    }

    #[test]
    fn server_wins_newer() {
        let c = mk("s", "r1", SyncOperation::Update, json!({"v":"c"}), ts(1000,0,"c"), "c");
        let s = mk("s", "r1", SyncOperation::Update, json!({"v":"s"}), ts(2000,0,"s"), "s");
        assert!(matches!(LwwResolver::resolve(&c, &s), ConflictResolution::ServerWins));
    }

    #[test]
    fn delete_wins_over_update() {
        let del = mk("s", "r1", SyncOperation::Delete, json!(null), ts(1000,0,"c"), "c");
        let upd = mk("s", "r1", SyncOperation::Update, json!({"v":"s"}), ts(2000,0,"s"), "s");
        assert!(matches!(LwwResolver::resolve(&del, &upd), ConflictResolution::ClientWins));
    }

    #[test]
    fn insert_conflict_newer_wins() {
        let c = mk("s", "r1", SyncOperation::Insert, json!({"n":"C"}), ts(3000,0,"c"), "c");
        let s = mk("s", "r1", SyncOperation::Insert, json!({"n":"S"}), ts(2000,0,"s"), "s");
        assert!(matches!(LwwResolver::resolve(&c, &s), ConflictResolution::ClientWins));
    }

    #[tokio::test]
    async fn apply_remote_inserts() {
        let conn = setup_db().await;
        let clock = Arc::new(HybridClock::new("srv".into()));
        let changes = vec![mk("sessions", "nr", SyncOperation::Insert, json!({"name":"R","id":"nr"}), clock.now(), "rem")];
        let conflicts = LwwResolver::apply_remote_changes(&conn, &changes, &clock).await.unwrap();
        assert!(conflicts.is_empty());
    }

    #[tokio::test]
    async fn apply_remote_with_conflict() {
        let conn = setup_db().await;
        let clock = Arc::new(HybridClock::new("srv".into()));
        let init = vec![mk("sessions", "cr", SyncOperation::Insert, json!({"name":"A"}), clock.now(), "da")];
        LwwResolver::apply_remote_changes(&conn, &init, &clock).await.unwrap();
        let dup = vec![mk("sessions", "cr", SyncOperation::Insert, json!({"name":"B"}), clock.now(), "db")];
        let conflicts = LwwResolver::apply_remote_changes(&conn, &dup, &clock).await.unwrap();
        assert!(conflicts.len() <= 1);
    }
}

// ─── Protocol serialization ───

mod protocol_tests {
    use super::*;

    #[test]
    fn sync_change_roundtrip() {
        let c = SyncChange {
            id: Some(42), table_name: "mem".into(), row_id: "m1".into(),
            operation: SyncOperation::Update, data: json!({"c":"hi","l":2}),
            hlc_timestamp: ts(1_700_000_000, 3, "dx"), device_id: "dx".into(), sync_version: 5,
        };
        let back: SyncChange = serde_json::from_str(&serde_json::to_string(&c).unwrap()).unwrap();
        assert_eq!(back.table_name, "mem");
        assert_eq!(back.sync_version, 5);
    }

    #[test]
    fn pull_roundtrip() {
        let req = SyncPullRequest { since_version: 10, limit: 50 };
        let b: SyncPullRequest = serde_json::from_str(&serde_json::to_string(&req).unwrap()).unwrap();
        assert_eq!(b.since_version, 10);
        let resp = SyncPullResponse { changes: vec![], server_version: 42, has_more: false };
        let r: SyncPullResponse = serde_json::from_str(&serde_json::to_string(&resp).unwrap()).unwrap();
        assert_eq!(r.server_version, 42);
    }

    #[test]
    fn push_roundtrip() {
        let req = SyncPushRequest { changes: vec![], device_id: "p1".into(), client_version: 7 };
        let b: SyncPushRequest = serde_json::from_str(&serde_json::to_string(&req).unwrap()).unwrap();
        assert_eq!(b.device_id, "p1");
        let resp = SyncPushResponse {
            applied: 3, server_version: 15,
            conflicts: vec![SyncConflict {
                table_name: "s".into(), row_id: "r1".into(),
                client_value: json!({"a":1}), server_value: json!({"a":2}),
                resolution: ConflictResolution::ServerWins,
            }],
        };
        let r: SyncPushResponse = serde_json::from_str(&serde_json::to_string(&resp).unwrap()).unwrap();
        assert_eq!(r.conflicts.len(), 1);
    }

    #[test]
    fn status_roundtrip() {
        let s = SyncStatus { device_id: "l1".into(), last_sync_at: Some("2026-03-11T12:00:00Z".into()), sync_version: 100, pending_changes: 5 };
        let b: SyncStatus = serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
        assert_eq!(b.sync_version, 100);
        assert_eq!(b.pending_changes, 5);
    }
}

// ─── SyncServer ───

mod server_tests {
    use super::*;

    async fn mk_server(dev: &str) -> SyncServer {
        let (conn, clock, tr) = setup_tracker(dev).await;
        SyncServer::new(conn, clock, Arc::new(tr))
    }

    #[tokio::test]
    async fn pull_empty() {
        let srv = mk_server("srv").await;
        let r = srv.handle_pull(SyncPullRequest { since_version: 0, limit: 100 }).await.unwrap();
        assert!(r.changes.is_empty());
        assert!(!r.has_more);
    }

    #[tokio::test]
    async fn push_then_pull() {
        let srv = mk_server("srv").await;
        let clock = HybridClock::new("ca".into());
        let changes = vec![mk("sessions", "s1", SyncOperation::Insert, json!({"name":"S1"}), clock.now(), "ca")];
        let pr = srv.handle_push(SyncPushRequest { changes, device_id: "ca".into(), client_version: 0 }).await.unwrap();
        assert!(pr.applied >= 1);
        let pl = srv.handle_pull(SyncPullRequest { since_version: 0, limit: 100 }).await.unwrap();
        assert!(!pl.changes.is_empty());
    }

    #[tokio::test]
    async fn status_unknown_device() {
        let srv = mk_server("srv").await;
        let st = srv.get_status("unknown").await.unwrap();
        assert_eq!(st.device_id, "unknown");
        assert_eq!(st.sync_version, 0);
        assert!(st.last_sync_at.is_none());
    }
}

// ─── Full integration ───

mod integration_tests {
    use super::*;

    #[tokio::test]
    async fn full_sync_cycle() {
        // Server
        let (sc, sclk, str_) = setup_tracker("server").await;
        let server = SyncServer::new(sc, sclk.clone(), Arc::new(str_));

        // Device A records changes
        let (_, _, tra) = setup_tracker("dev-a").await;
        tra.record_change("mem", "m1", SyncOperation::Insert, None, Some(json!({"c":"Remember"}))).await.unwrap();
        tra.record_change("sessions", "s1", SyncOperation::Insert, None, Some(json!({"n":"Work"}))).await.unwrap();
        assert_eq!(tra.pending_count().await.unwrap(), 2);

        // Push to server
        let unsynced = tra.get_unsynced_changes(100).await.unwrap();
        let pr = server.handle_push(SyncPushRequest {
            changes: unsynced.clone(), device_id: "dev-a".into(), client_version: 0,
        }).await.unwrap();
        assert!(pr.applied >= 2);

        // Mark synced
        let ids: Vec<i64> = unsynced.iter().filter_map(|c| c.id).collect();
        tra.mark_synced(&ids).await.unwrap();
        assert_eq!(tra.pending_count().await.unwrap(), 0);

        // Device B pulls
        let pl = server.handle_pull(SyncPullRequest { since_version: 0, limit: 100 }).await.unwrap();
        assert!(pl.changes.len() >= 2);
        assert!(pl.server_version > 0);
    }
}
