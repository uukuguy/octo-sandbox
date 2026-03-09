use anyhow::Result;
use async_trait::async_trait;
use octo_types::{
    MemoryCategory, MemoryEntry, MemoryFilter, MemoryId, MemoryResult, MemorySource,
    MemoryTimestamps, SearchOptions,
};
use tracing::debug;

use super::store_traits::MemoryStore;

pub struct SqliteMemoryStore {
    conn: tokio_rusqlite::Connection,
}

impl SqliteMemoryStore {
    pub fn new(conn: tokio_rusqlite::Connection) -> Self {
        Self { conn }
    }
}

#[async_trait]
impl MemoryStore for SqliteMemoryStore {
    async fn store(&self, entry: MemoryEntry) -> Result<MemoryId> {
        let id = entry.id.clone();
        let embedding_blob: Option<Vec<u8>> = entry
            .embedding
            .as_ref()
            .map(bincode::serialize)
            .transpose()?;

        self.conn
            .call(move |conn| {
                conn.execute(
                    "INSERT INTO memories (id, user_id, sandbox_id, category, content, metadata, embedding, importance, access_count, accessed_at, source_type, source_ref, ttl, created_at, updated_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
                    rusqlite::params![
                        entry.id.as_str(),
                        entry.user_id,
                        entry.sandbox_id,
                        entry.category.as_str(),
                        entry.content,
                        entry.metadata.to_string(),
                        embedding_blob,
                        entry.importance,
                        entry.access_count,
                        entry.timestamps.accessed_at,
                        entry.source_type.as_str(),
                        entry.source_ref,
                        entry.ttl,
                        entry.timestamps.created_at,
                        entry.timestamps.updated_at,
                    ],
                )?;
                Ok(())
            })
            .await?;

        debug!(id = %id, "Stored memory");
        Ok(id)
    }

