# Octo 部署配置架构设计

> **Phase AA** | 创建于 2026-03-23 | 状态: 设计完成

---

## 一、设计目标

建立统一的配置加载、凭证管理和部署运行架构，使 Octo 在以下三种场景下都能正确运行：

1. **开发者本地**（CLI / TUI）— `octo run`
2. **单机 Server**（Workbench）— `octo-server`
3. **容器化部署**（Docker / K8s）— `octo-platform-server`

---

## 二、业界最佳实践对标

对 8 个主流 CLI 工具（Claude Code, Cursor, gh, Docker, npm, Git, Terraform, Cargo）配置架构研究，得出共识：

| 原则 | 共识 | 采用工具数 |
|------|------|-----------|
| 双层配置（Global + Project） | 项目级覆盖全局级 | 6/8 |
| 凭证与配置分离 | 独立文件或 credential helper | 8/8 |
| 环境变量最高优先级 | `TOOL_PREFIX_*` 命名约定 | 8/8 |
| 完整优先链 | CLI > env > project.local > project > global > defaults | 8/8 |
| .local 变体 git-ignored | 个人覆盖不入库 | 3/8（新趋势） |
| 首次运行惰性创建 | 按需创建目录，非提前创建 | 6/8 |

---

## 三、目录布局设计

### 3.1 全局目录 `~/.octo/`

用户级配置和数据，不入 git，跨所有项目共享。

```
~/.octo/                              OCTO_GLOBAL_ROOT (可被环境变量覆盖)
├── config.yaml                       全局配置（默认 provider、server 参数等）
├── credentials.yaml                  凭证文件（API keys，mode 600 权限）
├── skills/                           全局 skills
├── mcp/                              全局 MCP server 定义
├── cache/                            缓存（模型、下载文件等）
├── tls/                              TLS 证书（自签名或导入）
└── projects/                         每项目隔离数据
    └── <encoded_project_key>/
        ├── meta.json                 项目元信息
        ├── octo.db                   SQLite（sessions, memory, tools, audit）
        ├── workspace/                Agent 输出文件
        └── history/                  会话历史快照
```

### 3.2 项目目录 `$PWD/.octo/`

项目级配置，可 git 跟踪，团队共享。

```
$PWD/.octo/                           OCTO_PROJECT_ROOT (可被环境变量覆盖)
├── config.yaml                       项目配置覆盖（模型选择、sandbox 策略、tools 配置等）
├── config.local.yaml                 个人覆盖（git-ignored，端口、日志级别等）
├── skills/                           项目 skills
├── mcp/                              项目 MCP server 定义
└── eval.toml                         评估配置
```

### 3.3 文件权限与 git 跟踪规则

| 文件 | 权限 | Git 跟踪 | 说明 |
|------|------|---------|------|
| `~/.octo/config.yaml` | 644 | N/A（不在仓库内） | 全局默认配置 |
| `~/.octo/credentials.yaml` | **600** | N/A | 敏感凭证，仅用户可读 |
| `$PWD/.octo/config.yaml` | 644 | **是** | 团队共享的项目配置 |
| `$PWD/.octo/config.local.yaml` | 644 | **否**（.gitignore） | 个人偏好覆盖 |
| `$PWD/.octo/skills/` | 755 | 是 | 项目 skills |
| `$PWD/.octo/eval.toml` | 644 | 是 | 评估配置 |
| `$PWD/.env` | 644 | **否** | 环境变量（向后兼容） |

### 3.4 .gitignore 模板

项目 `.gitignore` 应包含：

```gitignore
# Octo — 个人/敏感文件
.octo/config.local.yaml
.env
.env.*
```

---

## 四、配置优先链

### 4.1 完整优先级（从高到低）

