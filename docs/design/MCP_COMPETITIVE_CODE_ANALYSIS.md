# MCP 集成竞品代码级分析报告

> 基于代码事实的深度对比分析，覆盖 octo-sandbox 与 5 个竞品框架的 MCP 实现。
> 分析日期：2026-03-12

---

## 一、分析对象概览

| 项目 | MCP 代码位置 | SDK 选择 | 代码行数(MCP模块) |
|------|-------------|----------|-------------------|
| **octo-sandbox** | `crates/octo-engine/src/mcp/` | rmcp 0.16 | ~700行 (7文件) |
| **goose** | `crates/goose/src/agents/extension_manager.rs` | rmcp 1.1.0 | ~1500行+ (分散) |
| **zeroclaw** | `src/tools/mcp_*.rs` | 手写 JSON-RPC | ~1600行 (4文件) |
| **ironclaw** | `src/tools/mcp/*.rs` | 手写 JSON-RPC | ~2000行 (6文件) |
| **openfang** | `crates/openfang-runtime/src/mcp*.rs` | 手写 JSON-RPC | ~800行 (2文件) |
| **moltis** | `crates/mcp/src/` | 手写 JSON-RPC | ~2500行 (11文件) |

---

## 二、八维度逐项分析

### D1：MCP SDK 版本与传输协议

| 项目 | SDK | 版本 | Stdio | SSE | StreamableHTTP | 代码证据 |
|------|-----|------|-------|-----|----------------|----------|
| **octo-sandbox** | rmcp | 0.16 | YES (TokioChildProcess) | YES (StreamableHttpClientTransport) | YES (通过rmcp SDK) | `Cargo.toml`: `features = ["client", "transport-child-process", "transport-streamable-http-client-reqwest"]` |
| **goose** | rmcp | 1.1.0 | YES | YES | YES (server+client) | `Cargo.toml`: `features = ["schemars", "auth"]`, 子crate启用 `transport-streamable-http-server` |
| **zeroclaw** | 手写 | N/A | YES (process spawn) | YES (完整SSE解析) | YES (HTTP POST + Mcp-Session-Id) | `mcp_transport.rs`: `HttpTransport` 实现 `Mcp-Session-Id` header 管理 |
| **ironclaw** | 手写 | N/A | NO | YES (HTTP POST + SSE响应解析) | 部分 (无Session管理) | `client.rs`: 仅 HTTP POST，响应通过 SSE 解析 |
| **openfang** | 手写 | N/A | YES (process spawn) | YES (reqwest-eventsource) | NO | `mcp.rs`: Stdio + SSE，无 StreamableHTTP 支持 |
| **moltis** | 手写 | N/A | YES (tokio Command) | YES (HTTP POST + SSE解析) | YES (完整实现) | `sse_transport.rs`: `Mcp-Session-Id` + `MCP-Protocol-Version` + DELETE session cleanup |

**关键发现**：
- octo-sandbox 使用 rmcp 0.16，**落后 goose 的 1.1.0 达 6 个主版本**。rmcp 1.x 引入了 auth feature、更完善的 StreamableHTTP 支持。
- moltis 手写的 StreamableHTTP 实现最为完善：支持 `Mcp-Session-Id` 双向传播、SSE/JSON 响应自动检测、DELETE 请求关闭 session、401 带 session header 的重试逻辑。
- zeroclaw 手写的传输层最复杂(~1060行)：三种传输全覆盖，SSE 支持 endpoint 发现、BOM 处理、后台 reader task。

**评分**：
| 项目 | 得分 | 理由 |
|------|------|------|
| goose | 9/10 | rmcp 1.1.0 最新，三种传输全覆盖，server 能力 |
| moltis | 9/10 | 手写但实现最完善，StreamableHTTP 全部细节正确 |
| zeroclaw | 8/10 | 手写三种传输，SSE 实现最复杂 |
| octo-sandbox | 7/10 | rmcp 0.16 过旧，但三种传输通过 SDK 覆盖 |
| ironclaw | 5/10 | 仅 HTTP+SSE，无 Stdio，无 StreamableHTTP session |
| openfang | 4/10 | 仅 Stdio+SSE，无 StreamableHTTP |

