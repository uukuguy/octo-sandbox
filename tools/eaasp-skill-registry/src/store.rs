use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use tokio_rusqlite::Connection;

use crate::models::{SkillContent, SkillMeta, SkillStatus, SkillVersion, SubmitDraftRequest};

/// SQLite + filesystem store for skill assets.
pub struct SkillStore {
    db: Connection,
    base_dir: PathBuf,
}

impl SkillStore {
    /// Open (or create) the skill store at `base_dir`.
    /// Creates `registry.db` and `skills/` directory.
    pub async fn open(base_dir: &Path) -> Result<Self> {
        std::fs::create_dir_all(base_dir.join("skills")).context("create skills directory")?;

        let db_path = base_dir.join("registry.db");
        let db = Connection::open(db_path)
            .await
            .context("open SQLite database")?;

        db.call(|conn| {
            conn.execute_batch(
                "CREATE TABLE IF NOT EXISTS skills (
                    id          TEXT NOT NULL,
                    version     TEXT NOT NULL,
                    name        TEXT NOT NULL,
                    description TEXT NOT NULL DEFAULT '',
                    status      TEXT NOT NULL DEFAULT 'draft',
                    author      TEXT,
                    tags        TEXT NOT NULL DEFAULT '[]',
                    created_at  TEXT NOT NULL,
                    updated_at  TEXT NOT NULL,
                    git_commit  TEXT,
                    PRIMARY KEY (id, version)
                );",
            )?;
            Ok(())
        })
        .await
        .context("create skills table")?;