```
1. CLI 参数           --port 4000, --config /path/to/config.yaml
2. 环境变量           OCTO_PORT=4000, ANTHROPIC_API_KEY=sk-xxx
3. .env 文件          $PWD/.env (dotenvy, 向后兼容)
4. 项目本地覆盖       $PWD/.octo/config.local.yaml
5. 项目配置           $PWD/.octo/config.yaml
6. 全局配置           ~/.octo/config.yaml
7. 代码默认值         Rust impl Default
```

### 4.2 合并策略

采用**字段级浅合并**（field-level shallow merge）：

- 标量字段：高优先级直接覆盖低优先级
- 列表字段：高优先级替换（非追加）低优先级
- 嵌套对象：递归合并到第一层子字段

示例：

```yaml
# ~/.octo/config.yaml (全局)
provider:
  name: anthropic
  model: claude-sonnet-4-20250514
server:
  port: 3001

# $PWD/.octo/config.yaml (项目)
provider:
  model: qwen3.5-27b       # 覆盖 model，name 保持 anthropic
server:
  port: 4000               # 覆盖 port

# $PWD/.octo/config.local.yaml (个人)
logging:
  level: "octo_engine=debug"  # 仅影响本地调试
```

合并结果：

```yaml
provider:
  name: anthropic           # 来自全局
  model: qwen3.5-27b        # 来自项目
server:
  port: 4000                # 来自项目
logging:
  level: "octo_engine=debug" # 来自本地
```

### 4.3 `--config` 显式指定的行为

当用户使用 `--config /path/to/config.yaml` 时：

- **替代** 全局 + 项目配置（不合并，直接使用该文件作为唯一配置源）
- 环境变量仍然覆盖
- 这与 Cargo、Terraform 的行为一致：显式指定 = 跳过自动发现

---

## 五、凭证管理

### 5.1 凭证文件 `~/.octo/credentials.yaml`

```yaml
# ~/.octo/credentials.yaml
# 此文件由 `octo auth` 命令管理，权限 mode 600
# 请勿手动编辑，除非你知道自己在做什么

providers:
  anthropic:
    api_key: "sk-ant-xxxxx"
  openai:
    api_key: "sk-xxxxx"
    base_url: "https://openrouter.ai/api/v1"

# Octo Platform 认证
platform:
  jwt_secret: "xxx"

# 加密 Vault（可选）
vault:
  enabled: false
  # password 通过环境变量 OCTO_VAULT_PASSWORD 提供
```

### 5.2 凭证解析优先级

```
1. 环境变量     ANTHROPIC_API_KEY, OPENAI_API_KEY
2. .env 文件    $PWD/.env
3. credentials  ~/.octo/credentials.yaml
4. Vault        ~/.octo/projects/<key>/vault (如果 OCTO_VAULT_PASSWORD 已设置)
```

### 5.3 `octo auth` 命令（建议新增）

```bash
octo auth login          # 交互式设置 API keys → 写入 ~/.octo/credentials.yaml
octo auth status          # 显示已配置的 provider 和 key 掩码
octo auth logout          # 清除凭证
```

---

## 六、配置加载实现方案

### 6.1 Config::load() 改造

当前签名：

```rust
pub fn load(config_path: Option<&PathBuf>, cli_port: Option<u16>, cli_host: Option<&str>) -> Self
```

改造后签名：

```rust
pub fn load(
    explicit_config: Option<&PathBuf>,  // --config 显式指定
    cli_port: Option<u16>,
    cli_host: Option<&str>,
    octo_root: &OctoRoot,               // 新增：用于定位配置文件
) -> Self
```

伪代码：

```rust
fn load(explicit_config, cli_port, cli_host, octo_root) -> Config {
    let mut config = if let Some(path) = explicit_config {
        // 显式指定：只加载这一个文件
        load_yaml(path).unwrap_or_default()
    } else {
        // 自动发现：全局 → 项目 → 本地 三层合并
        let global = load_yaml(octo_root.global_config());     // ~/.octo/config.yaml
        let project = load_yaml(octo_root.project_config());   // $PWD/.octo/config.yaml
        let local = load_yaml(octo_root.project_local_config()); // $PWD/.octo/config.local.yaml

        merge(merge(global, project), local)
    };

    // CLI 参数覆盖
    apply_cli_args(&mut config, cli_port, cli_host);

    // 环境变量覆盖（最高优先级）
    apply_env_vars(&mut config);

    // 凭证注入（从 credentials.yaml + env）
    inject_credentials(&mut config, octo_root);

    config
}
```