---

### D2：工具桥接设计 (Tool Bridging)

| 项目 | 桥接模式 | 命名方案 | 注解映射 | Null值清理 | 代码证据 |
|------|---------|---------|---------|-----------|----------|
| **octo-sandbox** | `McpToolBridge` impl `Tool` trait | `ToolSource::Mcp(server_name)` (无前缀) | annotations -> RiskLevel (destructive/open_world/read_only) | 无 | `bridge.rs` L47-58: `fn source() -> ToolSource::Mcp(...)`, `fn risk_level()` |
| **goose** | ExtensionManager 直接注册 rmcp 工具 | 扩展名前缀管理 | rmcp 内置处理 | 无 | `extension_manager.rs`: 通过 rmcp SDK 自动管理 |
| **zeroclaw** | `McpToolWrapper` impl `Tool` trait | `server__tool` 双下划线分隔 | 无 | 无 | `mcp_tool.rs`: McpRegistry 管理 prefixed names |
| **ironclaw** | `McpToolWrapper` with approval gate | 原始名 + server 关联 | `destructive_hint`/`side_effects_hint` -> `requires_approval` | 无 | `client.rs`: `requires_approval` from annotations, `requires_sanitization=true` for ALL MCP tools |
| **openfang** | 直接注入 agent tools | `mcp_{server}_{tool}` 单下划线 | 无 | 无 | `mcp.rs`: original_names HashMap 保留连字符 |
| **moltis** | `McpToolBridge` impl `McpAgentTool` trait | `mcp__{server}__{tool}` 双下划线 | 无 | **YES** -- 递归清除null + 过滤`_`前缀metadata | `tool_bridge.rs` L117-132: `strip_nulls_recursive()` + `_`前缀过滤 |

**关键发现**：
- moltis 的 tool bridge 最为成熟：递归 null 值清理（解决 LLM 生成的 nullable optional 字段问题）+ 内部元数据键过滤（`_session_key` 等），这是实战中发现的重要细节。
- octo-sandbox 是唯一将 MCP tool annotations 映射到 risk level 的实现，实现了 MCP 2025-03 规范的 `McpToolAnnotations`（read_only, destructive, open_world）。
- ironclaw 对所有 MCP 工具强制 `requires_sanitization=true`，体现最强的安全默认值。

**评分**：
| 项目 | 得分 | 理由 |
|------|------|------|
| moltis | 9/10 | null清理 + metadata过滤 + 清晰命名 |
| octo-sandbox | 8/10 | annotations -> RiskLevel 映射独有 |
| ironclaw | 8/10 | approval gate + sanitization 安全默认 |
| goose | 7/10 | rmcp SDK 自动处理，但定制空间小 |
| zeroclaw | 6/10 | 功能完整但无高级特性 |
| openfang | 5/10 | 基础桥接，无注解/清理 |

---

### D3：Resources/Prompts 支持

