# LLM Provider 层代码级竞品分析

> 基于 octo-sandbox 及 3th-party/harnesses/rust-projects/ 下 8 个 Rust 项目的**实际代码**深入分析。
> 分析日期：2026-03-12
> 分析原则：基于**功能等价性**判断能力，不是数 struct 数量。

---

## 一、octo-sandbox Provider 层全景

### 1.1 文件清单与职责

| 文件 | 行数 | 职责 |
|------|------|------|
| `traits.rs` | 34 | Provider trait: `complete()`, `stream()`, `embed()`, `metering()` |
| `openai.rs` | 919 | OpenAI-compatible provider, `with_base_url()` 支持任意 OpenAI 兼容 API |
| `anthropic.rs` | 633 | Anthropic Messages API 原生实现 |
| `chain.rs` | 513 | ProviderChain: 多实例管理、健康检查、故障切换 |
| `pipeline.rs` | 483 | 装饰器链: Retry -> CircuitBreaker -> CostGuard -> Cache -> SmartRouter -> UsageRecorder |
| `smart_router.rs` | 451 | 复杂度分析器 + 跨 Provider 智能路由 (V1/V2) |
| `retry.rs` | 329 | 8 类错误分类 + 语义路由策略 + 指数退避 |
| `response_cache.rs` | 241 | LRU + SHA-256 缓存 (TTL 过期) |
| `metering_provider.rs` | 235 | Token 使用计量装饰器 |
| `usage_recorder.rs` | 196 | 按模型统计 token 使用 |
| `config.rs` | 79 | ProviderConfig, ProviderChainConfig |
| `mod.rs` | 42 | `create_provider()` 工厂函数 |

**合计约 4,155 行**，组成完整的 Provider 子系统。

### 1.2 核心设计特征

**Provider trait 定义** (`traits.rs:14-34`):
```rust
pub trait Provider: Send + Sync {
    fn id(&self) -> &str;
    fn metering(&self) -> Option<Arc<Metering>> { None }
    async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse>;
    async fn stream(&self, request: CompletionRequest) -> Result<CompletionStream>;
    async fn embed(&self, _texts: &[String]) -> Result<Vec<Vec<f32>>> { /* default error */ }
}
```

**关键能力**:
1. **base_url 通用性**: `OpenAIProvider::with_base_url()` 可接入任何 OpenAI-compatible API（DeepSeek, Groq, OpenRouter, Ollama, vLLM, LMStudio 等），无需额外代码
2. **装饰器管线**: `ProviderPipelineBuilder` 支持任意组合 Retry/CircuitBreaker/CostGuard/Cache/SmartRouter/UsageRecorder
3. **Thinking/Reasoning 支持**: OpenAI provider 同时支持 `reasoning_content`, `thinking`, `reasoning` 三个字段名 + content array 中的 thinking block（覆盖 DeepSeek, OpenRouter Claude, MiniMax, SiliconFlow 等）
4. **多模态**: 双 Provider 均支持 Image（base64/URL）和 Document 内容块
5. **跨 Provider 智能路由**: SmartRouter V2 可按复杂度分层派发到不同 Provider 实例
6. **8 类错误语义分类**: RateLimit/Overloaded/Timeout/ServiceError/BillingError/AuthError/ContextOverflow/Unknown，每类对应不同路由策略

---

## 二、竞品 Provider 层分析

### 2.1 Goose (Block)

**文件数量**: ~45 个 provider 相关文件
**Provider trait** (`base.rs`): 比 octo 复杂得多

```
Provider trait 方法:
- get_name(), stream(), complete(), complete_fast()
- get_model_config(), retry_config()
- fetch_supported_models(), fetch_recommended_models()
- map_to_canonical_model()
- supports_embeddings(), create_embeddings()
- supports_cache_control()
- generate_session_name()
- configure_oauth(), permission_routing()
- handle_permission_confirmation()
- as_lead_worker(), get_active_model_name()
```

**独立 Provider struct 列表** (每个独立文件):
anthropic, openai, azure, bedrock, databricks, gcpvertexai, google (Gemini), ollama, openrouter, snowflake, litellm, sagemaker_tgi, venice, xai, chatgpt_codex, codex, cursor_agent, gemini_cli, githubcopilot, avian, tetrate, claude_code, lead_worker

**专有能力**:
- `CanonicalModelRegistry`: 跨 provider 标准化模型名映射 (YAML 注册表)
- `ModelConfig` + `ModelInfo`: 包含 context_limit, token cost, cache_control 支持
- `LeadWorkerProvider`: 双模型 lead/worker 策略（lead 做规划，worker 执行）
- `DeclarativeProviderConfig`: 用户可通过 YAML 声明式添加自定义 provider
- `ProviderRegistry` + `auto_detect`: 自动检测可用 provider
- `ToolShim`: 为不支持 tool_call 的模型提供 XML shim
- `usage_estimator`: 当 API 不返回 token 用量时进行估算
- OAuth device flow 支持
- `ProviderError` 类型化错误 (10 种，含 `CreditsExhausted` + `retry_delay`)
- 重试带 jitter（防止惊群效应）
- OpenAI Responses API (GPT-5 Codex 系列) 自动检测

**局限**: 无 CircuitBreaker、无 CostGuard、无 ResponseCache、无智能路由

### 2.2 OpenFang (Agent OS)

**文件数量**: 6 个 driver 文件
**Driver trait** (`LlmDriver`): `complete()` + `stream(tx: mpsc::Sender)`

**Provider 接入策略**:
- Anthropic: 独立 `AnthropicDriver` (原生 Messages API)
- Gemini: 独立 `GeminiDriver` (原生 Gemini API)
- Claude Code: 独立 `ClaudeCodeDriver` (subprocess)
- GitHub Copilot: 独立 `CopilotDriver` (token exchange)
- **其余 30+ 个 provider**: 全部用 `OpenAIDriver` + 不同 base_url

这是与 octo-sandbox **完全相同的策略**: OpenAI-compatible driver + base_url 覆盖绝大多数 provider。区别在于 OpenFang 预置了 30+ 个 `provider_defaults()` 映射（provider name -> base_url + env_var），让用户输入 `provider: "groq"` 就自动解析为 `https://api.groq.com/openai/v1`。

**专有能力**:
- `Zeroizing<String>` API key 内存安全擦除
- `detect_available_provider()`: 自动扫描环境变量发现可用 provider
- 30+ 预置 provider 名称映射
- Thinking/Reasoning 流式解析（`reasoning_content` 字段）
- `extra_headers` 支持（Copilot IDE 认证）

**局限**: 无 ProviderChain/failover、无 CircuitBreaker、无 CostGuard、无 ResponseCache、无智能路由、无 embed() 支持