    async fn get(&self, id: &MemoryId) -> Result<Option<MemoryEntry>> {
        let id_str = id.as_str().to_string();
        let result = self
            .conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT id, user_id, sandbox_id, category, content, metadata, embedding, importance, access_count, accessed_at, source_type, source_ref, ttl, created_at, updated_at
                     FROM memories WHERE id = ?1",
                )?;
                let entry = stmt
                    .query_row(rusqlite::params![id_str], |row| {
                        row_to_entry(row)
                    })
                    .ok();
                Ok(entry)
            })
            .await?;
        Ok(result)
    }

    async fn update(&self, id: &MemoryId, content: &str) -> Result<()> {
        let id_str = id.as_str().to_string();
        let content_str = content.to_string();
        let now = chrono::Utc::now().timestamp();

        self.conn
            .call(move |conn| {
                conn.execute(
                    "UPDATE memories SET content = ?1, updated_at = ?2 WHERE id = ?3",
                    rusqlite::params![content_str, now, id_str],
                )?;
                Ok(())
            })
            .await?;

        debug!(id = %id, "Updated memory content");
        Ok(())
    }

    async fn delete(&self, id: &MemoryId) -> Result<()> {
        let id_str = id.as_str().to_string();

        self.conn
            .call(move |conn| {
                conn.execute(
                    "DELETE FROM memories WHERE id = ?1",
                    rusqlite::params![id_str],
                )?;
                Ok(())
            })
            .await?;

        debug!(id = %id, "Deleted memory");
        Ok(())
    }

    async fn delete_by_filter(&self, filter: MemoryFilter) -> Result<usize> {
        let result = self
            .conn
            .call(move |conn| {
                let mut sql = String::from("DELETE FROM memories WHERE user_id = ?");
                let mut params: Vec<Box<dyn rusqlite::types::ToSql>> =
                    vec![Box::new(filter.user_id.clone())];
                let mut param_idx = 2;

                if let Some(ref sid) = filter.sandbox_id {
                    sql.push_str(&format!(" AND sandbox_id = ?{param_idx}"));
                    params.push(Box::new(sid.clone()));
                    param_idx += 1;
                }

                if let Some(ref cats) = filter.categories {
                    if !cats.is_empty() {
                        let placeholders: Vec<String> = cats
                            .iter()
                            .enumerate()
                            .map(|(i, _)| format!("?{}", param_idx + i))
                            .collect();
                        sql.push_str(&format!(" AND category IN ({})", placeholders.join(",")));
                        for cat in cats {
                            params.push(Box::new(cat.as_str().to_string()));
                        }
                        param_idx += cats.len();
                    }
                }

                if let Some(ref sources) = filter.source_types {
                    if !sources.is_empty() {
                        let placeholders: Vec<String> = sources
                            .iter()
                            .enumerate()
                            .map(|(i, _)| format!("?{}", param_idx + i))
                            .collect();
                        sql.push_str(&format!(" AND source_type IN ({})", placeholders.join(",")));
                        for src in sources {
                            params.push(Box::new(src.as_str().to_string()));
                        }
                    }
                }

                let params_ref: Vec<&dyn rusqlite::types::ToSql> =
                    params.iter().map(|p| p.as_ref()).collect();

                let deleted = conn.execute(&sql, params_ref.as_slice())?;
                Ok(deleted)
            })
            .await?;

        debug!(count = result, "Deleted memories by filter");
        Ok(result)
    }

    async fn list(&self, filter: MemoryFilter) -> Result<Vec<MemoryEntry>> {
        let result = self
            .conn
            .call(move |conn| {
                let mut sql = String::from(
                    "SELECT id, user_id, sandbox_id, category, content, metadata, embedding, importance, access_count, accessed_at, source_type, source_ref, ttl, created_at, updated_at
                     FROM memories WHERE user_id = ?",
                );
                let mut params: Vec<Box<dyn rusqlite::types::ToSql>> =
                    vec![Box::new(filter.user_id.clone())];
                let mut param_idx = 2;

                if let Some(ref sid) = filter.sandbox_id {
                    sql.push_str(&format!(" AND sandbox_id = ?{param_idx}"));
                    params.push(Box::new(sid.clone()));
                    param_idx += 1;
                }

                if let Some(ref cats) = filter.categories {
                    if !cats.is_empty() {
                        let placeholders: Vec<String> = cats
                            .iter()
                            .enumerate()
                            .map(|(i, _)| format!("?{}", param_idx + i))
                            .collect();
                        sql.push_str(&format!(" AND category IN ({})", placeholders.join(",")));
                        for cat in cats {
                            params.push(Box::new(cat.as_str().to_string()));
                        }
                        param_idx += cats.len();
                    }
                }

                if let Some(ref sources) = filter.source_types {
                    if !sources.is_empty() {
                        let placeholders: Vec<String> = sources
                            .iter()
                            .enumerate()
                            .map(|(i, _)| format!("?{}", param_idx + i))
                            .collect();
                        sql.push_str(&format!(" AND source_type IN ({})", placeholders.join(",")));
                        for src in sources {
                            params.push(Box::new(src.as_str().to_string()));
                        }
                    }
                }

                sql.push_str(&format!(" ORDER BY updated_at DESC LIMIT {}", filter.limit));

                let params_ref: Vec<&dyn rusqlite::types::ToSql> =
                    params.iter().map(|p| p.as_ref()).collect();

                let mut stmt = conn.prepare(&sql)?;
                let rows = stmt.query_map(params_ref.as_slice(), row_to_entry)?;

                let mut entries = Vec::new();
                for row in rows {
                    entries.push(row?);
                }
                Ok(entries)
            })
            .await?;
        Ok(result)
    }

    async fn batch_store(&self, entries: Vec<MemoryEntry>) -> Result<Vec<MemoryId>> {
        let ids: Vec<MemoryId> = entries.iter().map(|e| e.id.clone()).collect();

        let entries_data: Vec<(
            String,
            String,
            String,
            String,
            String,
            String,
            Option<Vec<u8>>,
            f32,
            u32,
            i64,
            String,
            String,
            Option<i64>,
            i64,
            i64,
        )> = entries
            .into_iter()
            .map(|e| {
                let blob = e
                    .embedding
                    .as_ref()
                    .and_then(|emb| bincode::serialize(emb).ok());
                (
                    e.id.as_str().to_string(),
                    e.user_id,
                    e.sandbox_id,
                    e.category.as_str().to_string(),
                    e.content,
                    e.metadata.to_string(),
                    blob,
                    e.importance,
                    e.access_count,
                    e.timestamps.accessed_at,
                    e.source_type.as_str().to_string(),
                    e.source_ref,
                    e.ttl,
                    e.timestamps.created_at,
                    e.timestamps.updated_at,
                )
            })
            .collect();

        self.conn
            .call(move |conn| {
                let tx = conn.transaction()?;
                for row in &entries_data {
                    tx.execute(
                        "INSERT INTO memories (id, user_id, sandbox_id, category, content, metadata, embedding, importance, access_count, accessed_at, source_type, source_ref, ttl, created_at, updated_at)
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
                        rusqlite::params![
                            row.0, row.1, row.2, row.3, row.4, row.5, row.6,
                            row.7, row.8, row.9, row.10, row.11, row.12, row.13, row.14,
                        ],
                    )?;
                }
                tx.commit()?;
                Ok(())
            })
            .await?;

        debug!(count = ids.len(), "Batch stored memories");
        Ok(ids)
    }

    async fn search(&self, query: &str, opts: SearchOptions) -> Result<Vec<MemoryResult>> {
        let query_str = query.to_string();
        let has_embedding = opts.query_embedding.is_some();
        let query_embedding = opts.query_embedding.clone();
        let time_decay_enabled = opts.time_decay;
        let min_score = opts.min_score;
        let token_budget = opts.token_budget;
        let limit = opts.limit;
        let user_id = opts.user_id.clone();

        let results = self
            .conn
            .call(move |conn| {
                // Step 1: FTS5 search
                let fts_results = fts_search(conn, &query_str, &user_id, limit)?;

                // Step 2: Vector search (if embedding provided)
                let vec_results = if has_embedding {
                    vector_search(
                        conn,
                        query_embedding.as_ref().unwrap(),
                        &user_id,
                        limit,
                    )?
                } else {
                    Vec::new()
                };

                // Step 3: Score fusion
                let mut scored: std::collections::HashMap<String, (MemoryEntry, f32, String)> =
                    std::collections::HashMap::new();

                let fts_max = fts_results
                    .iter()
                    .map(|(_, s)| *s)
                    .fold(f32::NEG_INFINITY, f32::max)
                    .max(1.0);
                let vec_max = vec_results
                    .iter()
                    .map(|(_, s)| *s)
                    .fold(f32::NEG_INFINITY, f32::max)
                    .max(1.0);

                for (entry, raw_score) in &fts_results {
                    let norm = raw_score / fts_max;
                    let weight = if has_embedding { 0.3 } else { 1.0 };
                    scored
                        .entry(entry.id.as_str().to_string())
                        .and_modify(|(_, s, _)| *s += weight * norm)
                        .or_insert_with(|| (entry.clone(), weight * norm, "fts".to_string()));
                }

                for (entry, raw_score) in &vec_results {
                    let norm = raw_score / vec_max;
                    scored
                        .entry(entry.id.as_str().to_string())
                        .and_modify(|(_, s, src)| {
                            *s += 0.7 * norm;
                            *src = "hybrid".to_string();
                        })
                        .or_insert_with(|| (entry.clone(), 0.7 * norm, "vector".to_string()));
                }

                // Step 4: Time decay + importance weighting
                let now = chrono::Utc::now().timestamp();
                let mut results: Vec<MemoryResult> = scored
                    .into_values()
                    .map(|(entry, mut score, source)| {
                        if time_decay_enabled {
                            score *= time_decay(entry.timestamps.accessed_at, now);
                        }
                        score *= entry.importance;
                        MemoryResult {
                            entry,
                            score,
                            match_source: source,
                        }
                    })
                    .collect();

                // Step 5: Sort + filter
                results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));

                if let Some(min) = min_score {
                    results.retain(|r| r.score >= min);
                }

                // Step 6: Token budget truncation
                let mut budget_used = 0usize;
                results.retain(|r| {
                    let cost = r.entry.content.len() / 4; // rough token estimate
                    if budget_used + cost > token_budget {
                        return false;
                    }
                    budget_used += cost;
                    true
                });

                results.truncate(limit);

                // Step 7: Update accessed_at + access_count
                for r in &results {
                    let _ = conn.execute(
                        "UPDATE memories SET accessed_at = ?1, access_count = access_count + 1 WHERE id = ?2",
                        rusqlite::params![now, r.entry.id.as_str()],
                    );
                }

                Ok(results)
            })
            .await?;

        debug!(count = results.len(), "Memory search complete");
        Ok(results)
    }
}

