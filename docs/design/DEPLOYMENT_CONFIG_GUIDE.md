# Octo 部署配置使用指南

> **Phase AA** | 创建于 2026-03-23 | 适用版本: Phase AA+

---

## 快速开始

### 首次安装后

```bash
# 1. 编译安装
cargo install --path crates/octo-cli
cargo install --path crates/octo-server

# 2. 配置 API Key（二选一）
# 方式 A: 交互式（推荐）
octo auth login

# 方式 B: 环境变量
export ANTHROPIC_API_KEY="sk-ant-xxxxx"

# 3. 启动
octo run        # CLI/TUI 模式
octo-server     # Server 模式
```

### 项目初始化（可选）

```bash
cd my-project
octo init
# 创建:
#   .octo/config.yaml     项目配置模板
#   .octo/skills/          项目 skills 目录
```

---

## 配置文件位置

Octo 从以下位置加载配置，按优先级从低到高排列：

```
优先级 7（最低）:  代码默认值
优先级 6:          ~/.octo/config.yaml          全局配置
优先级 5:          $PWD/.octo/config.yaml        项目配置（可 git 跟踪）
优先级 4:          $PWD/.octo/config.local.yaml  个人覆盖（git-ignored）
优先级 3:          $PWD/.env                     环境变量文件
优先级 2:          Shell 环境变量                OCTO_*, ANTHROPIC_*, OPENAI_*
优先级 1（最高）:  CLI 参数                      --port, --config 等
```

高优先级的值覆盖低优先级的值。

### 特殊情况：`--config` 指定

使用 `--config /path/to/config.yaml` 时，跳过自动发现，只读取该文件 + 环境变量覆盖。

---

## 目录结构

### 全局目录 `~/.octo/`

```
~/.octo/
├── config.yaml              全局默认配置
├── credentials.yaml         API Keys（mode 600，仅用户可读）
├── skills/                  全局 skills（跨项目共享）
├── mcp/                     全局 MCP server 定义
├── cache/                   缓存数据
├── tls/                     TLS 证书
└── projects/
    └── Users_foo_myproject/ 每项目隔离数据
        ├── meta.json         项目元信息
        ├── octo.db           SQLite 数据库
        ├── workspace/        Agent 输出
        └── history/          会话历史
```

可通过 `OCTO_GLOBAL_ROOT` 环境变量覆盖位置。

### 项目目录 `$PWD/.octo/`

```
$PWD/.octo/
├── config.yaml              项目配置（团队共享，git 跟踪）
├── config.local.yaml        个人覆盖（git-ignored）
├── skills/                  项目 skills
├── mcp/                     项目 MCP server 定义
└── eval.toml                评估配置
```

可通过 `OCTO_PROJECT_ROOT` 环境变量覆盖位置。

---

## 配置文件格式

### config.yaml 完整参考

```yaml
# ═══════════════════════════════════════════════════════
# Octo 配置文件
# ═══════════════════════════════════════════════════════
# 可放在以下任一位置:
#   ~/.octo/config.yaml          全局
#   $PWD/.octo/config.yaml       项目
#   $PWD/.octo/config.local.yaml 个人

# ── Server ──────────────────────────────────────────
server:
  host: "127.0.0.1"        # 绑定地址
  port: 3001                # 端口
  cors_origins: []          # CORS 来源，空 = 允许所有

# ── LLM Provider ────────────────────────────────────
provider:
  name: "anthropic"         # anthropic | openai
  # api_key 建议放 credentials.yaml 或环境变量
  model: null               # 模型覆盖，如 "claude-sonnet-4-20250514"
  base_url: null            # 代理 URL

# ── Provider Chain（多 Provider 故障切换）──────────
# provider_chain:
#   failover_policy: automatic
#   health_check_interval_sec: 30
#   instances:
#     - id: primary
#       provider: anthropic
#       model: claude-sonnet-4-20250514
#       api_key: "${ANTHROPIC_API_KEY}"
#       priority: 1
#     - id: fallback
#       provider: openai
#       model: gpt-4o
#       api_key: "${OPENAI_API_KEY}"
#       priority: 2

# ── Database ────────────────────────────────────────
database:
  path: ""                  # 空 = 自动从 OctoRoot 解析

# ── Logging ─────────────────────────────────────────
logging:
  level: "octo_server=info,octo_engine=info,tower_http=info"

# ── Skills ──────────────────────────────────────────
skills:
  dirs: []                  # 空 = 自动从 OctoRoot 解析

# ── Tools ───────────────────────────────────────────
tools:
  web_search_priority:      # 搜索引擎优先级
    - "jina"
    - "tavily"
    - "ddg"

# ── MCP ─────────────────────────────────────────────
mcp:
  servers_dir: null         # MCP servers 目录（null = OctoRoot 自动解析）

# ── Auth ────────────────────────────────────────────
auth:
  mode: null                # none | api_key | full
  # api_keys:
  #   - key: "your-secret"
  #     user_id: "dev"
  #     permissions: ["read", "write", "admin"]

# ── TLS ─────────────────────────────────────────────
tls:
  enabled: false
  cert_path: null           # PEM 证书路径
  key_path: null            # PEM 私钥路径
  self_signed: false        # 自动生成自签名证书

# ── Scheduler ───────────────────────────────────────
scheduler:
  enabled: false
  check_interval_secs: 60
  max_concurrent: 5

# ── Smart Routing（复杂度路由）───────────────────
# smart_routing:
#   enabled: false
#   default_tier: medium

# ── Event Bus ───────────────────────────────────────
enable_event_bus: false

# ── Sync ────────────────────────────────────────────
sync:
  enabled: false
  node_id: null             # 自动生成 UUID
```