### 2.3 Ironclaw (NEAR AI)

未找到独立 provider 模块——使用外部 LLM SDK crate。

### 2.4 Moltis (多渠道平台)

使用 `service-traits` crate 定义 LLM service trait，具体 provider 实现在外部 crate。

### 2.5 Zeroclaw (CLI + IoT)

**Provider 架构**: 基于 `robot-kit` crate 的 trait 体系
- `traits.rs` 定义 LLM trait
- 支持 Gemini 原生 API
- Circuit breaker 集成测试存在

### 2.6 AutoAgents (泛型框架)

**Provider 架构**: `autoagents-llm` crate
- `providers/openai_compatible.rs`: OpenAI 兼容基座
- `backends/`: 独立 backend 实现 (deepseek, google, minimax, ollama, openrouter, phind)
- `pipeline/`: Provider pipeline 支持
- `evaluator/`: LLM 评估器（并行评测）

### 2.7 LocalGPT

**Provider 架构**: `agent/providers.rs` 单文件
- OpenAI provider 支持
- failover 支持 (`agent/failover.rs`)

### 2.8 Pi Agent Rust

大型单体 crate，provider 相关测试存在但核心代码结构不同。

---

## 三、8 维度深度对比

### 3.1 Provider Trait 抽象设计

| 项目 | Trait 方法数 | 关键设计决策 |
|------|------------|------------|
| **octo-sandbox** | 5 (id, metering, complete, stream, embed) | 极简接口，装饰器模式解耦横切关注点 |
| **goose** | 15+ | 胖接口，retry/session_name/oauth/permission 混入 trait |
| **openfang** | 2 (complete, stream) | 最简，但 stream 用 mpsc channel 而非 Stream trait |

**评估**: octo-sandbox 的 trait 设计最优——符合接口隔离原则(ISP)，横切关注点通过 Pipeline 装饰器注入，不污染核心接口。goose 的 Provider trait 过胖，但携带了更多业务能力。

**评分**: octo-sandbox 9/10, goose 7/10, openfang 7/10

### 3.2 OpenAI-Compatible 通用性

| 项目 | 方案 | 实际接入能力 |
|------|------|------------|
| **octo-sandbox** | `OpenAIProvider::with_base_url(key, url)` | 配置 base_url 即可接入 Groq/DeepSeek/OpenRouter/Ollama/vLLM/LMStudio 等 |
| **goose** | `OpenAiProvider` + `OPENAI_HOST`/`OPENAI_BASE_PATH` + `DeclarativeProviderConfig` + 20+ 独立 struct | 更多显式支持，但大量代码冗余 |
| **openfang** | `OpenAIDriver::new(key, base_url)` + 30 个 `provider_defaults()` | 与 octo 相同策略，但预置映射更多 |

**关键洞察**: octo-sandbox 的 `with_base_url()` 和 openfang 的 `OpenAIDriver::new(key, base_url)` 功能完全等价。goose 为每个 provider 写独立 struct 主要是为了支持 provider-specific 配置（OPENAI_ORGANIZATION, Azure AD token, GCP auth, Bedrock SigV4 等），但纯 OpenAI-compatible 的 provider（Groq, DeepSeek, Together 等）通过 base_url 就能解决。

**octo 的真正差距不在 struct 数量，而在**:
1. 缺少预置的 provider name -> base_url 映射表（用户需手动填 URL）
2. 缺少 `auto_detect()` 自动发现可用 provider
3. 缺少 Azure AD / GCP OAuth / Bedrock SigV4 等非标认证

**评分**: octo-sandbox 7.5/10, goose 9/10, openfang 8/10

### 3.3 Failover/重试机制深度

| 项目 | 重试 | Circuit Breaker | Failover | 错误分类 | Jitter |
|------|------|-----------------|----------|---------|--------|
| **octo-sandbox** | RetryPolicy (指数退避) | CircuitBreakerProvider (3态) | ProviderChain (优先级+健康检查) | 8 类 + routing_strategy | 无 |
| **goose** | RetryConfig (指数退避) | 无 | 无 (单 provider) | ProviderError (10 种) | 有 (0.8-1.2x) |
| **openfang** | 硬编码 3 次重试 | 无 | 无 | LlmError (结构化) | 无 |

**评估**: octo-sandbox 在容错层面**远超所有竞品**:
- **唯一实现 Circuit Breaker** (Closed->Open->HalfOpen 三态机)
- **唯一实现 ProviderChain** (多实例优先级排序 + 健康检查 + 自动故障切换)
- **唯一有 CostGuard** (成本预算控制)
- **唯一有 ResponseCache** (LRU + TTL 去重)
- 错误 routing_strategy 直接映射到恢复动作 (Retry/Failover/CompactAndRetry/Fail)

**评分**: octo-sandbox 9.5/10, goose 6/10, openfang 4/10

### 3.4 Streaming 实现质量

| 项目 | SSE 解析方式 | 多事件/chunk 处理 | 边界处理 |
|------|------------|-----------------|---------|
| **octo-sandbox** | 手写 SSE parser, `VecDeque<Result<StreamEvent>>` pending 队列 | 正确：单 chunk 含多 event 时全部入队 | `\n\n` 和 `\r\n\r\n` 双边界 |
| **goose** | `LinesCodec` + 逐行解析 | 依赖 tokio codec 分帧 | 框架处理 |
| **openfang** | 手写 SSE parser (与 octo 类似) | 正确 | `\n\n` 边界 |

**评估**: octo-sandbox 的 SSE 实现质量优秀——pending VecDeque 确保多事件 chunk 不丢数据，双边界处理兼容更多 API 服务。goose 使用 LinesCodec 框架更稳健但灵活性略低。

**工具调用流式处理**: 三者都正确实现了 tool_call 的增量累积模式（ToolCallAccum/ToolBlockAccum）。

**评分**: octo-sandbox 8.5/10, goose 8/10, openfang 7.5/10

### 3.5 Thinking/Reasoning 支持

| 项目 | Anthropic thinking | OpenAI reasoning_content | 多字段兼容 | Content array blocks |
|------|-------------------|------------------------|-----------|---------------------|
| **octo-sandbox** | content_block_start type=thinking + thinking_delta | reasoning_content 字段 | thinking, reasoning, reasoning_content 三字段 | delta.content array 中 type=thinking/reasoning |
| **goose** | Anthropic format 中 thinking block | 不明确 | 待确认 | 待确认 |
| **openfang** | Anthropic: thinking block (complete + stream) | OpenAI: reasoning_content 字段 | reasoning_content 单字段 | 无 |

