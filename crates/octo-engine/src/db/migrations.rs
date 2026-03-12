use rusqlite::Connection;

/// Migration struct for database migrations
pub struct Migration {
    pub version: u32,
    pub name: &'static str,
    pub sql: &'static str,
}

impl Migration {
    pub const fn new(version: u32, name: &'static str, sql: &'static str) -> Self {
        Self { version, name, sql }
    }

    pub fn execute(&self, conn: &Connection) -> rusqlite::Result<()> {
        conn.execute_batch(self.sql)?;
        Ok(())
    }
}

/// Migration v1: Initial schema (memory, sessions, memories)
pub fn migration_v1() -> Migration {
    Migration::new(
        1,
        "initial_schema",
        r#"
        -- Working Memory blocks persistence
        CREATE TABLE IF NOT EXISTS memory_blocks (
            id          TEXT NOT NULL,
            user_id     TEXT NOT NULL,
            sandbox_id  TEXT NOT NULL,
            label       TEXT NOT NULL,
            value       TEXT NOT NULL DEFAULT '',
            priority    INTEGER NOT NULL DEFAULT 128,
            max_age_turns INTEGER,
            last_updated_turn INTEGER NOT NULL DEFAULT 0,
            char_limit  INTEGER NOT NULL DEFAULT 2000,
            is_readonly INTEGER NOT NULL DEFAULT 0,
            updated_at  INTEGER NOT NULL DEFAULT (strftime('%s','now')),
            PRIMARY KEY (id, user_id, sandbox_id)
        );

        -- Session metadata
        CREATE TABLE IF NOT EXISTS sessions (
            session_id  TEXT PRIMARY KEY,
            user_id     TEXT NOT NULL,
            sandbox_id  TEXT NOT NULL,
            created_at  INTEGER NOT NULL DEFAULT (strftime('%s','now'))
        );

        -- Session messages
        CREATE TABLE IF NOT EXISTS session_messages (
            id          INTEGER PRIMARY KEY AUTOINCREMENT,
            session_id  TEXT NOT NULL,
            role        TEXT NOT NULL,
            content_json TEXT NOT NULL,
            created_at  INTEGER NOT NULL DEFAULT (strftime('%s','now'))
        );

        CREATE INDEX IF NOT EXISTS idx_session_messages_session_id
            ON session_messages(session_id);

        -- Persistent Memory (Layer 2)
        CREATE TABLE IF NOT EXISTS memories (
            id          TEXT PRIMARY KEY,
            user_id     TEXT NOT NULL,
            sandbox_id  TEXT NOT NULL DEFAULT '',
            category    TEXT NOT NULL,
            content     TEXT NOT NULL,
            metadata    TEXT NOT NULL DEFAULT '{}',
            embedding   BLOB,
            importance  REAL NOT NULL DEFAULT 0.5,
            access_count INTEGER NOT NULL DEFAULT 0,
            accessed_at INTEGER NOT NULL DEFAULT (strftime('%s','now')),
            source_type TEXT NOT NULL DEFAULT 'manual',
            source_ref  TEXT NOT NULL DEFAULT '',
            ttl         INTEGER,
            created_at  INTEGER NOT NULL DEFAULT (strftime('%s','now')),
            updated_at  INTEGER NOT NULL DEFAULT (strftime('%s','now'))
        );

        CREATE INDEX IF NOT EXISTS idx_memories_user_id ON memories(user_id);
        CREATE INDEX IF NOT EXISTS idx_memories_category ON memories(category);
        CREATE INDEX IF NOT EXISTS idx_memories_created_at ON memories(created_at);

        -- FTS5 virtual table for full-text search
        CREATE VIRTUAL TABLE IF NOT EXISTS memories_fts USING fts5(
            content,
            category,
            content=memories,
            content_rowid=rowid,
            tokenize='porter unicode61'
        );

        -- FTS5 sync triggers
        CREATE TRIGGER IF NOT EXISTS memories_ai AFTER INSERT ON memories BEGIN
            INSERT INTO memories_fts(rowid, content, category)
            VALUES (NEW.rowid, NEW.content, NEW.category);
        END;

        CREATE TRIGGER IF NOT EXISTS memories_ad AFTER DELETE ON memories BEGIN
            INSERT INTO memories_fts(memories_fts, rowid, content, category)
            VALUES ('delete', OLD.rowid, OLD.content, OLD.category);
        END;

        CREATE TRIGGER IF NOT EXISTS memories_au AFTER UPDATE ON memories BEGIN
            INSERT INTO memories_fts(memories_fts, rowid, content, category)
            VALUES ('delete', OLD.rowid, OLD.content, OLD.category);
            INSERT INTO memories_fts(rowid, content, category)
            VALUES (NEW.rowid, NEW.content, NEW.category);
        END;
        "#,
    )
}