### credentials.yaml 格式

```yaml
# ~/.octo/credentials.yaml
# 权限: mode 600 (仅用户可读写)
# 由 `octo auth login` 管理

providers:
  anthropic:
    api_key: "sk-ant-xxxxx"
  openai:
    api_key: "sk-xxxxx"
    base_url: "https://api.openai.com/v1"

# Platform JWT（可选）
platform:
  jwt_secret: "xxx"
```

### eval.toml 格式

```toml
# $PWD/.octo/eval.toml

[default]
provider = "openai"
model = "qwen3.5-27b"
base_url = "https://openrouter.ai/api/v1"
# api_key 从环境变量或 credentials.yaml 获取

[suites.gaia_core]
enabled = true
tasks_limit = 50

[suites.tool_use]
enabled = true
```

---

## 部署场景

### 场景 1: 开发者本地

```bash
# 一次性设置
octo auth login                      # 配置 API Key
echo '.octo/config.local.yaml' >> .gitignore

# 全局默认（所有项目共享）
cat > ~/.octo/config.yaml << 'EOF'
provider:
  name: anthropic
logging:
  level: "octo_engine=info"
EOF

# 项目配置（团队共享）
mkdir -p .octo
cat > .octo/config.yaml << 'EOF'
provider:
  model: "qwen3.5-27b"
  base_url: "https://openrouter.ai/api/v1"
tools:
  web_search_priority: ["jina", "tavily"]
EOF

# 个人覆盖（不入 git）
cat > .octo/config.local.yaml << 'EOF'
logging:
  level: "octo_engine=debug"
server:
  port: 4000
EOF

# 启动
octo run
```

### 场景 2: 单机 Server

```bash
# 配置文件
mkdir -p ~/.octo
cat > ~/.octo/config.yaml << 'EOF'
server:
  host: "0.0.0.0"
  port: 8080
provider:
  name: anthropic
auth:
  mode: api_key
tls:
  enabled: true
  self_signed: true
scheduler:
  enabled: true
EOF

# 凭证
cat > ~/.octo/credentials.yaml << 'EOF'
providers:
  anthropic:
    api_key: "sk-ant-xxxxx"
EOF
chmod 600 ~/.octo/credentials.yaml

# 启动
octo-server
```

### 场景 3: Docker

```bash
# docker-compose.yml
version: "3.8"
services:
  octo-server:
    image: octo-server:latest
    ports:
      - "3001:3001"
    environment:
      - OCTO_GLOBAL_ROOT=/data/octo
      - OCTO_HOST=0.0.0.0
      - ANTHROPIC_API_KEY=${ANTHROPIC_API_KEY}
      - OCTO_AUTH_MODE=api_key
      - OCTO_API_KEY=${OCTO_API_KEY}
    volumes:
      - octo-data:/data/octo
      - ./config.yaml:/data/octo/config.yaml:ro

volumes:
  octo-data:
```

```bash
# 启动
ANTHROPIC_API_KEY=sk-ant-xxx OCTO_API_KEY=my-secret docker compose up
```

### 场景 4: Kubernetes

参见设计文档 `DEPLOYMENT_CONFIG_DESIGN.md` 第七节的完整 K8s YAML 示例。

核心要点：
- ConfigMap 挂载 `config.yaml`
- Secret 注入 `ANTHROPIC_API_KEY` 等凭证
- PersistentVolumeClaim 存储 SQLite 和 Agent 输出
- `OCTO_GLOBAL_ROOT` 指向 Volume 挂载点

---

## 环境变量速查

### 核心变量