### 6.2 OctoRoot 新增路径方法

```rust
impl OctoRoot {
    /// 项目本地覆盖配置（git-ignored）
    pub fn project_local_config(&self) -> PathBuf {
        self.project_root.join("config.local.yaml")
    }

    /// 全局凭证文件
    pub fn credentials_path(&self) -> PathBuf {
        self.global_root.join("credentials.yaml")
    }

    /// 全局 MCP 目录
    pub fn global_mcp_dir(&self) -> PathBuf {
        self.global_root.join("mcp")
    }

    /// 项目 MCP 目录
    pub fn project_mcp_dir(&self) -> PathBuf {
        self.project_root.join("mcp")
    }

    /// 全局 TLS 目录
    pub fn tls_dir(&self) -> PathBuf {
        self.global_root.join("tls")
    }

    /// 项目 eval 配置
    pub fn eval_config(&self) -> PathBuf {
        self.project_root.join("eval.toml")
    }
}
```

### 6.3 向后兼容

| 旧路径 | 新路径 | 兼容策略 |
|--------|--------|---------|
| `$PWD/config.yaml` | `$PWD/.octo/config.yaml` | 如果旧路径存在且新路径不存在，仍然读取旧路径并打印迁移提示 |
| `$PWD/eval.toml` | `$PWD/.octo/eval.toml` | 同上 |
| `./data/octo.db` | `~/.octo/projects/<key>/octo.db` | OctoRoot 已处理 |
| `./data/tls` | `~/.octo/tls/` | 改为读取 OctoRoot |
| `./data/certs` | `~/.octo/tls/` | 统一到 tls_dir() |

迁移提示示例：

```
⚠️  Found config.yaml in project root (legacy location).
    Please move it to .octo/config.yaml:
    mv config.yaml .octo/config.yaml

    This legacy fallback will be removed in a future version.
```

---

## 七、部署模式

### 7.1 开发者本地（CLI / TUI）

```
                          用户执行
                          $ cd my-project && octo run
                              │
                              ▼
                        OctoRoot::discover()
                              │
                    ┌─────────┴─────────┐
                    ▼                   ▼
              ~/.octo/            $PWD/.octo/
              config.yaml         config.yaml
              credentials.yaml    config.local.yaml
              skills/             skills/
                    │                   │
                    └─────────┬─────────┘
                              ▼
                        Config::load()
                    (global → project → local → env)
                              │
                              ▼
                        AgentRuntime
                              │
                    ┌─────────┼─────────┐
                    ▼         ▼         ▼
               Provider   SQLite    SkillRegistry
               (LLM)      (DB)      (Skills)
```

**首次使用流程：**

```bash
# 1. 安装后首次运行
$ octo run
# → 自动创建 ~/.octo/ 目录结构
# → 检测无 API key，提示：
#   No API key configured. Run `octo auth login` to set up.

# 2. 配置凭证
$ octo auth login
# → 交互式输入 API key → 写入 ~/.octo/credentials.yaml

# 3. 项目初始化（可选）
$ octo init
# → 创建 $PWD/.octo/config.yaml（从模板）
# → 创建 $PWD/.octo/skills/（空目录）
```

### 7.2 单机 Server（Workbench）

```bash
# 方式 A: 最简启动（使用 ~/.octo/ 配置）
$ octo-server

# 方式 B: 指定配置文件
$ octo-server --config /etc/octo/config.yaml

# 方式 C: 环境变量（推荐生产环境）
$ OCTO_PORT=8080 \
  ANTHROPIC_API_KEY=sk-xxx \
  OCTO_TLS_ENABLED=true \
  OCTO_TLS_CERT_PATH=/etc/octo/tls/cert.pem \
  OCTO_TLS_KEY_PATH=/etc/octo/tls/key.pem \
  octo-server
```

