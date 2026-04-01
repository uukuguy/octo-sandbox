# Octo-Engine P1 增强设计：权限系统 + Context Collapse + Tool 接口

> 基于 Claude Code OSS 对比分析和企业智能体平台定位设计。
> 日期：2026-04-01
> 前置依赖：P0 上下文管理增强（CONTEXT_MANAGEMENT_ENHANCEMENT_DESIGN.md）

---

## 一、P1 改进清单

| 编号 | 改进项 | 概述 | 预估代码量 |
|------|--------|------|-----------|
| P1-1 | Permission 细粒度化 | 6 层规则引擎 + 通配符语法 + 决策追踪 | ~500 行 |
| P1-2 | Context Collapse | 粒度级折叠，启发式重要性评分 | ~200 行 |
| P1-3 | Snip Compact | 用户主动裁剪标记 | ~50 行 |
| P1-4 | Tool 接口增强 | is_read_only/is_destructive/validate_input | ~130 行 |

---

## 二、P1-1: Permission 细粒度化（核心改进）

### 2.1 规则语法

采用 Claude Code 的 `ToolName(pattern)` 格式：

```
bash(git *)          → 匹配所有 git 命令
file_edit(src/**/*.rs) → 匹配 src 下所有 Rust 文件编辑
bash(rm -rf *)       → 匹配所有 rm -rf 命令
file_read            → 匹配所有文件读取（无括号 = 匹配全部）
*                    → 匹配所有工具的所有调用
```

#### 规则数据结构

```rust
/// 单条权限规则
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionRule {
    /// 工具名（"bash", "file_edit", "*"）
    pub tool_name: String,
    /// glob 模式（None = 匹配该工具所有调用）
    pub pattern: Option<String>,
}

/// 规则行为
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PermissionBehavior {
    Allow,
    Deny,
    Ask,
}
```

#### 规则解析

```rust
impl PermissionRule {
    /// 从字符串解析: "bash(git *)" → tool_name="bash", pattern=Some("git *")
    pub fn parse(s: &str) -> Result<Self> {
        if let Some(paren_start) = s.find('(') {
            if s.ends_with(')') {
                let tool_name = s[..paren_start].trim().to_string();
                let pattern = s[paren_start + 1..s.len() - 1].trim().to_string();
                return Ok(Self {
                    tool_name,
                    pattern: Some(pattern),
                });
            }
        }
        // 无括号：匹配工具所有调用
        Ok(Self {
            tool_name: s.trim().to_string(),
            pattern: None,
        })
    }
}
```

#### 规则匹配

```rust
/// Per-tool 参数提取映射
fn extract_match_target(tool_name: &str, input: &Value) -> Option<String> {
    match tool_name {
        "bash" => input.get("command").and_then(|v| v.as_str()).map(String::from),
        "file_read" | "file_write" | "file_edit" =>
            input.get("file_path").and_then(|v| v.as_str()).map(String::from),
        "grep" | "glob" | "find" =>
            input.get("path").and_then(|v| v.as_str()).map(String::from),
        "web_fetch" | "web_search" =>
            input.get("url").or(input.get("query"))
                 .and_then(|v| v.as_str()).map(String::from),
        _ => {
            // 默认：序列化整个 input 作为匹配目标
            Some(input.to_string())
        }
    }
}

impl PermissionRule {
    /// 检查是否匹配给定的工具调用
    pub fn matches(&self, tool_name: &str, input: &Value) -> bool {
        // 工具名匹配（支持 "*"）
        if self.tool_name != "*" && self.tool_name != tool_name {
            return false;
        }
        // 无 pattern = 匹配该工具所有调用
        let pattern = match &self.pattern {
            None => return true,
            Some(p) => p,
        };
        // 提取匹配目标
        let target = match extract_match_target(tool_name, input) {
            Some(t) => t,
            None => return false,
        };
        // glob 匹配
        glob_match(pattern, &target)
    }
}

/// 简单 glob 匹配（支持 * 和 **）
fn glob_match(pattern: &str, text: &str) -> bool {
    // 使用 picomatch 或自实现
    // "*" 匹配除 "/" 外的任意字符
    // "**" 匹配任意字符（包括 "/"）
    picomatch::is_match(pattern, text)
}
```

### 2.2 六层规则体系