/// Migration v2: Tool execution records
pub fn migration_v2() -> Migration {
    Migration::new(
        2,
        "tool_executions",
        r#"
        -- Tool execution records
        CREATE TABLE IF NOT EXISTS tool_executions (
            id          TEXT PRIMARY KEY,
            session_id  TEXT NOT NULL,
            tool_name   TEXT NOT NULL,
            source      TEXT NOT NULL,
            input       TEXT NOT NULL,
            output      TEXT,
            status      TEXT NOT NULL DEFAULT 'running',
            started_at  INTEGER NOT NULL,
            duration_ms INTEGER,
            error       TEXT,
            created_at  TEXT NOT NULL DEFAULT (datetime('now'))
        );

        CREATE INDEX IF NOT EXISTS idx_tool_executions_session
            ON tool_executions(session_id);
        CREATE INDEX IF NOT EXISTS idx_tool_executions_tool
            ON tool_executions(tool_name);
        CREATE INDEX IF NOT EXISTS idx_tool_executions_started
            ON tool_executions(started_at DESC);
        "#,
    )
}

/// Migration v3: MCP Server configurations
pub fn migration_v3() -> Migration {
    Migration::new(
        3,
        "mcp_servers",
        r#"
        -- MCP Server configurations
        CREATE TABLE IF NOT EXISTS mcp_servers (
            id          TEXT PRIMARY KEY,
            name        TEXT NOT NULL,
            source      TEXT NOT NULL DEFAULT 'manual',
            command     TEXT NOT NULL,
            args        TEXT NOT NULL DEFAULT '[]',
            env         TEXT NOT NULL DEFAULT '{}',
            enabled     INTEGER NOT NULL DEFAULT 1,
            created_at  TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at  TEXT NOT NULL DEFAULT (datetime('now'))
        );

        -- MCP tool execution history
        CREATE TABLE IF NOT EXISTS mcp_executions (
            id          TEXT PRIMARY KEY,
            server_id   TEXT NOT NULL,
            tool_name   TEXT NOT NULL,
            params      TEXT NOT NULL,
            result      TEXT,
            error       TEXT,
            duration_ms INTEGER,
            executed_at TEXT NOT NULL DEFAULT (datetime('now')),
            FOREIGN KEY (server_id) REFERENCES mcp_servers(id)
        );

        CREATE INDEX IF NOT EXISTS idx_mcp_executions_server
            ON mcp_executions(server_id);
        CREATE INDEX IF NOT EXISTS idx_mcp_executions_time
            ON mcp_executions(executed_at DESC);

        -- MCP communication logs
        CREATE TABLE IF NOT EXISTS mcp_logs (
            id          TEXT PRIMARY KEY,
            server_id   TEXT NOT NULL,
            level       TEXT NOT NULL,
            direction   TEXT NOT NULL,
            method      TEXT,
            params      TEXT,
            result      TEXT,
            raw_data    TEXT,
            duration_ms INTEGER,
            logged_at   TEXT NOT NULL DEFAULT (datetime('now')),
            FOREIGN KEY (server_id) REFERENCES mcp_servers(id)
        );

        CREATE INDEX IF NOT EXISTS idx_mcp_logs_server_time
            ON mcp_logs(server_id, logged_at);
        "#,
    )
}

/// Migration v4: Add user_id for isolation
pub fn migration_v4() -> Migration {
    Migration::new(
        4,
        "user_isolation",
        r#"
        -- Add user_id to session_messages for isolation
        ALTER TABLE session_messages ADD COLUMN user_id TEXT NOT NULL DEFAULT 'default';

        -- Add user_id to tool_executions for isolation
        ALTER TABLE tool_executions ADD COLUMN user_id TEXT NOT NULL DEFAULT 'default';

        -- Add user_id to mcp_servers for isolation
        ALTER TABLE mcp_servers ADD COLUMN user_id TEXT NOT NULL DEFAULT 'default';

        -- Add user_id to mcp_executions for isolation
        ALTER TABLE mcp_executions ADD COLUMN user_id TEXT NOT NULL DEFAULT 'default';

        -- Add user_id to mcp_logs for isolation
        ALTER TABLE mcp_logs ADD COLUMN user_id TEXT NOT NULL DEFAULT 'default';

        -- Create indexes for user_id filtering
        CREATE INDEX IF NOT EXISTS idx_session_messages_user_id ON session_messages(user_id);
        CREATE INDEX IF NOT EXISTS idx_tool_executions_user_id ON tool_executions(user_id);
        CREATE INDEX IF NOT EXISTS idx_mcp_servers_user_id ON mcp_servers(user_id);
        CREATE INDEX IF NOT EXISTS idx_mcp_executions_user_id ON mcp_executions(user_id);
        CREATE INDEX IF NOT EXISTS idx_mcp_logs_user_id ON mcp_logs(user_id);
        "#,
    )
}