**评估**: octo-sandbox 的 Thinking 支持是**最全面**的——同时兼容 3 种字段名 (`reasoning_content`, `thinking`, `reasoning`) + content array blocks，覆盖 OpenAI o-系列、DeepSeek、MiniMax、SiliconFlow、OpenRouter Claude 等不同格式。还有 `create_thinking_config()` 为 Claude 模型自动启用 reasoning。

**评分**: octo-sandbox 9/10, goose 7/10, openfang 7/10

### 3.6 多模态支持（图片输入等）

| 项目 | Image base64 | Image URL | Document | 格式转换 |
|------|------------|----------|---------|---------|
| **octo-sandbox** | Anthropic + OpenAI 双支持 | OpenAI image_url | Anthropic document type + OpenAI fallback (base64 image_url) | 自动适配 |
| **goose** | ImageFormat::OpenAi / ImageFormat::Anthropic | 支持 | 无 Document | 通过 ImageFormat enum 切换 |
| **openfang** | Anthropic: base64 image | 无 URL 支持 | 无 | 仅 base64 |

**评估**: octo-sandbox 的多模态支持最完整——双 provider 都支持 Image，且 Anthropic provider 有原生 Document 类型支持（PDF 等）。OpenAI provider 将 Document 回退为 base64 image_url 也是合理的降级策略。

**评分**: octo-sandbox 8.5/10, goose 8/10, openfang 5/10

### 3.7 Token 计数精度

| 项目 | 计数来源 | 估算回退 | 按模型统计 | 成本计算 |
|------|---------|---------|-----------|---------|
| **octo-sandbox** | API 返回的 usage 字段 | MeteringProvider: chars/4 估算 | UsageRecorderProvider: by_model HashMap | CostGuardProvider: 固定费率 ($0.003/1K in, $0.015/1K out) |
| **goose** | API 返回的 usage 字段 | usage_estimator 模块 | ProviderUsage by model | ModelInfo: per-model token cost |
| **openfang** | API 返回的 usage 字段 | 无 | 无 | 外部 budget 系统 |

**评估**: goose 在 token 计数方面略优——`usage_estimator` 可在 API 不返回用量时进行估算，`ModelInfo.with_cost()` 支持 per-model 精确定价。octo-sandbox 的 CostGuard 使用固定费率是个简化假设，不够精确。

**评分**: octo-sandbox 7/10, goose 8.5/10, openfang 5/10

### 3.8 实际接入能力差异

**通过 base_url 配置就能接入的 provider** (octo = goose = openfang):
- OpenAI, Groq, DeepSeek, OpenRouter, Together, Mistral, Fireworks
- Ollama, vLLM, LM Studio (本地推理)
- Cerebras, SambaNova, Perplexity, Cohere, xAI, Replicate
- Moonshot(Kimi), Qwen(通义), MiniMax, Zhipu(智谱), Volcengine(火山引擎), Qianfan(千帆)

**需要独立认证逻辑的 provider** (octo 目前无法通过 base_url 解决):

| Provider | 认证方式 | goose 支持 | openfang 支持 | octo 支持 |
|----------|---------|-----------|--------------|----------|
| Azure OpenAI | Azure AD token + custom endpoint | azureauth.rs | 无 | 无 |
| AWS Bedrock | SigV4 签名 | bedrock.rs | 无 | 无 |
| GCP Vertex AI | GCP OAuth + project/location | gcpvertexai.rs + gcpauth.rs | 无 | 无 |
| Google Gemini | 非 OpenAI 格式 (generateContent API) | google.rs + formats/google.rs | gemini.rs | 无 |
| GitHub Copilot | PAT -> Copilot token exchange | githubcopilot.rs | copilot.rs | 无 |
| Snowflake Cortex | Snowflake session token | snowflake.rs | 无 | 无 |
| Databricks | Databricks auth | databricks.rs | 无 | 无 |

---

## 四、综合评分矩阵

| 维度 | 权重 | octo-sandbox | goose | openfang |
|------|------|-------------|-------|----------|
| Provider Trait 设计 | 10% | 9.0 | 7.0 | 7.0 |
| OpenAI-Compatible 通用性 | 15% | 7.5 | 9.0 | 8.0 |
| Failover/重试机制 | 20% | **9.5** | 6.0 | 4.0 |
| Streaming 质量 | 15% | 8.5 | 8.0 | 7.5 |
| Thinking/Reasoning | 10% | **9.0** | 7.0 | 7.0 |
| 多模态 | 10% | **8.5** | 8.0 | 5.0 |
| Token 计数精度 | 10% | 7.0 | **8.5** | 5.0 |
| 实际接入能力 | 10% | 7.0 | **9.5** | 7.5 |
| **加权总分** | 100% | **8.43** | **7.73** | **6.18** |

---

## 五、真差距分析（无法通过 base_url 解决）

### 差距 1: 非 OpenAI 格式 API（Gemini）
**影响**: 中等。Google Gemini 使用 `generateContent` API，格式完全不同于 OpenAI chat/completions。
**建议**: 添加 `GeminiProvider`，约 400-500 行。或者用 OpenRouter/Vertex AI 的 OpenAI-compatible 端点间接接入。

### 差距 2: 企业云认证（Azure AD / GCP OAuth / AWS SigV4）
**影响**: 高（企业客户需求）。这些认证方式无法通过简单的 Bearer token + base_url 解决。
**建议**:
- Azure: 添加 `AzureAuthProvider` 装饰器 (token refresh + custom headers)
- GCP: 添加 `GcpAuthProvider` 装饰器 (OAuth2 + project/location injection)
- Bedrock: 添加 `BedrockProvider` (SigV4 请求签名)
- 优先级建议: Azure > GCP > Bedrock

### 差距 3: Provider 名称自动解析
**影响**: 低（UX 优化）。用户当前需手动填写 base_url。
**建议**: 在 `config.rs` 或 `mod.rs` 中添加 `provider_defaults()` 映射表（参考 openfang 模式），让 `provider: "groq"` 自动解析为正确的 base_url 和 env var。约 100 行代码。

### 差距 4: Per-Model 精确成本计算
**影响**: 低。CostGuardProvider 使用固定费率 ($0.003/$0.015)，不区分模型。
**建议**: 添加 `ModelPricing` 结构体或 YAML 配置，按模型名查找输入/输出价格。

### 差距 5: 重试 Jitter
**影响**: 极低。octo 的 RetryPolicy 无 jitter，高并发下可能产生惊群效应。
**建议**: 在 `RetryPolicy::delay_for()` 中添加 `0.8 + rand() * 0.4` 因子。5 行改动。

### 差距 6: ToolShim（为不支持 tool_call 的模型提供 XML 兼容层）
**影响**: 中低。部分小模型和本地模型不支持 function calling。
**建议**: 参考 goose 的 `toolshim.rs`，将 tool 定义序列化为 XML 注入 system prompt，从回复中正则提取 tool calls。

