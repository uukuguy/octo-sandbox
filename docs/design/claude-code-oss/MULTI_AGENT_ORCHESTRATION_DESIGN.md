# Octo-Engine 多智能体编排增强设计

> 基于 CC-OSS 多 Agent 架构（单进程 Node.js + AsyncLocalStorage + 文件邮箱）与 Octo 多 Agent 架构（多 tokio task + mpsc channel + SessionRegistry + CollaborationManager）的代码级对比。
> 日期：2026-04-01
> 核心结论：Octo 的底层多 Agent 基础设施远优于 CC，但缺少两个上层抽象（团队 + 任务跟踪）和对应的 LLM 工具接口。

---

## 一、架构对比

### CC-OSS 多 Agent 模型

CC 受限于 Node.js 单进程模型，发展出 3 种 agent 模式：

| 模式 | 进程 | 隔离 | 通信 | 用途 |
|------|------|------|------|------|
| Sync Subagent | 同进程同步 | AsyncLocalStorage + clone cache | 返回值 | 短任务 |
| Fork Subagent | 同进程异步 | 共享 prompt cache | 完成通知 | 研究/实现（Anthropic 专属） |
| Teammate | 同进程/tmux | file mailbox + team.json | 文件邮箱投递 | 团队协作 |

上层编排：
- **Coordinator mode**：leader 决策、worker 执行，通过 `<task-notification>` XML 汇报
- **TeamCreate/Delete**：文件级团队管理 (`.claude/teams/{name}/team.json`)
- **SendMessage**：结构化消息（纯文本 + shutdown/plan_approval 协议）
- **6 个 Task 工具**：跟踪 agent 任务状态 + 依赖关系

### Octo 多 Agent 模型

Octo 基于 Rust + Tokio 构建，有 7 种 agent 能力：

| 模式 | 进程 | 隔离 | 通信 | 用途 |
|------|------|------|------|------|
| 主 Session | tokio task | DashMap entry + per-session ToolRegistry/KG/Memory | mpsc + broadcast | 用户交互 |
| Sub-Agent | tokio task (嵌套) | depth=3/concurrent=4 限制 | SubAgentManager map | 递归子任务 |
| Dual Agent | 两个 executor 共享 session | Plan(无工具) / Build(有工具) | switch() 切换 | 计划+执行 |
| Multi-Session | 独立 tokio task | 完全隔离的 session entry | mpsc channel | 并发会话 (Phase AJ) |
| Collaboration | 双向 MPSC channel | 共享 CollaborationContext | Byzantine 共识 | 多 agent 决策 |
| AgentCatalog | DashMap 注册表 | by_id/name/tag/tenant 多索引 | — | agent 发现 |
| AgentRouter | 关键词+置信度 | — | — | 任务路由 |

### 精确差距

| CC 能力 | Octo 状态 | 差距 |
|---------|----------|------|
| Sync subagent | SubAgentManager | 持平 |
| Fork subagent (cache 共享) | 无 | 不需要（Anthropic 专属） |
| Async background agent | multi-session | **Octo 更强** |
| TeamCreate/Delete | **无团队抽象** | **需补充** |
| SendMessage 结构化消息 | CollaborationManager | **Octo 更强**（双向 MPSC vs 文件邮箱） |
| File mailbox | mpsc channel | **Octo 更优** |
| Coordinator mode | Router + DualAgent + Collaboration | **不同但 Octo 更灵活** |
| 6 个 Task 工具 | **无结构化任务** | **需补充** |
| Agent memory (per-agent) | 共享 MemoryStore | 半覆盖 |
| Worktree isolation | Docker SessionSandboxManager | **Octo 更强** |
| Agent resume | SessionStore 持久化 | 持平 |

---

## 二、Octo 不应照搬 CC 的原因

CC 的多 Agent 设计是**受限于 Node.js 单进程模型的妥协方案**：