| 项目 | Resources (list/read) | Prompts (list/get) | 类型转换 | 代码证据 |
|------|----------------------|-------------------|---------|----------|
| **octo-sandbox** | **YES** -- list_resources + read_resource | **YES** -- list_prompts + get_prompt | `convert.rs`: map_resources, map_resource_content (text+blob), map_prompts, map_prompt_messages | `traits.rs`: McpClient trait 含 list_resources/read_resource/list_prompts/get_prompt (带默认实现); `manager.rs` 暴露完整 API |
| **goose** | **YES** -- list_resources + read_resource | **YES** -- list_prompts + get_prompt | rmcp SDK 内置，`mcp_utils.rs` 提取 text/blob | `extension_manager.rs`: `read_resource_tool()`, `list_resources_from_extension()`; `agent.rs` L1729-1754 |
| **zeroclaw** | **NO** | **NO** | N/A | `mcp_client.rs` 仅 tools/list + tools/call |
| **ironclaw** | **NO** (protocol.rs 定义了 ServerCapabilities 含 resources/prompts 字段但无调用) | **NO** | N/A | `protocol.rs`: `resources: Option<Value>` 仅解析能力声明 |
| **openfang** | **NO** | **NO** | N/A | `mcp.rs` 仅实现 tools/list + tools/call |
| **moltis** | **NO** (types.rs 定义 ServerCapabilities 含 resources/prompts 但未实现) | **NO** | N/A | `types.rs` L95-97: 仅 `serde_json::Value` 类型，McpClientTrait 无资源/提示方法 |

**关键发现**：
- **仅 octo-sandbox 和 goose 实现了 Resources 和 Prompts 支持**。其余 4 个项目全部缺失。
- octo-sandbox 的实现包含完整的类型转换层（`convert.rs`），将 rmcp 原生类型映射为内部 `McpResourceInfo`/`McpPromptInfo` 类型，支持 text 和 blob resource content。
- 这是 octo-sandbox 的显著优势——MCP 协议不仅仅是 tools，Resources 和 Prompts 是完整生态的关键组件。

**评分**：
| 项目 | 得分 | 理由 |
|------|------|------|
| octo-sandbox | 9/10 | 完整实现 + 类型转换层 |
| goose | 9/10 | 完整实现，通过 rmcp SDK |
| ironclaw | 2/10 | 仅能力声明解析，无实现 |
| moltis | 2/10 | 仅类型定义，无实现 |
| zeroclaw | 1/10 | 完全缺失 |
| openfang | 1/10 | 完全缺失 |

---

### D4：安全验证 (URI/SSRF)

| 项目 | URI 验证 | SSRF 防护 | HTTPS 强制 | 代码证据 |
|------|---------|----------|-----------|----------|
| **octo-sandbox** | `validate_resource_uri()`: scheme 白名单 + 路径遍历检测 | `validate_server_url()`: 私有 IP 阻断 (10.x, 172.16-31.x, 192.168.x, 169.254.x, loopback) | 非 localhost 强制 HTTPS | `traits.rs`: 两个独立验证函数，在 `stdio.rs` 和 `sse.rs` 中调用 |
| **goose** | rmcp SDK 内置 | 无独立 SSRF 防护 | rmcp SDK 处理 | 代码中未发现专门的 SSRF/URI 验证逻辑 |
| **zeroclaw** | 无 | 无 | 无 | `mcp_transport.rs`: 直接使用用户提供的 URL，无验证 |
| **ironclaw** | 无 URI 验证 | 无 SSRF 防护 | 远程服务器 HTTPS 验证 | `config.rs`: HTTPS validation for remote servers only |
| **openfang** | 无 URI 验证 | 基础: 仅阻断 metadata endpoints (169.254.x) | 无 | `mcp.rs`: `is_cloud_metadata_endpoint()` 仅检查 metadata IP |
| **moltis** | 无 MCP 专属验证 | **YES** -- 独立 `ssrf.rs` 模块，DNS 解析后验证 | 无 | `crates/tools/src/ssrf.rs`: `is_private_ip()` 覆盖 IPv4+IPv6 全范围 + CGNAT + allowlist; 但 MCP sse_transport 本身未调用 |

**关键发现**：
- **octo-sandbox 是唯一在 MCP 层面同时实现 URI 验证和 SSRF 防护的项目**。`validate_resource_uri()` 验证 URI scheme 白名单和路径遍历，`validate_server_url()` 阻断私有 IP。
- moltis 有非常完善的独立 SSRF 模块（`ssrf.rs`），包含 IPv4+IPv6 全覆盖、CGNAT 检测、configurable allowlist 和 async DNS 解析后验证，但**未集成到 MCP SSE transport 中**——这是一个明显的遗漏。
- openfang 仅检查 cloud metadata endpoint (169.254.169.254)，远不够全面。
- goose 完全依赖 rmcp SDK，无独立安全验证。