---

## 六、octo-sandbox 的独有优势（竞品均无）

| 能力 | 描述 | 竞品状态 |
|------|------|---------|
| **装饰器管线** | `ProviderPipelineBuilder` 可任意组合 6 种装饰器 | 无竞品有此模式 |
| **Circuit Breaker** | 3 态状态机 (Closed/Open/HalfOpen) | 仅 zeroclaw 有测试但实现不详 |
| **CostGuard** | 运行时成本预算控制（微美元精度原子操作） | 无 |
| **ResponseCache** | SHA-256 键 + LRU + TTL | 无 |
| **SmartRouter V2** | 跨 Provider 复杂度分层路由 | 无 |
| **错误语义路由** | 8 类错误 -> 4 种恢复策略自动映射 | goose 有类型化错误但无路由策略 |
| **Thinking 多格式兼容** | 3 字段名 + content array blocks | 无竞品覆盖如此全面 |
| **Embed 统一接口** | Provider trait 内置 embed()，OpenAI 有完整实现 | goose 有但是独立 trait |

---

## 七、改进路线图（按优先级排序）

### P0: 快速改进（< 1 天工作量）
1. **添加 provider_defaults 映射表** — 让 `provider: "groq"` 自动工作 (~100 行)
2. **重试添加 Jitter** — `delay_for()` 加随机因子 (~5 行)
3. **CostGuard per-model 定价** — 从固定费率改为模型查找表 (~50 行)

### P1: 企业功能（1-3 天）
4. **Azure OpenAI 认证装饰器** — AD token refresh + endpoint 格式 (~200 行)
5. **GeminiProvider** — generateContent API 原生支持 (~400 行)

### P2: 完善功能（3-7 天）
6. **GCP Vertex AI 认证** — OAuth2 service account (~300 行)
7. **AWS Bedrock SigV4** — 请求签名 (~400 行)
8. **ToolShim** — XML tool 兼容层 (~200 行)
9. **auto_detect** — 扫描环境变量自动发现可用 provider (~50 行)

---

## 八、总结

**octo-sandbox 的 Provider 层在架构深度上是所有竞品中最优秀的**。装饰器管线、Circuit Breaker、CostGuard、ResponseCache、SmartRouter 这套组合拳是其他任何项目都没有的。这不是 struct 数量的竞争，而是**工程成熟度**的差距。

goose 的优势在于**广度**——支持的 provider 数量最多，企业云认证最全，UI/UX 打磨最好。但它的 Provider trait 过胖，缺少容错基础设施。

openfang 验证了一个关键事实：**30+ provider 只用 1 个 OpenAIDriver + provider_defaults 映射就能覆盖**，与 octo-sandbox 的 `with_base_url()` 策略完全吻合。

**结论**: octo-sandbox 不需要为每个 provider 写独立 struct。真正需要补的是：(1) provider name 自动解析映射表，(2) Gemini 原生 API，(3) 企业云认证（Azure/GCP/Bedrock），(4) 重试 jitter。这些是**确实无法通过 base_url 配置解决**的真差距。

---
---

# Context Engineering 层代码级竞品分析

> 基于 octo-sandbox `crates/octo-engine/src/context/` 及 3th-party/harnesses/rust-projects/ 下 6 个竞品的**实际代码**深入分析。
> 分析日期：2026-03-12
> 分析维度：System Prompt 构建、Token 估算精度、Context 压缩/降级策略、工具结果截断、消息保护标记、Bootstrap 文件发现、多模态内容处理

---

## 九、octo-sandbox Context Engineering 全景

### 9.1 文件清单与职责

| 文件 | 行数 | 职责 |
|------|------|------|
| `system_prompt.rs` | ~180 | Zone A: SystemPromptBuilder (AgentManifest 驱动) |
| `builder.rs` | ~250 | Zone A+B: Bootstrap 发现 + 动态上下文构建 |
| `budget.rs` | ~211 | ContextBudgetManager: 6 级降级判定 |
| `pruner.rs` | ~300 | ContextPruner: 6 阶段渐进式裁剪 |
| `manager.rs` | ~200 | ContextManager 统一门面 + TokenCounter trait |
| `token_counter.rs` | ~30 | CjkAwareCounter 类型别名 |
| `tiktoken_counter.rs` | ~60 | TiktokenCounter (cl100k_base, feature-gated) |
| `auto_compact.rs` | ~80 | 启发式自动摘要占位符 |
| `flush.rs` | ~100 | MemoryFlusher: LLM 事实提取 |
| `fork.rs` | ~80 | ContextFork: 隔离式 Skill 执行 |
| `observation_masker.rs` | ~120 | ObservationMasker: 轮次遮蔽 |
| `mod.rs` | ~40 | 模块导出 + default_token_counter() |

**合计约 1,651 行**，组成完整的 Context Engineering 子系统。

### 9.2 架构概览

```
Zone A (Static):  SystemPromptBuilder
                  ├── AgentManifest (role/goal/backstory)
                  ├── Bootstrap Files (AGENTS.md, CLAUDE.md, SOUL.md...)
                  ├── Skill L1 Index + L2 Active Skill
                  └── Core Instructions

Zone B (Dynamic): builder.rs::build_dynamic_context()
                  ├── 当前日期时间
                  ├── Session 状态
                  └── WorkingMemory XML 块

Zone C (History): ContextBudgetManager + ContextPruner
                  ├── 6 级降级 (None → SoftTrim → AutoCompaction → OverflowCompaction → ToolResultTruncation → FinalError)
                  ├── Dual-Track 估算 (API actual + chars/4 增量)
                  ├── SKILL_PROTECTED_MARKER 保护
                  └── UTF-8 安全截断

Cross-cutting:    ContextFork (隔离执行)
                  ObservationMasker (工具结果遮蔽)
                  MemoryFlusher (压缩前事实抢救)
                  TokenCounter trait (EstimateCounter / TiktokenCounter)
```

---

## 十、竞品 Context Engineering 分析

### 10.1 Goose — Token 计数精度领先

**关键文件**: `crates/goose/src/token_counter.rs` (308 行)

**Token 计数架构**:
- 使用 `tiktoken_rs::o200k_base()` — 最新的 OpenAI 编码 (GPT-4o/o-系列)
- `DashMap<u64, usize>` 并发缓存 + AHash 哈希，MAX_TOKEN_CACHE_SIZE: 10,000
- **结构化工具 schema token 建模**: 每个 tool 定义的 token 消耗不是简单按字符估算，而是精确建模 JSON Schema 结构:
  - `FUNC_INIT_TOKEN_COUNT: 7` — 函数声明开销
  - `PROP_INIT_TOKEN_COUNT: 3` — 属性初始化
  - `PROP_KEY_TOKEN_COUNT: 3` — 属性 key
  - `ENUM_INIT_TOKEN_COUNT: -3` — 枚举初始化（节省 token）
  - `ENUM_ITEM_TOKEN_COUNT: 3` — 枚举值
  - `FUNC_END_TOKEN_COUNT: 12` — 函数结束