        Ok(Self {
            db,
            base_dir: base_dir.to_path_buf(),
        })
    }

    /// Submit a new skill draft. Writes SKILL.md to the filesystem and
    /// inserts/replaces metadata into SQLite.
    pub async fn submit_draft(&self, req: SubmitDraftRequest) -> Result<SkillMeta> {
        let skill_dir = self
            .base_dir
            .join("skills")
            .join(&req.id)
            .join(&req.version);
        std::fs::create_dir_all(&skill_dir).context("create skill version directory")?;

        // Build SKILL.md content with frontmatter.
        // Ensure frontmatter_yaml ends with newline so the closing --- is on its own line.
        let yaml = if req.frontmatter_yaml.ends_with('\n') {
            req.frontmatter_yaml.clone()
        } else {
            format!("{}\n", req.frontmatter_yaml)
        };
        let skill_md = format!("---\n{yaml}---\n\n{}", req.prose);
        std::fs::write(skill_dir.join("SKILL.md"), &skill_md).context("write SKILL.md")?;

        let now = chrono::Utc::now().to_rfc3339();
        let tags_json = serde_json::to_string(&req.tags.unwrap_or_default())?;

        let meta = SkillMeta {
            id: req.id.clone(),
            name: req.name.clone(),
            description: req.description.clone(),
            version: req.version.clone(),
            status: SkillStatus::Draft,
            author: req.author.clone(),
            tags: serde_json::from_str(&tags_json)?,
            created_at: now.clone(),
            updated_at: now.clone(),
        };

        let m = meta.clone();
        let tags_json_clone = tags_json.clone();
        self.db
            .call(move |conn| {
                conn.execute(
                    "INSERT OR REPLACE INTO skills
                        (id, version, name, description, status, author, tags, created_at, updated_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                    rusqlite::params![
                        m.id,
                        m.version,
                        m.name,
                        m.description,
                        m.status.to_string(),
                        m.author,
                        tags_json_clone,
                        m.created_at,
                        m.updated_at,
                    ],
                )?;
                Ok(())
            })
            .await
            .context("insert skill into database")?;

        Ok(meta)
    }

    /// Read a skill by ID and optional version. If version is None, returns the latest.
    pub async fn read_skill(
        &self,
        id: String,
        version: Option<String>,
    ) -> Result<Option<SkillContent>> {
        let id_clone = id.clone();
        let version_clone = version.clone();
        let meta = self
            .db
            .call(move |conn| {
                let (sql, params): (String, Vec<Box<dyn rusqlite::types::ToSql>>) =
                    if let Some(ref v) = version_clone {
                        (
                            "SELECT id, version, name, description, status, author, tags, created_at, updated_at
                             FROM skills WHERE id = ?1 AND version = ?2"
                                .to_string(),
                            vec![Box::new(id_clone.clone()), Box::new(v.clone())],
                        )
                    } else {
                        (
                            "SELECT id, version, name, description, status, author, tags, created_at, updated_at
                             FROM skills WHERE id = ?1 ORDER BY created_at DESC LIMIT 1"
                                .to_string(),
                            vec![Box::new(id_clone.clone())],
                        )
                    };

                let params_refs: Vec<&dyn rusqlite::types::ToSql> =
                    params.iter().map(|p| p.as_ref()).collect();

                let mut stmt = conn.prepare(&sql)?;
                let mut rows = stmt.query(params_refs.as_slice())?;

                if let Some(row) = rows.next()? {
                    Ok(Some(row_to_meta(row)?))
                } else {
                    Ok(None)
                }
            })
            .await
            .context("query skill from database")?;

        let Some(meta) = meta else {
            return Ok(None);
        };

        // Read SKILL.md from filesystem
        let skill_path = self
            .base_dir
            .join("skills")
            .join(&meta.id)
            .join(&meta.version)
            .join("SKILL.md");

        if !skill_path.exists() {
            return Ok(None);
        }

        let content = std::fs::read_to_string(&skill_path).context("read SKILL.md")?;

        let (frontmatter_yaml, prose) = parse_skill_md(&content);
        let parsed_v2 = crate::skill_parser::parse_v2_frontmatter(&frontmatter_yaml).ok();

        Ok(Some(SkillContent {
            meta,
            frontmatter_yaml,
            prose,
            parsed_v2,
        }))
    }

    /// Search skills by optional tag, text query, status, scope, and limit.
    ///
    /// `scope` filters by the `access_scope` field parsed from a skill's v2
    /// frontmatter. Because that field lives on the filesystem (not SQL),
    /// scope filtering is applied as a post-query in-memory filter.
    pub async fn search(
        &self,
        tag: Option<String>,
        query: Option<String>,
        status: Option<String>,
        scope: Option<String>,
        limit: Option<usize>,
    ) -> Result<Vec<SkillMeta>> {
        let base: Vec<SkillMeta> = self.db
            .call(move |conn| {
                let mut sql = "SELECT id, version, name, description, status, author, tags, created_at, updated_at FROM skills WHERE 1=1".to_string();
                let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
                let mut idx = 1;

                if let Some(ref t) = tag {
                    sql.push_str(&format!(" AND tags LIKE ?{idx}"));
                    params.push(Box::new(format!("%\"{t}\"%")));
                    idx += 1;
                }

                if let Some(ref q) = query {
                    sql.push_str(&format!(
                        " AND (name LIKE ?{idx} OR description LIKE ?{})",
                        idx + 1
                    ));
                    let pattern = format!("%{q}%");
                    params.push(Box::new(pattern.clone()));
                    params.push(Box::new(pattern));
                    idx += 2;
                }

                if let Some(ref s) = status {
                    sql.push_str(&format!(" AND status = ?{idx}"));
                    params.push(Box::new(s.clone()));
                    #[allow(unused_assignments)]
                    {
                        idx += 1;
                    }
                }

                let lim = limit.unwrap_or(100);
                sql.push_str(&format!(" ORDER BY updated_at DESC LIMIT {lim}"));

                let params_refs: Vec<&dyn rusqlite::types::ToSql> =
                    params.iter().map(|p| p.as_ref()).collect();

                let mut stmt = conn.prepare(&sql)?;
                let mut rows = stmt.query(params_refs.as_slice())?;

                let mut results = Vec::new();
                while let Some(row) = rows.next()? {
                    results.push(row_to_meta(row)?);
                }
                Ok(results)
            })
            .await
            .context("search skills")?;

        // Post-query scope filter: for each candidate, read its full content
        // and compare `parsed_v2.access_scope`. O(N) — acceptable for MVP.
        let Some(scope_value) = scope else {
            return Ok(base);
        };

        let mut filtered = Vec::with_capacity(base.len());
        for meta in base {
            match self
                .read_skill(meta.id.clone(), Some(meta.version.clone()))
                .await?
            {
                Some(content) => {
                    let matches = content
                        .parsed_v2
                        .as_ref()
                        .and_then(|v| v.access_scope.as_ref())
                        .map(|s| s == &scope_value)
                        .unwrap_or(false);
                    if matches {
                        filtered.push(meta);
                    }
                }
                None => continue,
            }
        }
        Ok(filtered)
    }

    /// Promote a skill version to a new status.
    pub async fn promote(
        &self,
        id: String,
        version: String,
        target_status: SkillStatus,
    ) -> Result<()> {
        let now = chrono::Utc::now().to_rfc3339();
        let status_str = target_status.to_string();

        self.db
            .call(move |conn| {
                let changed = conn.execute(
                    "UPDATE skills SET status = ?1, updated_at = ?2 WHERE id = ?3 AND version = ?4",
                    rusqlite::params![status_str, now, id, version],
                )?;
                if changed == 0 {
                    return Err(tokio_rusqlite::Error::Rusqlite(
                        rusqlite::Error::QueryReturnedNoRows,
                    ));
                }
                Ok(())
            })
            .await
            .context("promote skill status")
    }

    /// List all versions of a skill.
    pub async fn list_versions(&self, id: String) -> Result<Vec<SkillVersion>> {
        self.db
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT version, status, created_at, git_commit
                     FROM skills WHERE id = ?1 ORDER BY created_at DESC",
                )?;
                let mut rows = stmt.query(rusqlite::params![id])?;
                let mut versions = Vec::new();
                while let Some(row) = rows.next()? {
                    versions.push(SkillVersion {
                        version: row.get(0)?,
                        status: parse_status(&row.get::<_, String>(1)?),
                        created_at: row.get(2)?,
                        git_commit: row.get(3)?,
                    });
                }
                Ok(versions)
            })
            .await
            .context("list skill versions")
    }
}