| CC 做法 | 为什么是妥协 | Octo 的更好方案 |
|---------|------------|----------------|
| AsyncLocalStorage 模拟隔离 | 单线程无真隔离 | 每 session 独立 tokio task |
| 文件邮箱 (.claude/teams/inboxes/) | 文件 I/O 延迟高 | mpsc channel 零拷贝 |
| tmux 窗格生成 teammate | 依赖终端环境 | 纯 server 端 session 管理 |
| Fork subagent 共享 prompt cache | Anthropic API 专属优化 | 多 Provider 无此需求 |
| Coordinator 手动分配 | 无调度器 | AgentRouter 自动路由 |

**结论：Octo 应在自己的优势基础设施上补充上层抽象，而非降级到 CC 的模型。**

---

## 三、需要补充的两个上层抽象

### 3.1 团队抽象层 (TeamManager)

**目的**：在 SessionRegistry 上加一层逻辑分组，让 LLM 能创建和管理 agent 团队。

**位置**：`crates/octo-engine/src/agent/team.rs` (新文件)

```rust
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use dashmap::DashMap;
use octo_types::SessionId;

/// 团队：一组 session 的逻辑分组
#[derive(Debug, Clone)]
pub struct Team {
    pub name: String,
    pub description: Option<String>,
    pub leader_session_id: SessionId,
    pub members: HashMap<String, TeamMember>,
    pub created_at: Instant,
}

#[derive(Debug, Clone)]
pub struct TeamMember {
    pub name: String,
    pub session_id: SessionId,
    pub agent_type: Option<String>,
    pub role: TeamRole,
    pub joined_at: Instant,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TeamRole {
    Leader,
    Worker,
}

/// 团队管理器
pub struct TeamManager {
    teams: DashMap<String, Team>,
}

impl TeamManager {
    pub fn new() -> Self {
        Self {
            teams: DashMap::new(),
        }
    }

    /// 创建团队，当前 session 成为 leader
    pub fn create_team(
        &self,
        name: &str,
        leader_session_id: SessionId,
        description: Option<&str>,
    ) -> Result<Team, String> {
        if self.teams.contains_key(name) {
            return Err(format!("Team '{}' already exists", name));
        }
        let team = Team {
            name: name.to_string(),
            description: description.map(String::from),
            leader_session_id: leader_session_id.clone(),
            members: HashMap::from([(
                "leader".to_string(),
                TeamMember {
                    name: "leader".to_string(),
                    session_id: leader_session_id,
                    agent_type: None,
                    role: TeamRole::Leader,
                    joined_at: Instant::now(),
                },
            )]),
            created_at: Instant::now(),
        };
        self.teams.insert(name.to_string(), team.clone());
        Ok(team)
    }

    /// 添加成员（创建新 session 并加入团队）
    pub fn add_member(
        &self,
        team_name: &str,
        member_name: &str,
        session_id: SessionId,
        agent_type: Option<&str>,
    ) -> Result<(), String> {
        let mut team = self.teams.get_mut(team_name)
            .ok_or_else(|| format!("Team '{}' not found", team_name))?;
        team.members.insert(
            member_name.to_string(),
            TeamMember {
                name: member_name.to_string(),
                session_id,
                agent_type: agent_type.map(String::from),
                role: TeamRole::Worker,
                joined_at: Instant::now(),
            },
        );
        Ok(())
    }

    /// 解散团队
    pub fn dissolve_team(&self, team_name: &str) -> Result<Vec<SessionId>, String> {
        let (_, team) = self.teams.remove(team_name)
            .ok_or_else(|| format!("Team '{}' not found", team_name))?;
        Ok(team.members.values().map(|m| m.session_id.clone()).collect())
    }

    /// 查找成员的 session_id
    pub fn find_member(&self, team_name: &str, member_name: &str) -> Option<SessionId> {
        self.teams.get(team_name)
            .and_then(|t| t.members.get(member_name).map(|m| m.session_id.clone()))
    }

    /// 列出所有团队
    pub fn list_teams(&self) -> Vec<Team> {
        self.teams.iter().map(|e| e.value().clone()).collect()
    }
}
```