- `count_chat_tokens()` 跳过 `!message.metadata.agent_visible` 的消息
- `count_everything()` 包含 resources

**Prompt 管理**: `agents/prompt_manager.rs`
- `PromptManager` 带 `system_prompt_override`, `system_prompt_extras` (IndexMap)
- `SystemPromptBuilder` 包含 extensions_info, frontend_instructions, hints, code_execution_mode
- 模板驱动，支持运行时扩展

**对比 octo-sandbox**: goose 在 token 计数精度上明显领先——使用最新 o200k_base 编码（octo 用 cl100k_base），且对 tool schema 有精确的结构化建模。DashMap 并发缓存在高吞吐场景下性能更好。但 goose 没有 context 压缩/降级/pruning 机制。

### 10.2 OpenFang — 最成熟的 Prompt 构建 + LLM 压缩

**关键文件**:
- `crates/openfang-runtime/src/prompt_builder.rs` (944 行) — 14 段有序 prompt 构建
- `crates/openfang-runtime/src/context_budget.rs` (323 行) — 动态 tool 预算
- `crates/openfang-runtime/src/context_overflow.rs` (248 行) — 4 阶段溢出恢复
- `crates/openfang-runtime/src/compactor.rs` (~1500 行) — LLM 3 阶段摘要压缩

**Prompt Builder 亮点**:
1. **14 段有序构建**: 每段有明确职责，subagent 模式跳过 8+ 段（减少不相关上下文）
2. **`PromptContext` 结构体 25+ 字段**: soul_md, identity_md, peer_agents, channel_type, canonical_context, heartbeat_md 等
3. **Prompt Cache 优化**: `build_canonical_context_message()` 将动态内容（日期、session 状态）移出 system_prompt 到 user message，让 system_prompt 保持稳定以利用 Anthropic prompt cache
4. **Channel-Aware 格式化**: `build_channel_section()` 按渠道类型设置 char 限制（telegram 4096, discord 2000, irc 512）
5. **Peer Agent 感知**: `build_peer_agents_section()` 让 agent 知道同伴存在
6. **Code Block 剥离**: `strip_code_blocks()` 防止 LLM 复制示例代码
7. **工具分类显示**: `tool_category()` + `tool_hint()` 按类别分组展示工具

**Context Budget 亮点**:
- `tool_chars_per_token: 2.0`（工具文本按 2 char/token 计算，比一般文本更保守）
- `general_chars_per_token: 4.0`（一般文本按 4 char/token）
- `per_result_cap()`: 单次工具结果上限 = 30% context_window * chars_per_token
- `single_result_max()`: 50% context_window
- `total_tool_headroom_chars()`: 75% context_window
- Layer 1: `truncate_tool_result_dynamic()` — 按换行符对齐截断
- Layer 2: `apply_context_guard()` — 扫描所有工具结果，超 75% 时压缩最旧的到 2K

**Context Overflow 亮点** (4 阶段):
- Stage 1 (70-90%): keep last 10 messages
- Stage 2 (>90%): keep last 4 + summary marker message
- Stage 3: 截断所有工具结果到 2000 chars
- Stage 4: FinalError 建议 `/reset` 或 `/compact`

**Compactor 亮点** (LLM 摘要):
- 3 阶段策略: 全量单次摘要 -> 自适应分块 + 合并 -> 最小回退
- `CompactionConfig` 含 threshold, keep_recent, max_summary_tokens, base_chunk_ratio, safety_margin
- `estimate_token_count()` 用 chars/4 + per-message overhead

**对比 octo-sandbox**: OpenFang 的 prompt 构建是所有竞品中最成熟的。prompt cache 优化（动态内容移出 system prompt）是 octo 完全缺失的能力。LLM 3 阶段摘要压缩也远比 octo 的占位符 auto_compact 成熟。但 OpenFang 没有 ContextFork、ObservationMasker、MemoryFlusher 这些 octo 独有的机制。

### 10.3 Ironclaw — 非 LLM Context 管理

**关键文件**: `src/context/manager.rs`
- `HashMap<Uuid, JobContext>` 带 `RwLock`
- 状态机: Pending -> InProgress -> Completed/Failed/Stuck
- **这是 Job/Task Context，不是 LLM Context Management**
- 无 token 计数、无 prompt 构建、无 context 压缩

### 10.4 Zeroclaw — 无专门模块

**关键文件**: `src/memory/mod.rs`
- Memory backends (sqlite, markdown, lucid, cortex, qdrant)
- 无专门的 context engineering 模块
- 无 token 计数或 prompt 构建

### 10.5 Moltis — 外部依赖

**关键文件**: `crates/chat/src/lib.rs`
- 使用 `build_system_prompt_with_session_runtime()` 来自 `moltis_agents` 外部 crate
- 实际 prompt 构建逻辑不在本地代码中

### 10.6 Pi Agent Rust — 无 Context Engineering

**关键文件**: `src/session.rs`
- Session tree 结构 + JSONL 持久化
- 支持 branching
- 无 context 压缩、无 token 计数

---

## 十一、7 维度深度对比

### 11.1 System Prompt 构建

| 项目 | 架构 | 段数 | Subagent 支持 | Prompt Cache 优化 | Bootstrap 文件 |
|------|------|------|-------------|-----------------|--------------|
| **octo-sandbox** | Zone A/B/C 三区 | ~6 段 (role, goal, backstory, bootstrap, skills, core_instructions) | 无 | 无 | 6 种 (AGENTS.md, CLAUDE.md, SOUL.md, TOOLS.md, IDENTITY.md, BOOTSTRAP.md) |
| **openfang** | 14 段有序 | 14 段 | subagent 模式跳过 8+ 段 | `build_canonical_context_message()` 移动态内容到 user msg | soul_md, identity_md + canonical_context |
| **goose** | PromptManager + SystemPromptBuilder | ~5 段 (base, extensions, frontend, hints, code_execution) | 无 | 无 | 无 |

**评分**: octo-sandbox 7/10, openfang **9.5/10**, goose 6/10

**差距分析**:
- **OpenFang 的 Prompt Cache 优化是关键差距**: 将日期、session 状态等动态内容从 system prompt 移到 user message，使 system prompt 保持稳定，显著提升 Anthropic prompt cache 命中率。octo 的 `build_dynamic_context()` 目前嵌入 system prompt 中，每次都会变化，破坏 cache。
- **Subagent 模式**: OpenFang 在 subagent 场景下跳过 8+ 不相关段（peer agents, channel type, heartbeat 等），减少 token 浪费。octo 无此优化。
- **Channel-Aware 格式化**: OpenFang 按渠道类型（telegram/discord/irc）设置输出 char 限制，octo 无此概念。