**评分**：
| 项目 | 得分 | 理由 |
|------|------|------|
| octo-sandbox | 9/10 | MCP 层面双重验证(URI+SSRF)，集成到实际调用链 |
| moltis | 7/10 | SSRF 模块最完善(IPv6/CGNAT/allowlist)但未集成到 MCP |
| ironclaw | 4/10 | 仅远程 HTTPS 验证 |
| openfang | 3/10 | 仅 metadata endpoint 检查 |
| goose | 2/10 | 完全依赖 SDK，无独立防护 |
| zeroclaw | 1/10 | 无任何安全验证 |

---

### D5：热插拔能力 (Hot-Plug)

| 项目 | 运行时添加/移除 | 启用/禁用 | 重启 | 注册表持久化 | 代码证据 |
|------|---------------|---------|------|------------|----------|
| **octo-sandbox** | `add_server`/`remove_server` | 无独立 enable/disable | 无独立 restart | `McpStorage` SQLite (mcp_servers 表) | `manager.rs`: add_server_v2 + remove_server; `storage.rs`: CRUD with user_id |
| **goose** | rmcp ExtensionManager 动态管理 | rmcp 内置 | rmcp 内置 | 配置文件 | 通过 rmcp SDK 管理生命周期 |
| **zeroclaw** | 配置驱动 | 无 | 无 | 配置文件 | `mcp_client.rs`: McpRegistry 在启动时加载 |
| **ironclaw** | Database 持久化 + JSON fallback | 无独立 API | 无 | DB + JSON file | `config.rs`: Database trait + JSON persistence |
| **openfang** | `connect_mcp_server` 运行时 | 无 | 无 | 无持久化 | `mcp.rs`: HashMap 管理连接 |
| **moltis** | `add_server`/`remove_server`/`update_server` | `enable_server`/`disable_server` | `restart_server` | `McpRegistry` JSON 文件持久化 | `manager.rs`: 完整生命周期; `registry.rs`: JSON file CRUD with save-on-write |

**关键发现**：
- moltis 的热插拔最完整：add/remove/update/enable/disable/restart 全套操作 + JSON 文件持久化 + 自动 save-on-write。
- octo-sandbox 有 add/remove 但缺少 enable/disable 和 restart 的独立 API。持久化通过 SQLite 实现，支持多用户过滤。
- openfang 最弱：无持久化，服务器重启后配置丢失。

**评分**：
| 项目 | 得分 | 理由 |
|------|------|------|
| moltis | 10/10 | 全套生命周期操作 + 持久化 + auto-save |
| octo-sandbox | 7/10 | add/remove + SQLite 持久化，缺 enable/disable/restart |
| ironclaw | 6/10 | DB 持久化，但操作 API 不完整 |
| goose | 6/10 | SDK 管理，定制有限 |
| zeroclaw | 4/10 | 仅配置驱动，无运行时管理 |
| openfang | 3/10 | 运行时添加但无持久化 |

---

### D6：持久化 (Persistence)

| 项目 | 服务器配置 | 执行记录 | 日志记录 | 多用户隔离 | 代码证据 |
|------|----------|---------|---------|-----------|----------|
| **octo-sandbox** | SQLite mcp_servers | SQLite mcp_executions (duration_ms) | SQLite mcp_logs (direction/method/raw_data/pagination) | YES -- user_id 字段 + per-user 查询 | `storage.rs`: 3 表完整 CRUD，支持 level filter + offset/limit 分页 |
| **goose** | 配置文件 | 无独立持久化 | 无独立持久化 | 无 | 依赖 rmcp SDK |
| **zeroclaw** | 配置文件 | 无 | 无 | 无 | 纯内存运行时 |
| **ironclaw** | Database trait + JSON | 无 | 无 | 无 | `config.rs`: 仅服务器配置持久化 |
| **openfang** | 无 | 无 | 无 | 无 | 纯内存 HashMap |
| **moltis** | JSON 文件 (mcp-servers.json) | 无 | 无 (但有 metrics feature) | 无 | `registry.rs`: JSON file; 无执行/日志持久化 |