/// Parse a `SkillMeta` from a SQLite row.
fn row_to_meta(row: &rusqlite::Row<'_>) -> rusqlite::Result<SkillMeta> {
    let tags_str: String = row.get(6)?;
    let tags: Vec<String> = serde_json::from_str(&tags_str).unwrap_or_default();

    Ok(SkillMeta {
        id: row.get(0)?,
        version: row.get(1)?,
        name: row.get(2)?,
        description: row.get(3)?,
        status: parse_status(&row.get::<_, String>(4)?),
        author: row.get(5)?,
        tags,
        created_at: row.get(7)?,
        updated_at: row.get(8)?,
    })
}

/// Parse status string to enum.
fn parse_status(s: &str) -> SkillStatus {
    match s {
        "draft" => SkillStatus::Draft,
        "tested" => SkillStatus::Tested,
        "reviewed" => SkillStatus::Reviewed,
        "production" => SkillStatus::Production,
        _ => SkillStatus::Draft,
    }
}

/// Parse a SKILL.md file into (frontmatter_yaml, prose).
/// If content starts with `---\n`, splits at the closing `\n---\n`.
pub fn parse_skill_md(content: &str) -> (String, String) {
    if content.starts_with("---\n") {
        let rest = &content[4..]; // skip opening "---\n"
        if let Some(end_idx) = rest.find("\n---\n") {
            let frontmatter = rest[..end_idx + 1].to_string(); // include trailing newline
            let prose = rest[end_idx + 5..].trim_start().to_string(); // skip "\n---\n"
            return (frontmatter, prose);
        }
    }
    // No frontmatter detected
    (String::new(), content.to_string())
}