#### 规则来源

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum RuleSource {
    Platform = 1,    // 平台管理员设定（octo-platform-server 从 DB 加载）
    Tenant = 2,      // 租户管理员设定（octo-platform-server 从 DB 加载）
    Project = 3,     // 项目级（$PROJECT/.octo/security_rules.yaml, git 提交）
    User = 4,        // 用户级（~/.octo/security_rules.yaml）
    Session = 5,     // 会话级（CLI 参数 / API 请求）
    ToolDefault = 6, // 工具自身声明（Tool trait 的 risk_level/approval）
}
```

#### 规则集

```rust
#[derive(Debug, Clone)]
pub struct PermissionRuleSet {
    pub source: RuleSource,
    pub allow_rules: Vec<PermissionRule>,
    pub deny_rules: Vec<PermissionRule>,
    pub ask_rules: Vec<PermissionRule>,
}
```

#### 规则文件格式

```yaml
# ~/.octo/security_rules.yaml 或 $PROJECT/.octo/security_rules.yaml
rules:
  allow:
    - "file_read"
    - "bash(git *)"
    - "bash(cargo *)"
    - "grep"
    - "glob"
  deny:
    - "bash(rm -rf *)"
    - "bash(curl * | sh)"
    - "bash(sudo *)"
    - "file_edit(/etc/**)"
    - "file_edit(~/.ssh/**)"
  ask:
    - "bash(pip install *)"
    - "bash(npm install *)"
    - "file_write"
```

### 2.3 权限引擎

```rust
pub struct PermissionEngine {
    rule_sets: Vec<PermissionRuleSet>,
}

impl PermissionEngine {
    /// 空引擎
    pub fn empty() -> Self {
        Self { rule_sets: vec![] }
    }

    /// 从 YAML 文件加载（workbench/cli 用）
    pub fn from_files(
        project_rules: Option<&Path>,
        user_rules: Option<&Path>,
    ) -> Result<Self> {
        let mut engine = Self::empty();
        if let Some(path) = user_rules {
            if path.exists() {
                let rules = load_rules_from_yaml(path, RuleSource::User)?;
                engine.add_rule_set(rules);
            }
        }
        if let Some(path) = project_rules {
            if path.exists() {
                let rules = load_rules_from_yaml(path, RuleSource::Project)?;
                engine.add_rule_set(rules);
            }
        }
        Ok(engine)
    }

    /// 添加规则集
    pub fn add_rule_set(&mut self, rules: PermissionRuleSet) {
        self.rule_sets.push(rules);
        self.rule_sets.sort_by_key(|r| r.source);
    }

    /// 添加 session 级临时规则
    pub fn add_session_rules(&mut self, rules: PermissionRuleSet) {
        self.add_rule_set(rules);
    }

    /// 评估工具调用权限
    pub fn evaluate(
        &self,
        tool_name: &str,
        input: &Value,
    ) -> PermissionDecision {
        // 1. 从最高优先级开始，检查 deny 规则
        //    deny 向下穿透，不可被低优先级 allow 覆盖
        for rule_set in &self.rule_sets {
            for rule in &rule_set.deny_rules {
                if rule.matches(tool_name, input) {
                    return PermissionDecision::Deny {
                        source: rule_set.source,
                        rule: rule.clone(),
                        reason: format!(
                            "{:?} rule denies {}({})",
                            rule_set.source,
                            tool_name,
                            rule.pattern.as_deref().unwrap_or("*")
                        ),
                    };
                }
            }
        }

        // 2. 没有 deny 命中，从最高优先级检查 allow 和 ask
        for rule_set in &self.rule_sets {
            for rule in &rule_set.allow_rules {
                if rule.matches(tool_name, input) {
                    return PermissionDecision::Allow {
                        source: rule_set.source,
                        rule: rule.clone(),
                    };
                }
            }
            for rule in &rule_set.ask_rules {
                if rule.matches(tool_name, input) {
                    return PermissionDecision::Ask {
                        source: rule_set.source,
                        rule: rule.clone(),
                    };
                }
            }
        }

        // 3. 无规则匹配 → 回退到 Tool trait 默认
        PermissionDecision::UseToolDefault
    }
}
```

### 2.4 决策类型

```rust
#[derive(Debug, Clone)]
pub enum PermissionDecision {
    /// 规则明确允许
    Allow {
        source: RuleSource,
        rule: PermissionRule,
    },
    /// 规则明确拒绝
    Deny {
        source: RuleSource,
        rule: PermissionRule,
        reason: String,
    },
    /// 规则要求人工确认
    Ask {
        source: RuleSource,
        rule: PermissionRule,
    },
    /// 无规则匹配，使用工具默认声明
    UseToolDefault,
}
```

### 2.5 与现有组件的集成

```
改进前:
  harness → ApprovalManager.check_requirement(tool, approval, risk)
                ↓
            ApprovalDecision (Approved/NeedsApproval/Denied)
                ↓
            ApprovalGate (WebSocket 异步审批)