**关键发现**：
- **octo-sandbox 在持久化维度遥遥领先**。三张独立的 SQLite 表覆盖服务器配置、执行记录（含耗时）和协议日志（含方向/方法/原始数据），支持分页查询和多用户隔离。
- 所有竞品最多只持久化服务器配置，无一实现执行记录或协议日志持久化。
- moltis 有 `#[cfg(feature = "metrics")]` 的运行时指标（连接数、工具调用次数、耗时 histogram），但非持久化。

**评分**：
| 项目 | 得分 | 理由 |
|------|------|------|
| octo-sandbox | 10/10 | 三表完整持久化 + 多用户隔离 + 分页 |
| moltis | 5/10 | 配置持久化 + 运行时 metrics |
| ironclaw | 4/10 | 配置持久化 (DB+JSON) |
| goose | 3/10 | 仅配置文件 |
| zeroclaw | 2/10 | 仅配置文件 |
| openfang | 1/10 | 无任何持久化 |

---

### D7：MCP Server 角色

| 项目 | 充当 MCP Server | 实现深度 | 代码证据 |
|------|----------------|---------|----------|
| **octo-sandbox** | **NO** | N/A | 仅 MCP Client |
| **goose** | **YES** -- 通过 rmcp server features | 完整 | `Cargo.toml`: goose-test-support 启用 `["server", "macros", "transport-streamable-http-server"]` |
| **zeroclaw** | **NO** | N/A | 仅 MCP Client |
| **ironclaw** | **NO** | N/A | 仅 MCP Client |
| **openfang** | **YES** -- 手写实现 | 基础: handle initialize/tools/list/tools/call | `mcp_server.rs`: `handle_mcp_request()` 处理 3 种方法，工具执行 stub 未完全接入 |
| **moltis** | **NO** | N/A | 仅 MCP Client |

**关键发现**：
- 仅 goose 和 openfang 实现了 MCP Server 角色。
- goose 通过 rmcp SDK 的 server features 实现，最为完整。
- openfang 手写了基础的 `handle_mcp_request()` 处理器（stateless），但工具执行是 stub，未完全接入运行时。
- octo-sandbox、zeroclaw、ironclaw、moltis 均仅为 MCP Client。

**评分**：
| 项目 | 得分 | 理由 |
|------|------|------|
| goose | 9/10 | rmcp SDK server features，完整实现 |
| openfang | 4/10 | 基础实现，工具执行 stub |
| 其余 | 0/10 | 未实现 |

---

### D8：认证与 OAuth 支持

| 项目 | OAuth 2.1 | PKCE | DCR (动态客户端注册) | Token 刷新 | Token 存储 | 代码证据 |
|------|----------|------|---------------------|-----------|-----------|----------|
| **octo-sandbox** | **NO** | NO | NO | NO | NO | 仅 `validate_server_url()` 的 HTTPS 检查 |
| **goose** | **YES** -- rmcp auth feature | 通过 SDK | 通过 SDK | 通过 SDK | 通过 SDK | `Cargo.toml`: `features = ["schemars", "auth"]`; `oauth/mod.rs`: AuthorizationManager |
| **zeroclaw** | **NO** | NO | NO | NO | NO | 无认证支持 |
| **ironclaw** | **YES** -- 手写完整实现 | YES (S256) | YES (RFC 7591) | YES (401 auto refresh) | SecretsStore | `auth.rs`: 完整 OAuth 2.1 + PKCE + DCR + browser auth flow + well-known discovery |
| **openfang** | **NO** | NO | NO | NO | NO | 无认证支持 |
| **moltis** | **YES** -- 最完善的手写实现 | YES (S256) | YES (RFC 7591) | YES (60s buffer 自动刷新) | JSON file (TokenStore + RegistrationStore) | `auth.rs`: RFC 9728 resource metadata discovery + RFC 8414 AS metadata + DCR + PKCE + token expiry with 60s buffer + path-aware fallback discovery |