### 11.2 Token 估算精度

| 项目 | 主要方法 | 编码模型 | CJK 感知 | 结构化 Schema 建模 | 缓存 | Dual-Track |
|------|---------|---------|---------|-----------------|------|-----------|
| **octo-sandbox** | EstimateCounter (CJK-aware) + TiktokenCounter (feature-gated) | cl100k_base | ASCII 0.25, CJK 0.67 tokens/char | 无 | 无 | API actual + chars/4 增量 |
| **goose** | tiktoken + DashMap cache | **o200k_base** (最新) | 无专门处理 | **有**: FUNC_INIT(7), PROP_KEY(3), ENUM_INIT(-3), FUNC_END(12) | DashMap 10K entries | 无 |
| **openfang** | chars/4 + per-message overhead | 无 BPE | 无 | tool_chars_per_token: 2.0 vs general: 4.0 | 无 | 无 |

**评分**: octo-sandbox 7.5/10, goose **9/10**, openfang 5/10

**差距分析**:
- **Goose 的 o200k_base**: 这是 GPT-4o/o-系列使用的最新编码，比 cl100k_base 更准确。octo 的 TiktokenCounter 使用 cl100k_base 对新模型可能不够精确。
- **Goose 的工具 Schema 结构化建模**: 精确到 JSON Schema 每个结构元素的 token 成本（函数头 7 token、属性 key 3 token 等），比 chars/4 估算准确得多。octo 的 `estimate_tool_specs_tokens()` 只是简单的 chars/4。
- **Goose 的 DashMap 并发缓存**: 在高频 token 计数场景下避免重复 BPE 编码，性能优势明显。
- **octo 的 CJK 感知是独有优势**: ASCII 0.25 vs CJK 0.67 tokens/char 的区分处理，对中日韩文本估算更准确。goose 和 openfang 都没有此优化。
- **octo 的 Dual-Track 是独有优势**: 使用 API 实际返回的 input_tokens 作为基线，仅对新增消息做 chars/4 估算，比纯估算精确得多。

### 11.3 Context 压缩/降级策略

| 项目 | 降级级数 | 阈值 | LLM 摘要 | Memory Flush | 观察遮蔽 |
|------|---------|------|---------|-------------|---------|
| **octo-sandbox** | 6 级 (None/SoftTrim/AutoCompaction/OverflowCompaction/ToolResultTruncation/FinalError) | 60%/70%/90% | 占位符 (auto_compact.rs) | MemoryFlusher (LLM fact extraction) | ObservationMasker (keep_recent_turns: 3) |
| **openfang** | 4 级 (Stage 1-4) | 70%/90% | **3 阶段 LLM 摘要** (full -> chunked+merge -> minimal) | 无 | 无 |
| **goose** | 0 级 (无降级) | 无 | 无 | 无 | 无 |

**评分**: octo-sandbox **8.5/10**, openfang 8/10, goose 2/10

**差距分析**:
- **OpenFang 的 LLM 3 阶段摘要压缩远超 octo 的占位符**: octo 的 `auto_compact.rs` 只生成 `[Compacted: {title}... ({chars} chars -> {tokens_saved} tokens saved)]` 占位文本，丢失了所有语义信息。OpenFang 的 compactor 用 LLM 生成真正的摘要，保留关键信息。
- **octo 的 MemoryFlusher 是独有优势**: 压缩前用 LLM 提取事实写入 WorkingMemory，确保重要信息不随压缩丢失。OpenFang 没有此机制。
- **octo 的 ObservationMasker 是独有优势**: 遮蔽非最近 3 轮的工具结果，非破坏性（返回新 Vec），有效减少 token 占用同时保留对话结构。OpenFang 没有此机制。
- **octo 的 SoftTrim (60-70%) 更细粒度**: 在 AutoCompaction 之前先做轻度的头尾裁剪，OpenFang 直接从 70% 跳到 keep-last-10。
- **octo 的 6 级 vs OpenFang 4 级**: 更多梯度意味着更平滑的降级体验。

### 11.4 工具结果截断

| 项目 | 截断策略 | 动态上限 | 换行对齐 | UTF-8 安全 |
|------|---------|---------|---------|----------|
| **octo-sandbox** | SoftTrim: 1500 head + 500 tail; ToolResultTruncation: 8000 chars | 固定常量 | 无 | `char_indices()` |
| **openfang** | Layer 1: per_result_cap = 30% context_window; Layer 2: apply_context_guard 压缩最旧到 2K; Stage 3: 全部截断到 2000 | **动态**: 按 context window 比例 | **有**: 按换行符对齐 | `cap_str()` UTF-8 安全 |
| **goose** | 无显式截断 | 无 | 无 | 无 |

**评分**: octo-sandbox 7/10, openfang **9/10**, goose 2/10

**差距分析**:
- **OpenFang 的动态工具预算是关键差距**: `per_result_cap()` = 30% * context_window * chars_per_token，根据模型 context window 大小动态调整截断上限。200K context 模型允许更大的工具结果，8K 模型自动收紧。octo 使用固定常量 (1500/500/8000)，不适应不同模型。
- **OpenFang 的 tool_chars_per_token: 2.0**: 对工具文本使用比一般文本更保守的估算（2 char/token vs 4 char/token），因为工具输出往往包含更多结构化数据/代码，token 密度更高。octo 统一使用 4 char/token。
- **OpenFang 的换行对齐截断**: `truncate_tool_result_dynamic()` 在换行符位置截断，保持输出可读性。octo 的 SoftTrim 直接按字符位置切割，可能切断行中间。
- **OpenFang 的 apply_context_guard**: 全局扫描所有工具结果，当总占用超 75% 时优先压缩最旧的结果到 2K，保留最新的。octo 没有全局工具结果感知。

### 11.5 消息重要性标记/保护

| 项目 | 保护机制 | 标记方式 | 保护范围 | Tool Chain 安全 |
|------|---------|---------|---------|---------------|
| **octo-sandbox** | SKILL_PROTECTED_MARKER `[SKILL:ALWAYS]` | 消息文本前缀 | `always: true` 的 skill 注入消息 | `find_compaction_boundary()` 不在 tool chain 中间切割 |
| **openfang** | summary marker message | 系统消息注入 | 压缩摘要 | 无显式保护 |
| **goose** | `metadata.agent_visible` | bool 标记 | 非 agent_visible 消息跳过计数 | 无 |

**评分**: octo-sandbox **8.5/10**, openfang 5/10, goose 4/10