改进后:
  harness → PermissionEngine.evaluate(tool_name, input)
                ↓
            PermissionDecision
                ├─ Allow → 直接执行
                ├─ Deny → 拒绝 + 记录审计
                ├─ Ask → ApprovalManager → ApprovalGate
                └─ UseToolDefault → ApprovalManager.check_requirement() (现有逻辑)
```

`ApprovalManager` 和 `ApprovalGate` 完全保留，PermissionEngine 是它们的上游决策者。

### 2.6 各产品集成

| 产品 | 初始化方式 | 启用的层级 |
|------|-----------|-----------|
| **octo-cli** | `PermissionEngine::from_files(project, user)` + session 规则 | Project + User + Session + ToolDefault |
| **octo-server** | 同 CLI + WS 审批 | Project + User + Session + ToolDefault |
| **octo-platform-server** | `from_files` + `add_rule_set(platform, tenant)` (从 DB) | 全部 6 层 |

### 2.7 文件清单

| 文件 | 内容 | 预估行数 |
|------|------|---------|
| `security/permission_engine.rs` (新) | PermissionEngine 核心 | ~250 行 |
| `security/permission_rule.rs` (新) | 规则解析 + 匹配 | ~150 行 |
| `security/permission_types.rs` (新) | 类型定义 | ~60 行 |
| `security/mod.rs` | 导出新模块 | ~5 行 |
| `agent/harness.rs` | 集成 PermissionEngine 评估 | ~40 行 |
| `agent/loop_config.rs` | 新增 permission_engine 字段 | ~10 行 |

---

## 三、P1-2: Context Collapse（粒度级折叠）

### 3.1 位置

新增文件：`context/collapse.rs`

### 3.2 设计

```rust
pub struct ContextCollapser {
    /// 保护最近 N 轮不折叠
    pub keep_recent_turns: usize,
}

impl Default for ContextCollapser {
    fn default() -> Self {
        Self { keep_recent_turns: 3 }
    }
}

impl ContextCollapser {
    /// 将消息折叠到目标 token 数以下
    pub fn collapse(
        &self,
        messages: &mut Vec<ChatMessage>,
        target_tokens: usize,
        current_tokens: usize,
    ) -> usize {
        if current_tokens <= target_tokens {
            return 0;
        }

        let tokens_to_free = current_tokens - target_tokens;
        let protect_from = messages.len().saturating_sub(self.keep_recent_turns * 2);

        // 对可折叠消息打分
        let mut scored: Vec<(usize, f32, usize)> = Vec::new(); // (index, score, est_tokens)
        for (i, msg) in messages.iter().enumerate() {
            if i >= protect_from { break; } // 保护最近消息
            let score = Self::score_message(msg);
            let tokens = estimate_message_tokens(msg);
            scored.push((i, score, tokens));
        }

        // 按分数升序排列（最低分最先折叠）
        scored.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

        // 从最低分开始折叠，直到释放足够 token
        let mut freed = 0usize;
        let mut collapsed_count = 0usize;
        for (idx, _score, tokens) in &scored {
            if freed >= tokens_to_free { break; }
            Self::collapse_message(&mut messages[*idx]);
            freed += tokens;
            collapsed_count += 1;
        }

        collapsed_count
    }