**关键发现**：
- **moltis 的 OAuth 实现最为完善**：遵循 MCP Authorization Spec (2025-06-18)，实现了完整的 RFC 9728 Protected Resource Metadata 发现、RFC 8414 AS Metadata 发现、RFC 7591 动态客户端注册、PKCE (S256)、token 刷新（60s 缓冲区）、path-aware URL fallback（先尝试完整路径，失败后回退到 origin）。代码包含 `McpOAuthOverride` 支持手动覆盖发现流程。
- ironclaw 的 OAuth 同样完整，但缺少 moltis 的 path-aware fallback 逻辑。
- goose 通过 rmcp 1.1.0 的 auth feature 获得 OAuth 支持。
- **octo-sandbox 完全缺失 OAuth 支持**——这是最大的功能差距。

**评分**：
| 项目 | 得分 | 理由 |
|------|------|------|
| moltis | 10/10 | MCP Auth Spec 完整实现 + RFC 9728/8414/7591 + path-aware fallback |
| ironclaw | 9/10 | 完整 OAuth 2.1 + PKCE + DCR + token refresh |
| goose | 8/10 | 通过 rmcp auth feature，完整但不可定制 |
| octo-sandbox | 0/10 | 完全缺失 |
| zeroclaw | 0/10 | 完全缺失 |
| openfang | 0/10 | 完全缺失 |

---

## 三、综合评分矩阵

| 维度 | 权重 | octo-sandbox | goose | zeroclaw | ironclaw | openfang | moltis |
|------|------|-------------|-------|----------|----------|----------|--------|
| D1: SDK/传输 | 15% | 7 | 9 | 8 | 5 | 4 | 9 |
| D2: 工具桥接 | 10% | 8 | 7 | 6 | 8 | 5 | 9 |
| D3: Resources/Prompts | 15% | 9 | 9 | 1 | 2 | 1 | 2 |
| D4: 安全验证 | 15% | 9 | 2 | 1 | 4 | 3 | 7 |
| D5: 热插拔 | 10% | 7 | 6 | 4 | 6 | 3 | 10 |
| D6: 持久化 | 10% | 10 | 3 | 2 | 4 | 1 | 5 |
| D7: Server角色 | 10% | 0 | 9 | 0 | 0 | 4 | 0 |
| D8: OAuth认证 | 15% | 0 | 8 | 0 | 9 | 0 | 10 |
| **加权总分** | | **6.35** | **6.70** | **2.85** | **4.80** | **2.35** | **6.55** |

---

## 四、octo-sandbox 真实差距分析

### 严重差距 (CRITICAL)

1. **OAuth 2.1 完全缺失** (D8: 0/10)
   - 无法连接需要认证的 MCP 服务器（如 Linear、Notion 等 SaaS 服务的 MCP endpoint）
   - 竞品 moltis/ironclaw/goose 均已实现完整 OAuth 流程
   - 建议：参考 moltis `crates/mcp/src/auth.rs` 的实现，添加 RFC 9728 + RFC 8414 + RFC 7591 + PKCE 支持

2. **MCP Server 角色缺失** (D7: 0/10)
   - 无法将自身能力暴露为 MCP Server 供外部客户端调用
   - goose 通过 rmcp server features 实现，openfang 有基础手写实现
   - 建议：升级 rmcp 到 1.x 后启用 `transport-streamable-http-server` feature