**AgentRuntime 集成**：

```rust
// runtime.rs 新增字段
pub(crate) team_manager: Arc<TeamManager>,

// 暴露给 LLM 工具
pub fn team_manager(&self) -> &Arc<TeamManager> {
    &self.team_manager
}
```

**预估**: ~200 行

### 3.2 结构化任务跟踪 (TaskTracker)

**目的**：让 LLM 能创建、更新、列出结构化任务，跟踪工作进度。

**位置**：`crates/octo-engine/src/agent/task_tracker.rs` (新文件)

```rust
use std::sync::Arc;
use std::time::Instant;

use dashmap::DashMap;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Pending,
    InProgress,
    Completed,
    Blocked,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackedTask {
    pub id: String,
    pub subject: String,
    pub description: String,
    pub status: TaskStatus,
    pub owner: Option<String>,       // agent 名称或 session_id
    pub team: Option<String>,        // 所属团队
    pub created_at: String,          // ISO 8601
    pub updated_at: String,
}

/// 结构化任务跟踪器
pub struct TaskTracker {
    tasks: DashMap<String, TrackedTask>,
    next_id: std::sync::atomic::AtomicU32,
}

impl TaskTracker {
    pub fn new() -> Self {
        Self {
            tasks: DashMap::new(),
            next_id: std::sync::atomic::AtomicU32::new(1),
        }
    }

    pub fn create(&self, subject: &str, description: &str, team: Option<&str>) -> TrackedTask {
        let id = format!("task-{}", self.next_id.fetch_add(1, std::sync::atomic::Ordering::Relaxed));
        let now = chrono::Utc::now().to_rfc3339();
        let task = TrackedTask {
            id: id.clone(),
            subject: subject.to_string(),
            description: description.to_string(),
            status: TaskStatus::Pending,
            owner: None,
            team: team.map(String::from),
            created_at: now.clone(),
            updated_at: now,
        };
        self.tasks.insert(id, task.clone());
        task
    }

    pub fn update(&self, id: &str, status: Option<TaskStatus>, owner: Option<&str>) -> Result<TrackedTask, String> {
        let mut entry = self.tasks.get_mut(id)
            .ok_or_else(|| format!("Task '{}' not found", id))?;
        if let Some(s) = status {
            entry.status = s;
        }
        if let Some(o) = owner {
            entry.owner = Some(o.to_string());
        }
        entry.updated_at = chrono::Utc::now().to_rfc3339();
        Ok(entry.clone())
    }

    pub fn list(&self, team: Option<&str>) -> Vec<TrackedTask> {
        self.tasks.iter()
            .filter(|e| team.map_or(true, |t| e.team.as_deref() == Some(t)))
            .map(|e| e.value().clone())
            .collect()
    }

    pub fn get(&self, id: &str) -> Option<TrackedTask> {
        self.tasks.get(id).map(|e| e.value().clone())
    }
}
```

**AgentRuntime 集成**：

```rust
// runtime.rs 新增字段
pub(crate) task_tracker: Arc<TaskTracker>,
```

**预估**: ~150 行

---

## 四、LLM 工具接口

已在 `TOOL_SYSTEM_ENHANCEMENT_DESIGN.md` 中设计。此处补充团队相关工具：

### team_create

```
名称: team_create
参数: name (string), description (optional string)
描述: 创建 agent 团队。你成为 leader，可以通过 session_create 添加成员。
```

### team_add_member

```
名称: team_add_member
参数: team (string), name (string), agent_type (optional string), prompt (string)
描述: 向团队添加新成员。自动创建子会话并加入团队。
```

### team_dissolve

```
名称: team_dissolve
参数: team (string)
描述: 解散团队，停止所有成员会话。
```

**工具描述手册**（工具-提示词三层耦合）：