/// Migration v5: Add scheduled_tasks and task_executions tables
pub fn migration_v5() -> Migration {
    Migration::new(
        5,
        "add_scheduled_tasks",
        r#"
        CREATE TABLE IF NOT EXISTS scheduled_tasks (
            id TEXT PRIMARY KEY,
            user_id TEXT,
            name TEXT NOT NULL,
            cron TEXT NOT NULL,
            agent_config TEXT NOT NULL,
            enabled INTEGER NOT NULL DEFAULT 1,
            last_run TEXT,
            next_run TEXT,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_scheduled_tasks_user_id ON scheduled_tasks(user_id);
        CREATE INDEX IF NOT EXISTS idx_scheduled_tasks_next_run ON scheduled_tasks(next_run);
        CREATE INDEX IF NOT EXISTS idx_scheduled_tasks_enabled ON scheduled_tasks(enabled);

        CREATE TABLE IF NOT EXISTS task_executions (
            id TEXT PRIMARY KEY,
            task_id TEXT NOT NULL,
            started_at TEXT NOT NULL,
            finished_at TEXT,
            status TEXT NOT NULL,
            result TEXT,
            error TEXT
        );

        CREATE INDEX IF NOT EXISTS idx_task_executions_task_id ON task_executions(task_id);
        CREATE INDEX IF NOT EXISTS idx_task_executions_started_at ON task_executions(started_at);
        "#,
    )
}

/// Migration v6: Add audit_logs table
pub fn migration_v6() -> Migration {
    Migration::new(
        6,
        "add_audit_logs",
        r#"
        CREATE TABLE IF NOT EXISTS audit_logs (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            timestamp TEXT NOT NULL DEFAULT (datetime('now')),
            event_type TEXT NOT NULL,
            user_id TEXT,
            session_id TEXT,
            resource_id TEXT,
            action TEXT NOT NULL,
            result TEXT NOT NULL,
            metadata TEXT,
            ip_address TEXT
        );

        CREATE INDEX IF NOT EXISTS idx_audit_event_type ON audit_logs(event_type);
        CREATE INDEX IF NOT EXISTS idx_audit_user_id ON audit_logs(user_id);
        CREATE INDEX IF NOT EXISTS idx_audit_timestamp ON audit_logs(timestamp);
        "#,
    )
}

/// Migration v7: Byzantine consensus persistence tables
pub fn migration_v7() -> Migration {
    Migration::new(
        7,
        "byzantine_consensus_persistence",
        r#"
        -- Byzantine proposals persistence
        CREATE TABLE IF NOT EXISTS byzantine_proposals (
            id TEXT NOT NULL,
            session_id TEXT NOT NULL,
            collaboration_id TEXT NOT NULL,
            proposer TEXT NOT NULL,
            action TEXT NOT NULL,
            description TEXT NOT NULL DEFAULT '',
            phase TEXT NOT NULL DEFAULT 'PrePrepare',
            prepare_votes TEXT NOT NULL DEFAULT '[]',
            commit_votes TEXT NOT NULL DEFAULT '[]',
            total_agents INTEGER NOT NULL,
            created_at TEXT NOT NULL,
            finalized_at TEXT,
            PRIMARY KEY (id, session_id)
        );
        CREATE INDEX IF NOT EXISTS idx_bp_session ON byzantine_proposals(session_id, collaboration_id);
        CREATE INDEX IF NOT EXISTS idx_bp_phase ON byzantine_proposals(session_id, phase);

        -- Consensus view state persistence
        CREATE TABLE IF NOT EXISTS consensus_view_state (
            session_id TEXT NOT NULL,
            collaboration_id TEXT NOT NULL,
            view_number INTEGER NOT NULL DEFAULT 0,
            leader TEXT NOT NULL,
            agents TEXT NOT NULL DEFAULT '[]',
            timeout_ms INTEGER NOT NULL DEFAULT 5000,
            pending_requests TEXT NOT NULL DEFAULT '[]',
            updated_at TEXT NOT NULL,
            PRIMARY KEY (session_id, collaboration_id)
        );

        -- Consensus signatures audit log (immutable)
        CREATE TABLE IF NOT EXISTS consensus_signatures (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            session_id TEXT NOT NULL,
            proposal_id TEXT NOT NULL,
            agent_id TEXT NOT NULL,
            phase TEXT NOT NULL,
            approve INTEGER NOT NULL,
            signature BLOB NOT NULL,
            public_key BLOB NOT NULL,
            payload TEXT NOT NULL,
            created_at TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_cs_proposal ON consensus_signatures(session_id, proposal_id);

        -- Consensus keypairs (AES-GCM encrypted private keys)
        CREATE TABLE IF NOT EXISTS consensus_keypairs (
            agent_id TEXT NOT NULL,
            session_id TEXT NOT NULL,
            public_key BLOB NOT NULL,
            private_key_encrypted BLOB NOT NULL,
            encryption_nonce BLOB NOT NULL,
            created_at TEXT NOT NULL,
            PRIMARY KEY (agent_id, session_id)
        );
        "#,
    )
}

/// Migration v8: Sync infrastructure tables
pub fn migration_v8() -> Migration {
    Migration::new(
        8,
        "sync_infrastructure",
        r#"
        -- Sync metadata: device sync state
        CREATE TABLE IF NOT EXISTS sync_metadata (
            device_id TEXT PRIMARY KEY,
            last_sync_at TEXT,
            sync_version INTEGER NOT NULL DEFAULT 0,
            server_url TEXT,
            created_at TEXT NOT NULL DEFAULT (datetime('now'))
        );

        -- Sync changelog: change tracking with HLC columns
        CREATE TABLE IF NOT EXISTS sync_changelog (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            table_name TEXT NOT NULL,
            row_id TEXT NOT NULL,
            operation TEXT NOT NULL,
            data TEXT NOT NULL DEFAULT '{}',
            hlc_physical_ms INTEGER NOT NULL DEFAULT 0,
            hlc_logical INTEGER NOT NULL DEFAULT 0,
            node_id TEXT NOT NULL DEFAULT '',
            device_id TEXT NOT NULL DEFAULT '',
            sync_version INTEGER NOT NULL DEFAULT 0,
            synced INTEGER NOT NULL DEFAULT 0,
            created_at TEXT NOT NULL DEFAULT (datetime('now'))
        );

        CREATE INDEX IF NOT EXISTS idx_cl_unsynced ON sync_changelog(synced, sync_version);
        CREATE INDEX IF NOT EXISTS idx_cl_table ON sync_changelog(table_name, row_id);
        CREATE INDEX IF NOT EXISTS idx_cl_device ON sync_changelog(device_id);

        -- Auto-assign sync_version on insert via trigger
        CREATE TRIGGER IF NOT EXISTS sync_changelog_version
        AFTER INSERT ON sync_changelog
        WHEN NEW.sync_version = 0
        BEGIN
            UPDATE sync_changelog
            SET sync_version = NEW.id
            WHERE id = NEW.id;
        END;
        "#,
    )
}

/// Migration v9: Session threads and turns for conversation branching/undo
pub fn migration_v9() -> Migration {
    Migration::new(
        9,
        "session_threads_turns",
        r#"
        -- Conversation threads within a session
        CREATE TABLE IF NOT EXISTS threads (
            thread_id TEXT PRIMARY KEY,
            session_id TEXT NOT NULL,
            title TEXT,
            parent_thread_id TEXT,
            created_at INTEGER NOT NULL DEFAULT (strftime('%s','now')),
            FOREIGN KEY (session_id) REFERENCES sessions(session_id)
        );

        CREATE INDEX IF NOT EXISTS idx_threads_session_id ON threads(session_id);

        -- Individual conversation turns within a thread
        CREATE TABLE IF NOT EXISTS turns (
            turn_id TEXT PRIMARY KEY,
            thread_id TEXT NOT NULL,
            user_message_json TEXT NOT NULL,
            assistant_messages_json TEXT NOT NULL,
            created_at INTEGER NOT NULL DEFAULT (strftime('%s','now')),
            FOREIGN KEY (thread_id) REFERENCES threads(thread_id)
        );

        CREATE INDEX IF NOT EXISTS idx_turns_thread_id ON turns(thread_id);
        "#,
    )
}