### 重要差距 (HIGH)

3. **rmcp SDK 版本过旧** (0.16 vs goose 的 1.1.0)
   - 缺少 rmcp 1.x 的 auth feature、更完善的类型系统、server 能力
   - 建议：升级 rmcp 至 1.x，获得 OAuth + Server + 更好的 StreamableHTTP 支持

4. **热插拔 API 不完整** (D5: 7/10)
   - 缺少 enable/disable/restart 独立 API
   - 建议：参考 moltis McpManager 的完整生命周期管理

### 中等差距 (MEDIUM)

5. **工具桥接缺少 null 值清理** (D2: 8/10)
   - LLM 生成的 nullable optional 字段会传递 null 值到严格验证的 MCP server
   - 建议：参考 moltis `crates/mcp/src/tool_bridge.rs` 的 `strip_nulls_recursive()` 实现

6. **工具桥接缺少内部元数据过滤**
   - Agent runner 注入的 `_session_key` 等内部键可能传递到 MCP server
   - 建议：参考 moltis 的 `_` 前缀过滤逻辑

---

## 五、octo-sandbox 现有优势

1. **Resources/Prompts 完整支持** (D3: 9/10) -- 仅 goose 同级，其余全部缺失
2. **安全验证最佳** (D4: 9/10) -- 唯一在 MCP 层面实现 URI + SSRF 双重防护
3. **持久化最完善** (D6: 10/10) -- 三表 SQLite + 多用户隔离 + 分页查询，竞品无一达到
4. **Tool Annotations -> RiskLevel 映射** -- 唯一利用 MCP 2025-03 规范注解做安全分级

---

## 六、增强路线图建议

### Phase 1: 紧急 (1-2 周)
- [ ] 升级 rmcp 0.16 -> 1.x
- [ ] 添加 OAuth 2.1 支持（PKCE + DCR + token 刷新）
- [ ] 添加 tool bridge null 值清理 + metadata 过滤

### Phase 2: 重要 (2-4 周)
- [ ] 启用 rmcp server features，实现 MCP Server 角色
- [ ] 完善热插拔 API（enable/disable/restart）
- [ ] 添加 MCP 连接 metrics（连接数、调用次数、延迟）

### Phase 3: 增强 (4-8 周)
- [ ] StreamableHTTP session 管理增强（Mcp-Session-Id 双向传播）
- [ ] MCP server health check + 自动重连
- [ ] 集成 SSRF allowlist 到配置系统

---

## 七、代码证据索引

| 文件 | 项目 | 关键内容 |
|------|------|---------|
| `crates/octo-engine/src/mcp/traits.rs` | octo-sandbox | validate_resource_uri, validate_server_url, McpClient trait |
| `crates/octo-engine/src/mcp/bridge.rs` | octo-sandbox | McpToolBridge, annotations -> RiskLevel |
| `crates/octo-engine/src/mcp/storage.rs` | octo-sandbox | 三表 SQLite 持久化 |
| `crates/octo-engine/src/mcp/convert.rs` | octo-sandbox | Resources/Prompts 类型转换 |
| `crates/mcp/src/auth.rs` | moltis | 完整 OAuth 2.1 实现 |
| `crates/mcp/src/sse_transport.rs` | moltis | StreamableHTTP + Mcp-Session-Id |
| `crates/mcp/src/tool_bridge.rs` | moltis | null清理 + metadata过滤 |
| `crates/tools/src/ssrf.rs` | moltis | 完善的 SSRF 防护模块 |
| `src/tools/mcp/auth.rs` | ironclaw | OAuth 2.1 + PKCE + DCR |
| `src/tools/mcp_transport.rs` | zeroclaw | 三种手写传输(~1060行) |
| `crates/openfang-runtime/src/mcp_server.rs` | openfang | MCP Server 角色 |