### 7.3 容器化部署（Docker / K8s）

```dockerfile
# Dockerfile
FROM rust:1.75-slim AS builder
# ... build steps ...

FROM debian:bookworm-slim
COPY --from=builder /app/octo-server /usr/local/bin/
COPY --from=builder /app/octo /usr/local/bin/

# 默认全局根（可通过 OCTO_GLOBAL_ROOT 覆盖）
ENV OCTO_GLOBAL_ROOT=/data/octo
# 不使用项目根（server 模式不需要）
ENV OCTO_PROJECT_ROOT=/data/octo/project

VOLUME /data/octo
EXPOSE 3001

ENTRYPOINT ["octo-server"]
```

```yaml
# Kubernetes ConfigMap
apiVersion: v1
kind: ConfigMap
metadata:
  name: octo-config
data:
  config.yaml: |
    server:
      host: "0.0.0.0"
      port: 3001
    provider:
      name: anthropic
    logging:
      level: "octo_server=info,octo_engine=info"
    tls:
      enabled: true
      cert_path: /etc/octo/tls/tls.crt
      key_path: /etc/octo/tls/tls.key

---
# Kubernetes Secret
apiVersion: v1
kind: Secret
metadata:
  name: octo-credentials
type: Opaque
data:
  ANTHROPIC_API_KEY: <base64>
  OCTO_JWT_SECRET: <base64>

---
# Deployment
apiVersion: apps/v1
kind: Deployment
metadata:
  name: octo-server
spec:
  template:
    spec:
      containers:
      - name: octo-server
        image: octo-server:latest
        envFrom:
        - secretRef:
            name: octo-credentials
        env:
        - name: OCTO_GLOBAL_ROOT
          value: "/data/octo"
        volumeMounts:
        - name: config
          mountPath: /data/octo/config.yaml
          subPath: config.yaml
        - name: data
          mountPath: /data/octo
      volumes:
      - name: config
        configMap:
          name: octo-config
      - name: data
        persistentVolumeClaim:
          claimName: octo-data
```

---

## 八、环境变量完整映射

### 8.1 OCTO_* 核心变量

| 变量 | 默认值 | 说明 |
|------|--------|------|
| `OCTO_GLOBAL_ROOT` | `~/.octo` | 全局根目录 |
| `OCTO_PROJECT_ROOT` | `$PWD/.octo` | 项目根目录 |
| `OCTO_HOST` | `127.0.0.1` | Server 绑定地址 |
| `OCTO_PORT` | `3001` | Server 端口 |
| `OCTO_DB_PATH` | 自动解析 | 数据库路径覆盖 |
| `OCTO_LOG` | config 值 | 日志级别 |
| `OCTO_LOG_FORMAT` | `default` | 日志格式 (default/json) |
| `OCTO_WORKING_DIR` | `$PWD` | Sandbox 工作目录 |
| `OCTO_CORS_ORIGINS` | 空 | CORS 来源（逗号分隔） |
| `OCTO_ENABLE_EVENT_BUS` | `false` | 启用 EventBus |
| `OCTO_AUTH_MODE` | `none` | 认证模式 (none/api_key/full) |
| `OCTO_API_KEY` | - | 服务端 API key |
| `OCTO_VAULT_PASSWORD` | - | Vault 主密码 |

### 8.2 TLS 变量

| 变量 | 默认值 | 说明 |
|------|--------|------|
| `OCTO_TLS_ENABLED` | `false` | 启用 TLS |
| `OCTO_TLS_CERT_PATH` | - | 证书路径 |
| `OCTO_TLS_KEY_PATH` | - | 私钥路径 |
| `OCTO_TLS_SELF_SIGNED` | `false` | 自签名证书 |

