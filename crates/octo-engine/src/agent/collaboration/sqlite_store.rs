//! SQLite-backed implementation of [`ByzantineStore`].

use anyhow::{Context, Result};
use async_trait::async_trait;
use rusqlite::OptionalExtension;
use tokio_rusqlite::Connection;

use octo_types::SessionId;

use super::consensus::{
    ByzantineProposal, ConsensusPhase, ConsensusVote, ViewChangeRequest, ViewChangeTracker,
    ViewState,
};
use super::crypto::{ConsensusKeypair, SignedMessage};
use super::persistence::{ByzantineStore, SignatureRecord};

/// SQLite-backed Byzantine consensus store.
pub struct SqliteByzantineStore {
    conn: Connection,
}

impl SqliteByzantineStore {
    /// Creates a new store backed by the given connection.
    pub fn new(conn: Connection) -> Self {
        Self { conn }
    }
}

fn phase_to_str(phase: &ConsensusPhase) -> &'static str {
    match phase {
        ConsensusPhase::PrePrepare => "PrePrepare",
        ConsensusPhase::Prepare => "Prepare",
        ConsensusPhase::Commit => "Commit",
        ConsensusPhase::Finalized => "Finalized",
        ConsensusPhase::Failed => "Failed",
    }
}

fn str_to_phase(s: &str) -> ConsensusPhase {
    match s {
        "PrePrepare" => ConsensusPhase::PrePrepare,
        "Prepare" => ConsensusPhase::Prepare,
        "Commit" => ConsensusPhase::Commit,
        "Finalized" => ConsensusPhase::Finalized,
        "Failed" => ConsensusPhase::Failed,
        _ => ConsensusPhase::PrePrepare,
    }
}

fn now_iso8601() -> String {
    chrono::Utc::now().to_rfc3339()
}