/// FTS5 full-text search, returns (entry, bm25_score).
fn fts_search(
    conn: &rusqlite::Connection,
    query: &str,
    user_id: &str,
    limit: usize,
) -> rusqlite::Result<Vec<(MemoryEntry, f32)>> {
    // Build FTS match query: simple tokenization for FTS5
    let fts_query = query.split_whitespace().collect::<Vec<_>>().join(" OR ");

    if fts_query.is_empty() {
        return Ok(Vec::new());
    }

    let mut stmt = conn.prepare(
        "SELECT m.id, m.user_id, m.sandbox_id, m.category, m.content, m.metadata, m.embedding,
                m.importance, m.access_count, m.accessed_at, m.source_type, m.source_ref,
                m.ttl, m.created_at, m.updated_at,
                -rank as score
         FROM memories_fts fts
         JOIN memories m ON m.rowid = fts.rowid
         WHERE memories_fts MATCH ?1 AND m.user_id = ?2
         ORDER BY rank
         LIMIT ?3",
    )?;

    let rows = stmt.query_map(rusqlite::params![fts_query, user_id, limit as i64], |row| {
        let entry = row_to_entry(row)?;
        let score: f32 = row.get(15)?;
        Ok((entry, score))
    })?;

    let mut results = Vec::new();
    for r in rows.flatten() {
        results.push(r);
    }
    Ok(results)
}