**差距分析**:
- **octo 的 SKILL_PROTECTED_MARKER 是最完善的保护机制**: 标记为 `always: true` 的 skill 消息永远不会被 pruner 删除，即使在最激进的 OverflowCompaction 阶段。
- **octo 的 Tool Chain 安全是独有能力**: `find_compaction_boundary()` 确保不在 ToolUse-ToolResult 配对中间切割，`find_protection_boundary()` 使用轮次计数（跳过纯 ToolResult 消息）。其他竞品都没有此保护。
- **goose 的 `agent_visible` 是不同场景**: 用于跳过不需要显示给 agent 的消息（如元数据），不是压缩保护。

### 11.6 Bootstrap 文件自动发现

| 项目 | 发现策略 | 文件列表 | 大小限制 | 截断策略 |
|------|---------|---------|---------|---------|
| **octo-sandbox** | 固定文件名列表，从工作目录搜索 | AGENTS.md, CLAUDE.md, SOUL.md, TOOLS.md, IDENTITY.md, BOOTSTRAP.md | 单文件 20K chars, 总量 50K chars, 最多 10 文件 | 70% head + 20% tail |
| **openfang** | 配置驱动 + 固定字段 | soul_md, identity_md (配置路径) + canonical_context (动态) | cap_str() | 尾部截断 |
| **goose** | 无自动发现 | 无 | 无 | 无 |

**评分**: octo-sandbox **8/10**, openfang 7/10, goose 1/10

**差距分析**:
- **octo 的 bootstrap 发现最完善**: 6 种文件名、大小限制体系、70/20 头尾截断都设计合理。
- **OpenFang 的优势在配置灵活性**: soul_md/identity_md 可配置任意路径，不限于固定文件名。
- **octo 的 70% head + 20% tail 截断策略优于 OpenFang 的纯尾部截断**: 保留文件开头（通常是概览/说明）和结尾（通常是最新内容），中间部分省略。

### 11.7 多模态内容处理

| 项目 | 图片 Token 估算 | 文档 Token 估算 | 多模态压缩 |
|------|---------------|---------------|-----------|
| **octo-sandbox** | `data.len() / 4` (base64 长度/4) | `data.len() / 4` | 无专门处理（与文本相同逻辑降级） |
| **openfang** | 无图片 token 估算 | 无 | 无 |
| **goose** | resources 包含在 `count_everything()` | 无 | 无 |

**评分**: octo-sandbox 5/10, openfang 3/10, goose 4/10

**差距分析**:
- **所有项目的多模态 context 管理都不成熟**: 没有任何项目实现了图片 token 的精确估算（Anthropic: 固定 1600 tokens/image; OpenAI: 基于分辨率的 tile 计算）。
- **octo 的 `data.len() / 4` 对 base64 图片严重高估**: 一张 100KB 的图片 base64 约 133K chars，chars/4 = 33K tokens，但 Anthropic 实际只算 ~1600 tokens。这会导致 context budget 过早触发降级。
- **所有项目都缺少图片压缩/缩放策略**: 当 context 紧张时应该降低图片分辨率或移除非关键图片。

---

## 十二、octo-sandbox 独有优势（Context Engineering）

| 能力 | 描述 | 竞品状态 |
|------|------|---------|
| **ContextFork** | 隔离式 skill 执行，快照父消息，限制 `max_parent_messages`，`new_messages()` 返回 fork 后新增 | 无竞品有此机制 |
| **ObservationMasker** | 非破坏性工具结果遮蔽，保留最近 3 轮，`[output hidden - {chars} chars]` 占位 | 无竞品有此机制 |
| **MemoryFlusher** | 压缩前 LLM 事实提取，写入 WorkingMemory 为 `AutoExtracted` 块 | 无竞品有此机制 |
| **Dual-Track 估算** | API 实际 input_tokens 作基线 + chars/4 仅估新增消息 | 无竞品有此模式 |
| **CJK-Aware 计数** | ASCII 0.25 vs CJK 0.67 tokens/char 区分处理 | 无竞品有此优化 |
| **6 级渐进降级** | None/SoftTrim/AutoCompaction/OverflowCompaction/ToolResultTruncation/FinalError | OpenFang 4 级，goose 0 级 |
| **Tool Chain 安全切割** | `find_compaction_boundary()` 不在 ToolUse-ToolResult 对中间切 | 无竞品有此保护 |
| **SKILL_PROTECTED_MARKER** | `[SKILL:ALWAYS]` 前缀标记的消息永不被 prune | 无竞品有等价机制 |
| **Bootstrap 70/20 截断** | 保留文件头 70% + 尾 20%，中间省略 | 无竞品有此策略 |

---

## 十三、竞品做得更好的领域

### 差距 1: Goose 的工具 Schema 结构化 Token 建模

**影响**: 中高。当 agent 挂载 20+ 工具时，tool schema 的 token 占用可达数千。结构化建模（FUNC_INIT 7 + PROP_KEY 3 * N + FUNC_END 12）比 chars/4 精确 2-5 倍。

**当前 octo 实现** (`budget.rs:86-92`):
```rust
pub fn estimate_tool_specs_tokens(tools: &[ToolSpec]) -> u64 {
    let chars: usize = tools.iter()
        .map(|t| t.name.len() + t.description.len() + t.input_schema.to_string().len())
        .sum();
    (chars / CHARS_PER_TOKEN) as u64
}
```

**建议**: 参考 goose `token_counter.rs` 的 `count_tool_tokens()` 实现，对 JSON Schema 结构元素分别计算 token 成本。

### 差距 2: OpenFang 的 Prompt Cache 优化

**影响**: 高。Anthropic prompt cache 可节省 90% 的 system prompt token 成本（cached tokens 按 0.1x 计费）。当前 octo 的 `build_dynamic_context()` 嵌入 system prompt，每次变化都破坏 cache。

**当前 octo 实现** (`builder.rs`): `build_dynamic_context()` 返回包含日期/session状态的字符串，被嵌入 system prompt。

**OpenFang 做法**: `build_canonical_context_message()` 将动态内容作为 user message 注入，system prompt 保持纯静态。

**建议**: 将 `build_dynamic_context()` 的输出从 system prompt 移到第一条 user message 中，添加 `[CONTEXT]` 前缀标识。

### 差距 3: OpenFang 的 LLM 3 阶段摘要压缩

**影响**: 高。octo 的 `auto_compact.rs` 只生成占位文本，压缩后丢失所有对话上下文。OpenFang 用 LLM 生成真正的摘要保留关键信息。

**当前 octo 实现** (`auto_compact.rs`):
```
输出: [Compacted: {title}... ({chars} chars -> {tokens_saved} tokens saved)]
```

