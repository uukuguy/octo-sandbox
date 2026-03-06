# P1-3: PlatformState + Per-User AgentRuntime 设计方案

> 日期：2026-03-04
> 状态：设计完成
> 前置：P1-1 (crate + Axum), P1-2 (用户认证)

---

## 一、设计决策

| 决策项 | 选择 | 理由 |
|--------|------|------|
| 数据隔离 | Per-user 目录 `data-platform/users/{user_id}/` | 天然物理隔离，删除即清理，最简单 |
| LLM 配置 | 环境变量 | P1 简单，与 workbench 一致；后续 P2 再扩展 per-user |
| 并发限制 | 可配置，默认 3 | 平衡灵活性与资源保护 |

---

## 二、架构设计

### 2.1 整体架构

```
AppState (Platform 级别)
├── config: PlatformConfig
├── db: UserDatabase (用户账号)
├── jwt: JwtManager
└── users: DashMap<UserId, Arc<UserRuntime>>  ← 新增

UserRuntime (用户级别)
├── user_id: UserId
├── config: UserRuntimeConfig
├── sessions: DashMap<SessionId, Arc<Session>>  ← 用户会话
└── db_path: PathBuf  ← data-platform/users/{user_id}/
```

### 2.2 懒加载策略

- **UserRuntime**：首次请求时创建，长期不活跃（默认 30 分钟）后回收
- **Session/AgentRuntime**：按需创建，会话结束即销毁
- **并发控制**：用户级别限制，默认 max 3 个并发 Agent

---

## 三、数据结构

### 3.1 PlatformConfig 扩展

```rust
#[derive(Debug, Clone)]
pub struct PlatformConfig {
    pub host: String,
    pub port: u16,
    pub data_dir: PathBuf,
    // 新增：用户运行时配置
    pub user_runtime: UserRuntimeConfig,
}

#[derive(Debug, Clone)]
pub struct UserRuntimeConfig {
    pub max_concurrent_agents: u32,  // 默认 3
    pub session_timeout_minutes: u32,  // 默认 30
    pub db_path_template: String,  // "data-platform/users/{user_id}"
}
```

### 3.2 UserRuntime

```rust
pub struct UserRuntime {
    pub user_id: UserId,
    pub config: Arc<UserRuntimeConfig>,
    // 按会话隔离的 AgentRuntime
    pub sessions: DashMap<SessionId, Arc<Session>>,
    // 懒加载：会话首次消息时创建
    pub db_path: PathBuf,
}

impl UserRuntime {
    pub async fn new(user_id: UserId, config: Arc<UserRuntimeConfig>) -> Result<Self> {
        let db_path = config.db_path_template
            .replace("{user_id}", &user_id.to_string());

        // 确保目录存在
        tokio::fs::create_dir_all(&db_path).await?;

        Ok(Self {
            user_id,
            config,
            sessions: DashMap::new(),
            db_path: PathBuf::from(db_path),
        })
    }

    pub async fn get_or_create_session(&self, session_id: SessionId) -> Result<Arc<Session>> {
        // 检查并发限制
        let current = self.sessions.len() as u32;
        if current >= self.config.max_concurrent_agents {
            return Err(Error::ConcurrentLimitExceeded {
                max: self.config.max_concurrent_agents,
                current,
            });
        }

        Ok(self.sessions
            .entry(session_id)
            .or_insert_with(|| Arc::new(Session::new(session_id, self.user_id.clone())))
            .value()
            .clone())
    }
}
```

### 3.3 目录结构

```
data-platform/
├── platform.db              # 用户账号数据（已有）
├── users/
│   ├── {user_id_a}/
│   │   ├── db.sqlite       # 用户记忆、会话（AgentRuntime 使用）
│   │   └── ...
│   └── {user_id_b}/
│       └── ...
└── logs/
```

---

## 四、API 设计

### 4.1 会话管理 API

| Method | Path | 说明 |
|--------|------|------|
| GET | `/api/sessions` | 列出用户所有会话 |
| POST | `/api/sessions` | 创建新会话 |
| GET | `/api/sessions/{session_id}` | 获取会话详情 |
| DELETE | `/api/sessions/{session_id}` | 删除会话 |

**Request/Response:**
```rust
#[derive(Debug, Deserialize)]
pub struct CreateSessionRequest {
    pub name: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct SessionResponse {
    pub id: SessionId,
    pub user_id: UserId,
    pub name: Option<String>,
    pub status: SessionStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

pub enum SessionStatus {
    Active,
    Paused,
    Completed,
}
```

### 4.2 WebSocket API

| Path | 说明 |
|------|------|
| `WS /ws/{session_id}` | 实时对话 |

**消息格式：**
```rust
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum WsMessage {
    #[serde(rename = "chat")]
    Chat { content: String },

    #[serde(rename = "response")]
    Response { content: String, done: bool },

    #[serde(rename = "error")]
    Error { message: String },
}
```

---

## 五、实施任务

| 任务 | 内容 | 文件 |
|------|------|------|
| P1-3.1 | 定义 UserRuntimeConfig + 扩展 PlatformConfig | `config.rs` |
| P1-3.2 | 实现 UserRuntime 结构体 | `user_runtime.rs` (新增) |
| P1-3.3 | AppState 新增 users DashMap | `main.rs` |
| P1-3.4 | 实现会话 CRUD API | `api/sessions.rs` (新增) |
| P1-3.5 | 实现 WebSocket handler | `ws.rs` (新增) |
| P1-3.6 | 集成 AgentRuntime | `user_runtime.rs` |
| P1-3.7 | 单元测试 | `tests/` |

---

## 六、验收标准

1. ✅ 3 用户同时登录，各自创建会话，互不干扰
2. ✅ 并发超过 3 个会话时返回 429 错误
3. ✅ 用户数据存储在独立目录 `data-platform/users/{user_id}/`
4. ✅ WebSocket 连接正常，会话结束后资源释放
5. ✅ 代码编译通过，无警告