    /// 消息重要性评分（0-100，越高越重要）
    fn score_message(msg: &ChatMessage) -> f32 {
        match msg.role {
            MessageRole::User => 100.0,       // 用户消息永不折叠
            MessageRole::System => 90.0,      // 系统消息几乎不折叠
            MessageRole::Assistant => {
                let text = msg.text_content();
                let has_code = text.contains("```") || text.contains("fn ") || text.contains("def ");
                let has_error = text.to_lowercase().contains("error") || text.contains("fix");
                let is_tool_result = msg.content.iter().any(|b| matches!(b, ContentBlock::ToolResult { .. }));
                let result_len = msg.content.iter()
                    .filter_map(|b| if let ContentBlock::ToolResult { content, .. } = b { Some(content.len()) } else { None })
                    .sum::<usize>();

                match (is_tool_result, result_len) {
                    (true, len) if len > 2000 => 20.0,  // 大工具结果：低优先（先折叠）
                    (true, len) if len > 500 => 40.0,   // 中等工具结果
                    (true, _) => 60.0,                    // 小工具结果
                    _ if has_code => 80.0,                // 含代码的回复
                    _ if has_error => 70.0,               // 含错误信息
                    _ => 50.0,                            // 普通文本回复
                }
            }
        }
    }

    /// 将消息内容替换为一行摘要
    fn collapse_message(msg: &mut ChatMessage) {
        let summary = match msg.role {
            MessageRole::Assistant => {
                let tool_names: Vec<_> = msg.content.iter().filter_map(|b| {
                    if let ContentBlock::ToolUse { name, .. } = b { Some(name.as_str()) } else { None }
                }).collect();

                if !tool_names.is_empty() {
                    let result_lens: Vec<_> = msg.content.iter().filter_map(|b| {
                        if let ContentBlock::ToolResult { content, .. } = b { Some(content.len()) } else { None }
                    }).collect();
                    format!("[Collapsed: {}() → {} chars output]",
                        tool_names.join(", "),
                        result_lens.iter().sum::<usize>()
                    )
                } else {
                    let text = msg.text_content();
                    let preview = if text.len() > 80 { &text[..80] } else { &text };
                    format!("[Collapsed: {}...]", preview.trim())
                }
            }
            _ => return, // 不折叠 user/system 消息
        };

        msg.content = vec![ContentBlock::Text { text: summary }];
    }
}
```

### 3.3 Harness 集成

在 `harness.rs` 的 context 管理段，DegradationLevel 介于 SoftTrim 和 AutoCompaction 之间时触发：

```rust
// 新增：AutoCompaction 之前先尝试 collapse
if level == DegradationLevel::AutoCompaction {
    let collapser = ContextCollapser::default();
    let target = (ctx_window as f64 * 0.7) as usize; // 目标 70% 使用率
    let current = budget.estimate_total_usage(&system_prompt, &messages, &tool_specs) as usize;
    let collapsed = collapser.collapse(&mut messages, target, current);
    if collapsed > 0 {
        debug!(collapsed, "Context collapse freed messages");
        // 重新计算，如果够了就跳过 truncate
        let new_level = budget.compute_degradation_level(&system_prompt, &messages, &tool_specs);
        if new_level < DegradationLevel::AutoCompaction {
            continue; // 无需进一步压缩
        }
    }
}
```

---

## 四、P1-3: Snip Compact

### 4.1 位置

新增到 `context/compaction_pipeline.rs` 中作为方法。

### 4.2 设计

```rust
pub const SNIP_MARKER: &str = "[SNIP]";

impl CompactionPipeline {
    /// 从 snip 标记处截断消息历史
    /// 如果有 provider，先生成摘要；否则直接截断
    pub async fn snip_compact(
        messages: &mut Vec<ChatMessage>,
        provider: Option<&dyn Provider>,
        model: &str,
        pipeline: Option<&CompactionPipeline>,
        context: Option<&CompactionContext>,
    ) -> Result<usize> {
        let pos = messages.iter().rposition(|m| {
            m.content.iter().any(|b| {
                if let ContentBlock::Text { text } = b {
                    text.contains(SNIP_MARKER)
                } else {
                    false
                }
            })
        });

        let pos = match pos {
            Some(p) => p,
            None => return Ok(0),
        };

        // 如果有 pipeline + provider，对被删除的部分生成摘要
        if let (Some(pipeline), Some(provider), Some(ctx)) = (pipeline, provider, context) {
            let to_summarize = &messages[..pos];
            if to_summarize.len() >= 2 {
                if let Ok(result) = pipeline.compact(to_summarize, provider, model, ctx).await {
                    // 用摘要替换被删除部分
                    let removed = pos + 1;
                    messages.drain(..=pos);
                    messages.insert(0, result.boundary_marker);
                    for (i, msg) in result.summary_messages.into_iter().enumerate() {
                        messages.insert(1 + i, msg);
                    }
                    return Ok(removed);
                }
            }
        }

        // 无 pipeline 时直接截断
        let removed = pos + 1;
        messages.drain(..=pos);
        Ok(removed)
    }
}
```

---

## 五、P1-4: Tool 接口增强

### 5.1 新增 trait 方法

```rust
#[async_trait]
pub trait Tool: Send + Sync {
    // ... 现有方法保持不变 ...