#[async_trait]
impl ByzantineStore for SqliteByzantineStore {
    async fn save_proposal(
        &self,
        session_id: &SessionId,
        collaboration_id: &str,
        proposal: &ByzantineProposal,
    ) -> Result<()> {
        let sid = session_id.as_str().to_string();
        let cid = collaboration_id.to_string();
        let p = proposal.clone();
        self.conn
            .call(move |conn| {
                let prepare_json = serde_json::to_string(&p.prepare_votes)
                    .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
                let commit_json = serde_json::to_string(&p.commit_votes)
                    .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
                conn.execute(
                    "INSERT OR REPLACE INTO byzantine_proposals
                     (id, session_id, collaboration_id, proposer, action, description,
                      phase, prepare_votes, commit_votes, total_agents, created_at, finalized_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
                    rusqlite::params![
                        p.id,
                        sid,
                        cid,
                        p.proposer,
                        p.action,
                        p.description,
                        phase_to_str(&p.phase),
                        prepare_json,
                        commit_json,
                        p.total_agents as i64,
                        p.created_at,
                        p.finalized_at,
                    ],
                )?;
                Ok(())
            })
            .await
            .map_err(|e| anyhow::anyhow!("{}", e))
    }

    async fn load_proposal(
        &self,
        session_id: &SessionId,
        proposal_id: &str,
    ) -> Result<Option<ByzantineProposal>> {
        let sid = session_id.as_str().to_string();
        let pid = proposal_id.to_string();
        self.conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT id, proposer, action, description, phase,
                            prepare_votes, commit_votes, total_agents, created_at, finalized_at
                     FROM byzantine_proposals WHERE id = ?1 AND session_id = ?2",
                )?;
                let result = stmt
                    .query_row(rusqlite::params![pid, sid], |row| {
                        let phase_str: String = row.get(4)?;
                        let prepare_json: String = row.get(5)?;
                        let commit_json: String = row.get(6)?;
                        let total: i64 = row.get(7)?;
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, String>(2)?,
                            row.get::<_, String>(3)?,
                            phase_str,
                            prepare_json,
                            commit_json,
                            total,
                            row.get::<_, String>(8)?,
                            row.get::<_, Option<String>>(9)?,
                        ))
                    })
                    .optional()?;

                match result {
                    None => Ok(None),
                    Some((id, proposer, action, desc, phase_str, prep, comm, total, created, fin)) => {
                        let prepare_votes: Vec<ConsensusVote> =
                            serde_json::from_str(&prep).unwrap_or_default();
                        let commit_votes: Vec<ConsensusVote> =
                            serde_json::from_str(&comm).unwrap_or_default();
                        Ok(Some(ByzantineProposal {
                            id,
                            phase: str_to_phase(&phase_str),
                            proposer,
                            action,
                            description: desc,
                            prepare_votes,
                            commit_votes,
                            total_agents: total as usize,
                            created_at: created,
                            finalized_at: fin,
                        }))
                    }
                }
            })
            .await
            .map_err(|e| anyhow::anyhow!("{}", e))
    }

    async fn list_proposals(
        &self,
        session_id: &SessionId,
        collaboration_id: &str,
        phase_filter: Option<ConsensusPhase>,
    ) -> Result<Vec<ByzantineProposal>> {
        let sid = session_id.as_str().to_string();
        let cid = collaboration_id.to_string();
        let phase_str = phase_filter.as_ref().map(phase_to_str).map(String::from);
        self.conn
            .call(move |conn| {
                let (sql, params): (String, Vec<Box<dyn rusqlite::types::ToSql>>) =
                    if let Some(ref ps) = phase_str {
                        (
                            "SELECT id, proposer, action, description, phase,
                                    prepare_votes, commit_votes, total_agents, created_at, finalized_at
                             FROM byzantine_proposals
                             WHERE session_id = ?1 AND collaboration_id = ?2 AND phase = ?3"
                                .to_string(),
                            vec![
                                Box::new(sid.clone()),
                                Box::new(cid.clone()),
                                Box::new(ps.clone()),
                            ],
                        )
                    } else {
                        (
                            "SELECT id, proposer, action, description, phase,
                                    prepare_votes, commit_votes, total_agents, created_at, finalized_at
                             FROM byzantine_proposals
                             WHERE session_id = ?1 AND collaboration_id = ?2"
                                .to_string(),
                            vec![Box::new(sid.clone()), Box::new(cid.clone())],
                        )
                    };

                let param_refs: Vec<&dyn rusqlite::types::ToSql> =
                    params.iter().map(|p| p.as_ref()).collect();
                let mut stmt = conn.prepare(&sql)?;
                let rows = stmt.query_map(param_refs.as_slice(), |row| {
                    let phase_s: String = row.get(4)?;
                    let prep_j: String = row.get(5)?;
                    let comm_j: String = row.get(6)?;
                    let total: i64 = row.get(7)?;
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        phase_s,
                        prep_j,
                        comm_j,
                        total,
                        row.get::<_, String>(8)?,
                        row.get::<_, Option<String>>(9)?,
                    ))
                })?;

                let mut results = Vec::new();
                for r in rows {
                    let (id, proposer, action, desc, phase_s, prep_j, comm_j, total, created, fin) =
                        r?;
                    results.push(ByzantineProposal {
                        id,
                        phase: str_to_phase(&phase_s),
                        proposer,
                        action,
                        description: desc,
                        prepare_votes: serde_json::from_str(&prep_j).unwrap_or_default(),
                        commit_votes: serde_json::from_str(&comm_j).unwrap_or_default(),
                        total_agents: total as usize,
                        created_at: created,
                        finalized_at: fin,
                    });
                }
                Ok(results)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{}", e))
    }

    async fn update_proposal(
        &self,
        session_id: &SessionId,
        proposal: &ByzantineProposal,
    ) -> Result<()> {
        let sid = session_id.as_str().to_string();
        let p = proposal.clone();
        self.conn
            .call(move |conn| {
                let prepare_json = serde_json::to_string(&p.prepare_votes)
                    .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
                let commit_json = serde_json::to_string(&p.commit_votes)
                    .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
                conn.execute(
                    "UPDATE byzantine_proposals
                     SET phase = ?1, prepare_votes = ?2, commit_votes = ?3, finalized_at = ?4
                     WHERE id = ?5 AND session_id = ?6",
                    rusqlite::params![
                        phase_to_str(&p.phase),
                        prepare_json,
                        commit_json,
                        p.finalized_at,
                        p.id,
                        sid,
                    ],
                )?;
                Ok(())
            })
            .await
            .map_err(|e| anyhow::anyhow!("{}", e))
    }

    async fn delete_proposals(
        &self,
        session_id: &SessionId,
        collaboration_id: &str,
    ) -> Result<usize> {
        let sid = session_id.as_str().to_string();
        let cid = collaboration_id.to_string();
        self.conn
            .call(move |conn| {
                let count = conn.execute(
                    "DELETE FROM byzantine_proposals WHERE session_id = ?1 AND collaboration_id = ?2",
                    rusqlite::params![sid, cid],
                )?;
                Ok(count)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{}", e))
    }

    async fn save_view_state(
        &self,
        session_id: &SessionId,
        collaboration_id: &str,
        tracker: &ViewChangeTracker,
    ) -> Result<()> {
        let sid = session_id.as_str().to_string();
        let cid = collaboration_id.to_string();
        let view_number = tracker.state.view_number as i64;
        let leader = tracker.state.leader.clone();
        let agents_json = serde_json::to_string(&tracker.state.agents)
            .context("serialize agents")?;
        let timeout = tracker.timeout_ms as i64;
        let requests_json = serde_json::to_string(&tracker.requests)
            .context("serialize requests")?;
        let now = now_iso8601();

        self.conn
            .call(move |conn| {
                conn.execute(
                    "INSERT OR REPLACE INTO consensus_view_state
                     (session_id, collaboration_id, view_number, leader, agents,
                      timeout_ms, pending_requests, updated_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                    rusqlite::params![
                        sid,
                        cid,
                        view_number,
                        leader,
                        agents_json,
                        timeout,
                        requests_json,
                        now,
                    ],
                )?;
                Ok(())
            })
            .await
            .map_err(|e| anyhow::anyhow!("{}", e))
    }

    async fn load_view_state(
        &self,
        session_id: &SessionId,
        collaboration_id: &str,
    ) -> Result<Option<ViewChangeTracker>> {
        let sid = session_id.as_str().to_string();
        let cid = collaboration_id.to_string();
        self.conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT view_number, leader, agents, timeout_ms, pending_requests
                     FROM consensus_view_state
                     WHERE session_id = ?1 AND collaboration_id = ?2",
                )?;
                let result = stmt
                    .query_row(rusqlite::params![sid, cid], |row| {
                        Ok((
                            row.get::<_, i64>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, String>(2)?,
                            row.get::<_, i64>(3)?,
                            row.get::<_, String>(4)?,
                        ))
                    })
                    .optional()?;

                match result {
                    None => Ok(None),
                    Some((view_num, leader, agents_json, timeout, requests_json)) => {
                        let agents: Vec<String> =
                            serde_json::from_str(&agents_json).unwrap_or_default();
                        let requests: Vec<ViewChangeRequest> =
                            serde_json::from_str(&requests_json).unwrap_or_default();
                        let state = ViewState {
                            view_number: view_num as u64,
                            leader,
                            agents,
                        };
                        Ok(Some(ViewChangeTracker {
                            state,
                            requests,
                            timeout_ms: timeout as u64,
                        }))
                    }
                }
            })
            .await
            .map_err(|e| anyhow::anyhow!("{}", e))
    }

    async fn log_signature(
        &self,
        session_id: &SessionId,
        proposal_id: &str,
        signed_msg: &SignedMessage,
        phase: &str,
        approve: bool,
    ) -> Result<()> {
        let sid = session_id.as_str().to_string();
        let pid = proposal_id.to_string();
        let agent = signed_msg.agent_id.clone();
        let phase = phase.to_string();
        let sig = signed_msg.signature.clone();
        let pk = signed_msg.signer_public_key.clone();
        let payload = String::from_utf8_lossy(&signed_msg.payload).to_string();
        let now = now_iso8601();

        self.conn
            .call(move |conn| {
                conn.execute(
                    "INSERT INTO consensus_signatures
                     (session_id, proposal_id, agent_id, phase, approve,
                      signature, public_key, payload, created_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                    rusqlite::params![sid, pid, agent, phase, approve as i32, sig, pk, payload, now],
                )?;
                Ok(())
            })
            .await
            .map_err(|e| anyhow::anyhow!("{}", e))
    }

    async fn get_signatures(
        &self,
        session_id: &SessionId,
        proposal_id: &str,
    ) -> Result<Vec<SignatureRecord>> {
        let sid = session_id.as_str().to_string();
        let pid = proposal_id.to_string();
        self.conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT id, session_id, proposal_id, agent_id, phase, approve,
                            signature, public_key, payload, created_at
                     FROM consensus_signatures
                     WHERE session_id = ?1 AND proposal_id = ?2
                     ORDER BY id ASC",
                )?;
                let rows = stmt.query_map(rusqlite::params![sid, pid], |row| {
                    Ok(SignatureRecord {
                        id: row.get(0)?,
                        session_id: row.get(1)?,
                        proposal_id: row.get(2)?,
                        agent_id: row.get(3)?,
                        phase: row.get(4)?,
                        approve: row.get::<_, i32>(5)? != 0,
                        signature: row.get(6)?,
                        public_key: row.get(7)?,
                        payload: row.get(8)?,
                        created_at: row.get(9)?,
                    })
                })?;
                let mut results = Vec::new();
                for r in rows {
                    results.push(r?);
                }
                Ok(results)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{}", e))
    }

    async fn save_keypair(
        &self,
        session_id: &SessionId,
        agent_id: &str,
        keypair: &ConsensusKeypair,
        encryption_key: &[u8; 32],
    ) -> Result<()> {
        let (encrypted, nonce) = keypair
            .encrypt_private_key(encryption_key)
            .map_err(|e| anyhow::anyhow!("{}", e))?;
        let pk = keypair.public_key_bytes();
        let sid = session_id.as_str().to_string();
        let aid = agent_id.to_string();
        let now = now_iso8601();

        self.conn
            .call(move |conn| {
                conn.execute(
                    "INSERT OR REPLACE INTO consensus_keypairs
                     (agent_id, session_id, public_key, private_key_encrypted,
                      encryption_nonce, created_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    rusqlite::params![aid, sid, pk, encrypted, nonce, now],
                )?;
                Ok(())
            })
            .await
            .map_err(|e| anyhow::anyhow!("{}", e))
    }

    async fn load_keypair(
        &self,
        session_id: &SessionId,
        agent_id: &str,
        encryption_key: &[u8; 32],
    ) -> Result<Option<ConsensusKeypair>> {
        let sid = session_id.as_str().to_string();
        let aid = agent_id.to_string();
        let key = *encryption_key;
        self.conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT public_key, private_key_encrypted, encryption_nonce
                     FROM consensus_keypairs
                     WHERE agent_id = ?1 AND session_id = ?2",
                )?;
                let result = stmt
                    .query_row(rusqlite::params![aid, sid], |row| {
                        Ok((
                            row.get::<_, Vec<u8>>(0)?,
                            row.get::<_, Vec<u8>>(1)?,
                            row.get::<_, Vec<u8>>(2)?,
                        ))
                    })
                    .optional()?;

                match result {
                    None => Ok(None),
                    Some((pub_key, encrypted, nonce)) => {
                        let kp =
                            ConsensusKeypair::decrypt_and_restore(&aid, &pub_key, &encrypted, &nonce, &key)
                                .map_err(|e| {
                                    rusqlite::Error::ToSqlConversionFailure(Box::new(
                                        std::io::Error::other(e),
                                    ))
                                })?;
                        Ok(Some(kp))
                    }
                }
            })
            .await
            .map_err(|e| anyhow::anyhow!("{}", e))
    }
}