### 8.3 Provider 变量

| 变量 | 说明 |
|------|------|
| `LLM_PROVIDER` | 默认 provider (anthropic/openai) |
| `ANTHROPIC_API_KEY` | Anthropic API key |
| `ANTHROPIC_BASE_URL` | Anthropic 代理 URL |
| `ANTHROPIC_MODEL_NAME` | Anthropic 模型覆盖 |
| `OPENAI_API_KEY` | OpenAI API key |
| `OPENAI_BASE_URL` | OpenAI 代理 URL |
| `OPENAI_MODEL_NAME` | OpenAI 模型覆盖 |

---

## 九、配置诊断命令

### `octo config show`（已有，需增强）

```bash
$ octo config show
Configuration Sources:
  1. Global:  ~/.octo/config.yaml (found)
  2. Project: /Users/foo/myproject/.octo/config.yaml (found)
  3. Local:   /Users/foo/myproject/.octo/config.local.yaml (not found)
  4. Env:     3 OCTO_* variables set

Effective Configuration:
  server.host: 127.0.0.1 (from: global)
  server.port: 4000 (from: project)
  provider.name: anthropic (from: global)
  provider.model: qwen3.5-27b (from: project)
  logging.level: octo_engine=debug (from: env OCTO_LOG)
  database.path: ~/.octo/projects/.../octo.db (from: OctoRoot)

Credentials:
  anthropic: sk-ant-...xxxx (from: env ANTHROPIC_API_KEY)
  openai: sk-...xxxx (from: ~/.octo/credentials.yaml)
```

### `octo config validate`（已有，需增强）

```bash
$ octo config validate
✅ Global config valid
✅ Project config valid
⚠️  config.yaml found at project root (legacy location)
    → Move to .octo/config.yaml
✅ Credentials: anthropic configured
❌ Credentials: openai not configured (needed by provider_chain)
✅ Database path: ~/.octo/projects/.../octo.db (writable)
✅ Skills dirs: 2 directories configured
```

---

## 十、实施计划

### Phase AA 任务分解

| 任务 | 内容 | 估计 |
|------|------|------|
| AA-T1 | OctoRoot 新增路径方法（project_local_config, credentials_path, tls_dir, mcp dirs, eval_config） | 小 |
| AA-T2 | Config::load() 改造 — 三层配置合并 + OctoRoot 集成 + 向后兼容 | 中 |
| AA-T3 | credentials.yaml 加载 + 凭证注入 | 中 |
| AA-T4 | 硬编码路径修复（TLS → tls_dir(), certs → tls_dir(), eval.toml → OctoRoot） | 小 |
| AA-T5 | `octo config show/validate` 增强 — 显示配置来源 | 小 |
| AA-T6 | `octo init` 命令 — 项目初始化模板 | 小 |

### 依赖关系

```
AA-T1 → AA-T2 → AA-T3
                  ↗
AA-T4 ──────────
AA-T5 (独立)
AA-T6 (独立，依赖 AA-T1)
```

---

## 十一、决策记录

| 决策 | 选择 | 理由 |
|------|------|------|
| 配置格式 | YAML（保持不变） | 已有大量 YAML 配置，切换成本高；YAML 支持注释 |
| 合并策略 | 字段级浅合并 | 简单可预测，与 Cargo/Git 一致 |
| credentials 格式 | YAML | 与 config.yaml 一致，减少认知负担 |
| --config 行为 | 替代自动发现 | 与 Cargo --config 一致，显式指定 = 完全控制 |
| XDG 支持 | 暂不支持，保持 ~/.octo/ | 简化实现，未来可通过 OCTO_GLOBAL_ROOT 指向 XDG 路径 |
| .env 文件 | 保留向后兼容 | 大量用户已使用 .env，渐进迁移 |
| 旧 config.yaml 位置 | 兼容读取 + 迁移提示 | 平滑过渡，不破坏现有用户 |