| 变量 | 说明 | 默认值 |
|------|------|--------|
| `OCTO_GLOBAL_ROOT` | 全局根目录 | `~/.octo` |
| `OCTO_PROJECT_ROOT` | 项目根目录 | `$PWD/.octo` |
| `OCTO_HOST` | Server 地址 | `127.0.0.1` |
| `OCTO_PORT` | Server 端口 | `3001` |
| `OCTO_DB_PATH` | 数据库路径 | 自动解析 |
| `OCTO_LOG` | 日志级别 | 配置文件值 |
| `OCTO_LOG_FORMAT` | 日志格式 | `default` |
| `OCTO_AUTH_MODE` | 认证模式 | `none` |
| `OCTO_API_KEY` | 服务 API Key | - |

### Provider 变量

| 变量 | 说明 |
|------|------|
| `LLM_PROVIDER` | 默认 provider |
| `ANTHROPIC_API_KEY` | Anthropic Key |
| `ANTHROPIC_BASE_URL` | Anthropic 代理 |
| `OPENAI_API_KEY` | OpenAI Key |
| `OPENAI_BASE_URL` | OpenAI 代理 |

### TLS 变量

| 变量 | 说明 |
|------|------|
| `OCTO_TLS_ENABLED` | 启用 TLS |
| `OCTO_TLS_CERT_PATH` | 证书路径 |
| `OCTO_TLS_KEY_PATH` | 私钥路径 |
| `OCTO_TLS_SELF_SIGNED` | 自签名 |

---

## 诊断命令

```bash
# 查看所有路径
octo root show

# 查看生效配置及来源
octo config show

# 验证配置完整性
octo config validate

# 检查系统健康状态
octo doctor

# 初始化项目目录
octo init

# 初始化全局目录
octo root init
```

### `octo config show` 输出示例

```
Configuration Sources (highest priority first):
  ✅ Env:     OCTO_LOG=octo_engine=debug
  ✅ Env:     ANTHROPIC_API_KEY=sk-ant-...xxxx
  ─  Local:   .octo/config.local.yaml (not found)
  ✅ Project: .octo/config.yaml
  ✅ Global:  ~/.octo/config.yaml

Effective Config:
  server.host     = 127.0.0.1      (global)
  server.port     = 4000           (project)
  provider.name   = anthropic      (global)
  provider.model  = qwen3.5-27b   (project)
  logging.level   = octo_engine=debug  (env: OCTO_LOG)
  database.path   = ~/.octo/projects/.../octo.db  (OctoRoot)
  skills.dirs     = [.octo/skills/, ~/.octo/skills/]  (OctoRoot)
```

---

## 迁移指南

### 从旧配置迁移

如果你的项目根目录有 `config.yaml`（旧位置）：

```bash
# 创建 .octo 目录
mkdir -p .octo

# 移动配置文件
mv config.yaml .octo/config.yaml

# 如果有 eval.toml
mv eval.toml .octo/eval.toml

# 更新 .gitignore
echo '.octo/config.local.yaml' >> .gitignore
```

### 从 .env 迁移 API Keys

```bash
# 查看当前 .env 中的 keys
grep API_KEY .env

# 运行交互式设置（写入 ~/.octo/credentials.yaml）
octo auth login

# .env 中只保留非敏感的环境覆盖
# 删除 API_KEY 行，它们现在在 credentials.yaml 中
```

---

## 常见问题

### Q: config.yaml 放在哪里？

- **全局默认**: `~/.octo/config.yaml`（所有项目共享的默认值）
- **项目配置**: `$PWD/.octo/config.yaml`（项目特定，可 git 跟踪）
- **个人覆盖**: `$PWD/.octo/config.local.yaml`（个人偏好，不入 git）

### Q: API Key 放在哪里？

推荐优先级：
1. 环境变量 `ANTHROPIC_API_KEY`（CI/CD 场景）
2. `~/.octo/credentials.yaml`（本地开发）
3. `.env` 文件（向后兼容）

**永远不要**把 API Key 放在 `config.yaml` 中。

### Q: 团队成员怎么共享项目配置？

1. 把 `.octo/config.yaml` 加入 git
2. 把 `.octo/config.local.yaml` 加入 `.gitignore`
3. 每个人在 `config.local.yaml` 中放个人偏好（端口、日志级别等）
4. API Keys 各自通过 `octo auth login` 或环境变量配置

### Q: Docker 部署需要哪些文件？

最小部署只需要：
- `config.yaml`（通过 ConfigMap 或 Volume 挂载）
- 环境变量：`ANTHROPIC_API_KEY`、`OCTO_GLOBAL_ROOT`
- 一个持久化 Volume（存 SQLite 和 Agent 输出）

### Q: 配置不生效怎么排查？

```bash
# 1. 检查配置来源
octo config show

# 2. 检查配置有效性
octo config validate

# 3. 检查系统状态
octo doctor
```