```text
## 何时使用团队
- 需要 3+ 个 agent 协作的复杂任务
- 不同子任务需要不同专长（如：研究 + 实现 + 测试）
- 需要并行处理多个独立模块

## 何时不使用
- 简单的单 agent 任务
- 只需要 1-2 个子会话的情况（直接用 session_create）

## 使用模式
1. team_create 创建团队
2. team_add_member 为每个角色添加成员
3. session_message 分配任务
4. 收到子会话完成通知后综合结果
5. team_dissolve 清理
```

**预估**: 3 个工具 ~120 行

---

## 五、与现有架构的集成点

```
AgentRuntime
├─ sessions: DashMap (已有, Phase AJ)
├─ team_manager: Arc<TeamManager> (新增)
├─ task_tracker: Arc<TaskTracker> (新增)
├─ catalog: Arc<AgentCatalog> (已有)
├─ collaboration: CollaborationManager (已有)
│
└─ LLM Tools:
   ├─ session_create → runtime.start_session() (已有 API)
   ├─ session_message → session.send_message() → mpsc channel (已有)
   ├─ session_status → runtime.get_session_handle() (已有 API)
   ├─ session_stop → runtime.stop_session() (已有 API)
   ├─ team_create → team_manager.create_team() (新增)
   ├─ team_add_member → team_manager.add_member() + runtime.start_session() (组合)
   ├─ team_dissolve → team_manager.dissolve_team() + runtime.stop_session() (组合)
   ├─ task_create → task_tracker.create() (新增)
   ├─ task_update → task_tracker.update() (新增)
   └─ task_list → task_tracker.list() (新增)
```

**关键点**：所有新增 LLM 工具都是对**已有基础设施的薄包装**。session_create/message/status/stop 直接调用 Phase AJ 实现的 SessionRegistry API。团队工具是 SessionRegistry + TeamManager 的组合。

---

## 六、工作量总结

| 组件 | 新增代码 | 依赖 |
|------|---------|------|
| TeamManager (`agent/team.rs`) | ~200 行 | 无 |
| TaskTracker (`agent/task_tracker.rs`) | ~150 行 | 无 |
| team_create/add_member/dissolve 工具 | ~120 行 | TeamManager + SessionRegistry |
| session_create/message/status/stop 工具 | ~300 行 | SessionRegistry (已有) |
| task_create/update/list 工具 | ~200 行 | TaskTracker |
| 工具描述手册 | ~300 行 | 无 |
| 系统提示词指导 | ~30 行 | 无 |
| runtime.rs 集成 | ~20 行 | 无 |
| **合计** | **~1320 行** | |

---

## 七、Octo 已有优势（保持不动）

| 能力 | 说明 | CC 无等价物 |
|------|------|-----------|
| **CollaborationManager** | 双向 MPSC channel + Byzantine 共识 + 共享上下文 | CC 只有文件邮箱 |
| **DualAgentManager** | Plan/Build 双模切换 + 步骤提取注入 | CC 的 plan mode 更简单 |
| **AgentRouter** | 关键词+置信度路由 | CC 无自动路由 |
| **AgentCatalog** | DashMap 多索引注册 (id/name/tag/tenant) | CC 只有 flat list |
| **SessionSandboxManager** | Docker 容器级隔离 | CC 只有 git worktree |
| **SubAgentManager** | depth=3/concurrent=4 严格限制 | CC 无递归限制 |
| **Session-end 记忆管道** | 规则提取 → LLM 事实 → 摘要 → 程序模式 | CC 的 session memory 更简单 |

---

## 八、CC 模式中不需要实现的

| CC 模式 | 为什么不需要 |
|---------|------------|
| **Fork subagent** | Anthropic prompt cache 共享专属，多 Provider 下无意义 |
| **File mailbox** | mpsc channel 延迟更低、无文件 I/O 开销 |
| **Tmux/iTerm2 backend** | Octo 是 server/platform，不依赖终端环境 |
| **Coordinator mode** | Octo 的 Router + DualAgent + Collaboration 更灵活 |
| **AsyncLocalStorage** | tokio task 天然隔离，不需要模拟 |
| **team.json 文件持久化** | DashMap 内存管理，session 持久化已由 SessionStore 处理 |