**OpenFang 做法**: 全量摘要 -> 分块摘要+合并 -> 最小回退，3 层策略确保在不同 token 预算下都能生成有意义的摘要。

**建议**: 在 `auto_compact.rs` 中调用 Provider 的 `complete()` 方法生成真正的摘要。需要解决循环依赖问题（context 模块调用 provider 模块）——可通过 trait 抽象或回调函数注入。

### 差距 4: OpenFang 的 Subagent Prompt 裁剪

**影响**: 中。subagent 执行时不需要完整的 14 段 prompt（channel type, peer agents, heartbeat 等与子任务无关）。

**建议**: 在 `SystemPromptBuilder` 中添加 `mode: Normal | Subagent | Minimal` 参数，Subagent 模式跳过不相关段。

### 差距 5: OpenFang 的 Channel-Aware 输出格式化

**影响**: 中低（仅在 multi-channel 场景下有用）。

**建议**: 在 SystemPromptBuilder 中添加可选的 `output_format_hint` 段，根据前端类型（web/cli/telegram/discord）注入输出格式指导。

### 差距 6: OpenFang 的动态工具结果预算

**影响**: 中高。固定的 8000 chars 截断上限对 200K context 模型太小，对 8K context 模型太大。

**当前 octo 实现**: `TOOL_RESULT_TRUNCATION_CHARS: 8000` (固定常量)

**OpenFang 做法**: `per_result_cap() = context_window * 0.30 * chars_per_token`

**建议**: 将固定常量改为 `context_window` 的比例计算。

---

## 十四、Context Engineering 综合评分

| 维度 | 权重 | octo-sandbox | openfang | goose | ironclaw | zeroclaw | moltis | pi_agent |
|------|------|-------------|----------|-------|----------|----------|--------|----------|
| System Prompt 构建 | 20% | 7.0 | **9.5** | 6.0 | 1.0 | 1.0 | 2.0 | 1.0 |
| Token 估算精度 | 15% | 7.5 | 5.0 | **9.0** | 1.0 | 1.0 | 1.0 | 1.0 |
| Context 压缩/降级 | 20% | **8.5** | 8.0 | 2.0 | 1.0 | 1.0 | 1.0 | 1.0 |
| 工具结果截断 | 15% | 7.0 | **9.0** | 2.0 | 1.0 | 1.0 | 1.0 | 1.0 |
| 消息保护标记 | 10% | **8.5** | 5.0 | 4.0 | 1.0 | 1.0 | 1.0 | 1.0 |
| Bootstrap 文件发现 | 10% | **8.0** | 7.0 | 1.0 | 1.0 | 1.0 | 1.0 | 1.0 |
| 多模态内容处理 | 10% | 5.0 | 3.0 | 4.0 | 1.0 | 1.0 | 1.0 | 1.0 |
| **加权总分** | 100% | **7.48** | **7.05** | **4.25** | **1.00** | **1.00** | **1.30** | **1.00** |

**排名**: octo-sandbox (7.48) > openfang (7.05) > goose (4.25) > moltis (1.30) > ironclaw = zeroclaw = pi_agent (1.00)

---

## 十五、改进路线图（Context Engineering）

### P0: 高影响快速改进（< 2 天）

1. **Prompt Cache 优化** — 将 `build_dynamic_context()` 输出从 system prompt 移到 user message。~50 行改动。**预期收益**: system prompt cache 命中率从 0% 提升到 ~95%，token 成本降低 30-50%。

2. **动态工具结果预算** — 将 `TOOL_RESULT_TRUNCATION_CHARS`、`SOFT_TRIM_HEAD/TAIL` 改为 `context_window` 的比例。~30 行改动。**预期收益**: 200K context 模型的工具结果利用率提升 3-5 倍。

3. **图片 Token 估算修正** — Image 的 token 估算不应用 `data.len() / 4`，应根据 provider 使用固定值（Anthropic ~1600）或基于分辨率计算（OpenAI tile 模型）。~40 行改动。**预期收益**: 避免包含图片时过早触发降级。

### P1: 中等影响改进（2-5 天）

4. **LLM 摘要压缩** — 替换 `auto_compact.rs` 的占位符为真正的 LLM 摘要。需要注入 Provider 引用（通过 trait 或 closure）。~200 行。**预期收益**: 压缩后保留对话语义，而非丢失全部上下文。

5. **工具 Schema 结构化 Token 建模** — 参考 goose 的结构常量，对 JSON Schema 精确建模。~100 行。**预期收益**: tool token 估算精度提升 2-5 倍。

6. **tiktoken 编码升级** — 从 cl100k_base 升级到 o200k_base。~5 行改动。**预期收益**: 对 GPT-4o/o-系列模型的 token 计数更准确。

### P2: 架构级改进（5-10 天）

7. **Subagent Prompt 模式** — SystemPromptBuilder 添加 Normal/Subagent/Minimal 模式，Subagent 跳过不相关段。~80 行。

8. **Token Cache (DashMap)** — 添加并发 token 缓存避免重复 BPE 编码。~60 行。

9. **Channel-Aware 输出格式** — 按前端类型注入输出格式指导。~40 行。

10. **工具结果换行对齐截断** — SoftTrim 和 ToolResultTruncation 在换行符位置截断。~20 行。

---

## 十六、总结

### Context Engineering 层面

**octo-sandbox 总体领先**，以 7.48 分位居第一，主要得益于 5 项独有能力（ContextFork、ObservationMasker、MemoryFlusher、Dual-Track 估算、CJK-Aware 计数）和最精细的 6 级降级管线。

**OpenFang 在 3 个维度做得更好**:
1. Prompt Cache 优化（动态内容移出 system prompt）— **这是 octo 最该学习的**
2. LLM 3 阶段摘要压缩（octo 只有占位符）
3. 动态工具结果预算（按 context window 比例计算）

**Goose 在 Token 计数精度上领先**: o200k_base 编码 + DashMap 缓存 + 结构化 schema 建模。

**其余 4 个竞品（ironclaw, zeroclaw, moltis, pi_agent_rust）没有实质性的 Context Engineering 实现**。

### Provider + Context 双层综合

将 Provider 层和 Context Engineering 层合并评估:

| 项目 | Provider 分 | Context 分 | 综合分 |
|------|-----------|-----------|-------|
| **octo-sandbox** | 8.43 | 7.48 | **7.96** |
| **openfang** | 6.18 | 7.05 | **6.62** |
| **goose** | 7.73 | 4.25 | **6.00** |

**octo-sandbox 在整体工程成熟度上保持领先**。关键在于：Provider 层的装饰器管线 + Context 层的渐进式降级 + 独有的 Fork/Masker/Flusher 三件套，形成了完整的 context 生命周期管理能力，这是其他项目都不具备的系统性优势。