/// Brute-force vector search: load embeddings, compute cosine similarity.
fn vector_search(
    conn: &rusqlite::Connection,
    query_embedding: &[f32],
    user_id: &str,
    limit: usize,
) -> rusqlite::Result<Vec<(MemoryEntry, f32)>> {
    let mut stmt = conn.prepare(
        "SELECT id, user_id, sandbox_id, category, content, metadata, embedding,
                importance, access_count, accessed_at, source_type, source_ref,
                ttl, created_at, updated_at
         FROM memories
         WHERE user_id = ?1 AND embedding IS NOT NULL",
    )?;

    let rows = stmt.query_map(rusqlite::params![user_id], row_to_entry)?;

    let mut scored: Vec<(MemoryEntry, f32)> = Vec::new();
    for entry in rows.flatten() {
        if let Some(ref emb) = entry.embedding {
            let sim = cosine_similarity(query_embedding, emb);
            if sim > 0.0 {
                scored.push((entry, sim));
            }
        }
    }

    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    scored.truncate(limit);
    Ok(scored)
}

fn row_to_entry(row: &rusqlite::Row<'_>) -> rusqlite::Result<MemoryEntry> {
    let id: String = row.get(0)?;
    let user_id: String = row.get(1)?;
    let sandbox_id: String = row.get(2)?;
    let category_str: String = row.get(3)?;
    let content: String = row.get(4)?;
    let metadata_str: String = row.get(5)?;
    let embedding_blob: Option<Vec<u8>> = row.get(6)?;
    let importance: f32 = row.get(7)?;
    let access_count: u32 = row.get(8)?;
    let accessed_at: i64 = row.get(9)?;
    let source_type_str: String = row.get(10)?;
    let source_ref: String = row.get(11)?;
    let ttl: Option<i64> = row.get(12)?;
    let created_at: i64 = row.get(13)?;
    let updated_at: i64 = row.get(14)?;

    let category = MemoryCategory::parse(&category_str).unwrap_or(MemoryCategory::Profile);
    let source_type = MemorySource::parse(&source_type_str);
    let embedding: Option<Vec<f32>> =
        embedding_blob.and_then(|blob| bincode::deserialize(&blob).ok());
    let metadata: serde_json::Value =
        serde_json::from_str(&metadata_str).unwrap_or(serde_json::json!({}));

    Ok(MemoryEntry {
        id: MemoryId::from_string(id),
        user_id,
        sandbox_id,
        category,
        content,
        metadata,
        embedding,
        importance,
        access_count,
        source_type,
        source_ref,
        ttl,
        timestamps: MemoryTimestamps {
            created_at,
            updated_at,
            accessed_at,
        },
    })
}

fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let mut dot = 0.0f32;
    let mut norm_a = 0.0f32;
    let mut norm_b = 0.0f32;
    for (x, y) in a.iter().zip(b.iter()) {
        dot += x * y;
        norm_a += x * x;
        norm_b += y * y;
    }
    let denom = norm_a.sqrt() * norm_b.sqrt();
    if denom == 0.0 {
        0.0
    } else {
        dot / denom
    }
}

fn time_decay(accessed_at: i64, now: i64) -> f32 {
    let days = ((now - accessed_at) as f32) / 86400.0;
    (-0.05 * days.max(0.0)).exp()
}
