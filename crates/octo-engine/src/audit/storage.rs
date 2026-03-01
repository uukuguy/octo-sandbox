use sqlx::SqlitePool;
use chrono::Utc;

pub struct AuditStorage {
    pool: SqlitePool,
}

pub struct AuditEvent {
    pub event_type: String,
    pub user_id: Option<String>,
    pub session_id: Option<String>,
    pub resource_id: Option<String>,
    pub action: String,
    pub result: String,
    pub metadata: Option<serde_json::Value>,
    pub ip_address: Option<String>,
}

impl AuditStorage {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn log(&self, event: AuditEvent) -> Result<i64, sqlx::Error> {
        let result = sqlx::query(
            "INSERT INTO audit_logs (timestamp, event_type, user_id,
             session_id, resource_id, action, result, metadata, ip_address)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)"
        )
        .bind(Utc::now().to_rfc3339())
        .bind(&event.event_type)
        .bind(&event.user_id)
        .bind(&event.session_id)
        .bind(&event.resource_id)
        .bind(&event.action)
        .bind(&event.result)
        .bind(event.metadata.as_ref().map(|m| m.to_string()))
        .bind(&event.ip_address)
        .execute(&self.pool)
        .await?;

        Ok(result.last_insert_rowid())
    }

    pub async fn query(
        &self,
        event_type: Option<&str>,
        user_id: Option<&str>,
        limit: u32,
        offset: u32,
    ) -> Result<Vec<AuditRecord>, sqlx::Error> {
        let mut sql = "SELECT * FROM audit_logs WHERE 1=1".to_string();

        if event_type.is_some() {
            sql.push_str(" AND event_type = ?");
        }
        if user_id.is_some() {
            sql.push_str(" AND user_id = ?");
        }

        sql.push_str(" ORDER BY timestamp DESC LIMIT ? OFFSET ?");

        let mut query = sqlx::query_as::<_, AuditRecord>(&sql);

        if let Some(t) = event_type {
            query = query.bind(t);
        }
        if let Some(u) = user_id {
            query = query.bind(u);
        }
        query = query.bind(limit as i64).bind(offset as i64);

        let rows = query.fetch_all(&self.pool).await?;

        Ok(rows)
    }
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct AuditRecord {
    pub id: i64,
    pub timestamp: String,
    pub event_type: String,
    pub user_id: Option<String>,
    pub session_id: Option<String>,
    pub resource_id: Option<String>,
    pub action: String,
    pub result: String,
    pub metadata: Option<String>,
    pub ip_address: Option<String>,
}