    /// 是否只读操作
    fn is_read_only(&self) -> bool { false }

    /// 是否可能造成不可逆破坏
    fn is_destructive(&self) -> bool { false }

    /// 是否可与其他工具并行执行
    fn is_concurrency_safe(&self) -> bool { true }

    /// 输入参数验证（执行前调用）
    async fn validate_input(&self, _params: &Value, _ctx: &ToolContext) -> Result<()> {
        Ok(())
    }
}
```

### 5.2 各工具声明

| 工具 | is_read_only | is_destructive | is_concurrency_safe |
|------|-------------|----------------|-------------------|
| file_read | true | false | true |
| grep | true | false | true |
| glob | true | false | true |
| find | true | false | true |
| web_search | true | false | true |
| web_fetch | true | false | true |
| memory_recall | true | false | true |
| memory_search | true | false | true |
| file_write | false | false | false |
| file_edit | false | false | false |
| bash | false | **depends** | false |
| memory_store | false | false | true |
| memory_edit | false | false | true |
| memory_forget | false | true | true |

### 5.3 Bash validate_input 示例

```rust
impl Tool for BashTool {
    async fn validate_input(&self, params: &Value, ctx: &ToolContext) -> Result<()> {
        let command = params.get("command")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("Missing 'command' parameter"))?;

        // 基础危险命令检测
        let lower = command.to_lowercase();
        if lower.contains("rm -rf /") && !lower.contains("rm -rf ./") {
            return Err(anyhow!("Dangerous command detected: 'rm -rf /' targets root filesystem"));
        }
        if lower.contains("$(") || lower.contains("`") {
            // 命令替换存在注入风险，标记但不阻止（由 PermissionEngine 决策）
            tracing::warn!(command, "Command contains substitution, review recommended");
        }

        Ok(())
    }

    fn is_destructive(&self) -> bool {
        false // 取决于具体命令，不能在 trait 层面确定
    }
}
```

### 5.4 Harness 集成

```rust
// 在工具执行前，先调用 validate_input
if let Some(tool) = tools.get(&tu.name) {
    if let Err(e) = tool.validate_input(&input, &tool_ctx).await {
        warn!(tool = %tu.name, "Input validation failed: {e}");
        // 返回错误结果而不是直接终止
        tool_results.push(ToolOutput::error(format!("Validation error: {e}")));
        continue;
    }
}
```

---

## 六、P1 实施分组

### G1: PermissionEngine（P1-1）

依赖：无
新增文件：security/permission_engine.rs, permission_rule.rs, permission_types.rs
修改文件：security/mod.rs, agent/harness.rs, agent/loop_config.rs
可独立测试：是

### G2: Context Collapse + Snip（P1-2 + P1-3）

依赖：P0-2（CompactionPipeline，snip 的摘要功能）
新增文件：context/collapse.rs
修改文件：context/mod.rs, agent/harness.rs
可独立测试：是（collapse 不依赖 LLM）

### G3: Tool 接口增强（P1-4）

依赖：P1-1（validate_input 与 PermissionEngine 协作）
修改文件：tools/traits.rs + 各工具实现文件
可独立测试：是

### 推荐顺序

```
G1 (PermissionEngine) → G3 (Tool 接口) → G2 (Collapse + Snip)
     │                       │
     └── G3 的 validate_input 与 G1 的规则匹配协作
```

---

## 七、总预估

| 模块 | 新增代码 | 修改代码 | 总计 |
|------|---------|---------|------|
| PermissionEngine | ~460 行 | ~55 行 | ~515 行 |
| Context Collapse | ~200 行 | ~20 行 | ~220 行 |
| Snip Compact | ~50 行 | ~5 行 | ~55 行 |
| Tool 接口增强 | ~30 行 | ~100 行 | ~130 行 |
| **P1 总计** | **~740 行** | **~180 行** | **~920 行** |

加上 P0 的 ~1000 行，整个追赶方案总计约 **~2000 行新/修改代码**。
