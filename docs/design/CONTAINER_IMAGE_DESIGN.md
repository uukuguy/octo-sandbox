# Octo 沙箱容器镜像设计方案

**版本**: v1.0
**创建日期**: 2026-03-23
**状态**: Phase AD 设计阶段
**前置**: Phase AC（沙箱容器）已完成

---

## 一、设计目标

Octo 定位为**企业级自主智能体平台**。沙箱容器镜像是智能体工具执行的运行时环境，需要满足：

1. **企业级工具覆盖** — 文档处理、数据库交互、网络诊断等企业常用能力
2. **多平台支持** — linux/amd64 + linux/arm64 双架构
3. **智能体开发调试** — MCP SDK、Octo CLI、LLM SDK、性能分析等开发工具
4. **本地模型集成** — 通过宿主机服务提供 embedding/推理能力，容器内仅需客户端 SDK

---

## 二、镜像体系架构

### 2.1 两层镜像（不再是三层）

```
octo-sandbox:base       ← 生产运行时（~850MB）
    │
    └─► octo-sandbox:dev ← 全功能开发（~2GB）
```

**设计决策**：将原计划的 `dev` 和 `agent-dev` 合并为一个 `dev` 镜像。

**理由**：
- 开发者使用场景没有干净的边界（需要 Rust 的人大概率也需要 MCP SDK）
- dev 镜像是低频拉取场景，体积不敏感
- 减少维护成本，避免组合爆炸

### 2.2 镜像用途矩阵

| 镜像 | 用途 | 使用方式 | 体积目标 |
|------|------|---------|---------|
| `base` | 生产环境工具执行 | `SessionSandboxManager` 自动管理 | ~850MB |
| `dev` | Skill/MCP/扩展开发调试 | 开发者手动或 CI 使用 | ~2GB |

### 2.3 多镜像变体（未来 AC-D4）

```
octo-sandbox:base           ← 通用生产环境
octo-sandbox:base-aws       ← + AWS CLI
octo-sandbox:base-gcp       ← + GCP CLI
octo-sandbox:base-azure     ← + Azure CLI
octo-sandbox:dev            ← 全功能开发
```

云 CLI 不预装在 base 中的原因：
1. 体积大（AWS CLI v2 ~300MB）
2. 安全——默认容器不应有云操作能力
3. 不同企业用不同云，没有"通用"方案

---

## 三、Base 镜像设计

### 3.1 当前工具集（Phase AC 已实现）

| 类别 | 工具 | 来源 |
|------|------|------|
| 基础系统 | git, curl, wget, jq, openssh | apt-get |
| Python 运行时 | python3, pip3, venv | apt-get |
| 现代 CLI | rg, fd, bat, eza, dust, delta, sd, xh, tokei | GitHub Releases |
| GitHub CLI | gh | GitHub Releases |
| Python 包 | requests, httpx, aiohttp, pydantic, rich, click, typer, pytest, ruff | pip3 |

### 3.2 企业级工具增强（Phase AD 新增）

#### 3.2.1 文档处理

**系统包**：

| 工具 | 用途 | 体积估算 |
|------|------|---------|
| `poppler-utils` | PDF 文本提取（pdftotext, pdfinfo, pdfimages） | ~5MB |
| `pandoc` | 万能文档格式转换（md/docx/html/pdf/rst） | ~30MB |
| `tesseract-ocr` | OCR 文字识别引擎 | ~15MB |
| `tesseract-ocr-chi-sim` | 中文简体 OCR 语言包 | ~20MB |
| `tesseract-ocr-chi-tra` | 中文繁体 OCR 语言包 | ~20MB |
| `tesseract-ocr-eng` | 英文 OCR 语言包（通常预装） | ~5MB |

**Python 包**（requirements-base.txt 追加）：

| 包 | 用途 |
|----|------|
| `pymupdf>=1.24` | PDF 精确解析（文字+图片+表格+元数据） |
| `python-docx>=1.1` | Word (.docx) 结构化读写 |
| `openpyxl>=3.1` | Excel (.xlsx) 结构化读写 |
| `python-pptx>=1.0` | PowerPoint (.pptx) 读写 |
| `chardet>=5.2` | 文件编码检测 |
| `tabulate>=0.9` | 表格格式化输出 |
| `markitdown>=0.1` | Microsoft 出品，Office/PDF/HTML → Markdown 转换 |

**不预装 LibreOffice 的理由**：
- 体积 +400MB（几乎翻倍）
- `pymupdf` + `python-docx` + `openpyxl` 已覆盖 95% 企业文档处理需求
- 如需 LibreOffice，通过多镜像变体 `octo-sandbox:base-office` 提供

#### 3.2.2 数据库客户端

| 工具 | 用途 | 体积估算 |
|------|------|---------|
| `postgresql-client` | psql CLI | ~5MB |
| `default-mysql-client` | mysql CLI | ~10MB |
| `sqlite3` | sqlite3 CLI（Python sqlite3 模块已有，这是独立 CLI） | ~1MB |

#### 3.2.3 网络与系统工具

| 工具 | 用途 | 体积估算 |
|------|------|---------|
| `dnsutils` | dig, nslookup | ~2MB |
| `netcat-openbsd` | nc（网络调试） | ~1MB |
| `openssl` | TLS/SSL 诊断 | ~3MB |
| `zip` + `unzip` | 压缩/解压 | ~1MB |
| `file` | 文件类型检测（MIME type） | ~1MB |
| `tree` | 目录结构可视化 | ~1MB |

### 3.3 镜像大小预估

| 层 | 当前 (AC) | 增强后 (AD) | 增量 |
|---|---|---|---|
| node:22-slim 基础 | ~200MB | ~200MB | 0 |
| 系统包 + Python | ~150MB | ~150MB | 0 |
| CLI tools | ~100MB | ~100MB | 0 |
| Python 基础包 | ~100MB | ~100MB | 0 |
| **文档处理（系统）** | 0 | ~95MB | +95MB |
| **文档处理（Python）** | 0 | ~60MB | +60MB |
| **DB 客户端** | 0 | ~16MB | +16MB |
| **网络/系统工具** | 0 | ~9MB | +9MB |
| **总计** | ~550MB | ~730MB | **+180MB** |

---

## 四、Dev 镜像设计

### 4.1 当前工具集（Phase AC 已实现）

| 类别 | 工具 |
|------|------|
| C/C++ 编译 | build-essential, pkg-config, libssl-dev |
| Rust 工具链 | rustup (stable), cargo |
| Python 数据科学 | numpy, pandas, matplotlib, scikit-learn |
| Python 代码质量 | black, mypy, isort, types-requests, types-pyyaml |
| 调试工具 | ipython, ipdb |
| 文档生成 | sphinx, mkdocs |

### 4.2 智能体开发增强（Phase AD 新增）

#### 4.2.1 MCP 工具开发套件

**Python MCP SDK**（requirements-dev.txt 追加）：

| 包 | 用途 |
|----|------|
| `mcp>=1.0` | 官方 Python MCP SDK |
| `fastmcp>=2.0` | 快速 MCP server 开发框架 |
| `httpx-sse>=0.4` | SSE 客户端（调试 streamable HTTP transport） |

**Node.js MCP SDK**（npm install -g）：

| 包 | 用途 |
|----|------|
| `@modelcontextprotocol/sdk` | 官方 TypeScript MCP SDK |
| `@modelcontextprotocol/inspector` | MCP Inspector 可视化调试工具 |

#### 4.2.2 Skill 开发环境

| 包/工具 | 用途 |
|---------|------|
| `yamllint>=1.35` | YAML 格式校验 |
| `jsonschema>=4.21` | JSON Schema 验证（skill manifest） |

#### 4.2.3 LLM 开发调试工具

| 包 | 用途 |
|----|------|
| `tiktoken>=0.7` | OpenAI tokenizer（token 计数） |
| `anthropic>=0.40` | Anthropic SDK（直接调试 API） |
| `openai>=1.50` | OpenAI SDK |
| `litellm>=1.50` | 统一 LLM API（测试多 provider） |
| `ollama>=0.4` | Ollama Python 客户端 |

#### 4.2.4 本地模型客户端（CPU-only）

| 包 | 用途 | 备注 |
|----|------|------|
| `sentence-transformers>=3.0` | 本地 embedding 开发/测试 | CPU 足够做原型验证 |
| `transformers>=4.45` | Hugging Face 模型加载 | 原型验证用 |
| `torch` (CPU-only) | PyTorch CPU 版本 | ~200MB（vs GPU 版 ~2GB） |
| `onnxruntime>=1.18` | ONNX 推理引擎 | 轻量级本地推理 |

**安装方式**（避免拉 CUDA）：
```bash
pip3 install torch --index-url https://download.pytorch.org/whl/cpu
```

#### 4.2.5 WASM 开发工具链

| 工具 | 用途 |
|------|------|
| `wasm-pack` | Rust → WASM 编译 |
| `wasm32-wasip1` target | WASI 编译目标 |

#### 4.2.6 Rust 开发体验

| 组件 | 用途 |
|------|------|
| `rust-analyzer` | LSP 语言服务 |
| `clippy` | Linter |
| `rustfmt` | Formatter |

#### 4.2.7 调试与性能分析

**系统工具**：

| 工具 | 用途 |
|------|------|
| `strace` | 系统调用跟踪 |
| `ltrace` | 库函数调用跟踪 |
| `tcpdump` | 网络包抓取 |

**Rust 工具**：

| 工具 | 用途 |
|------|------|
| `flamegraph` | Rust 性能火焰图（cargo install） |

### 4.3 Octo CLI 集成

Dev 镜像中的 Octo CLI 通过**宿主机挂载**方式提供，而非容器内编译：

```bash
# 使用方式：启动 dev 容器时挂载宿主机编译好的二进制
docker run -v $(pwd)/target/release/octo:/usr/local/bin/octo octo-sandbox:dev
```

这样 Octo CLI 始终与宿主机版本一致，无需在容器内维护独立的编译流程。

---

## 五、多平台构建方案

### 5.1 构建工具

使用 Docker Buildx（BuildKit）实现多平台构建。

### 5.2 本地构建

```bash
# 创建 buildx builder（一次性）
docker buildx create --name octo-builder --use --bootstrap

# 多平台构建 + 推送到 registry
docker buildx build \
  --platform linux/amd64,linux/arm64 \
  --tag ghcr.io/uukuguy/octo-sandbox:base \
  --tag ghcr.io/uukuguy/octo-sandbox:base-$(git rev-parse --short HEAD) \
  --push \
  container/

# 仅本地加载（单平台，用于本地测试）
docker buildx build \
  --platform linux/$(uname -m | sed 's/x86_64/amd64/;s/aarch64/arm64/') \
  --tag octo-sandbox:base \
  --load \
  container/
```

### 5.3 CI/CD 构建（GitHub Actions）

```yaml
name: Build Container Images
on:
  push:
    paths: ['container/**']
    branches: [main]
  pull_request:
    paths: ['container/**']
  workflow_dispatch:
    inputs:
      push_to_registry:
        description: 'Push to GHCR'
        type: boolean
        default: false

jobs:
  build-base:
    runs-on: ubuntu-latest
    permissions:
      contents: read
      packages: write
    steps:
      - uses: actions/checkout@v4
      - uses: docker/setup-qemu-action@v3
      - uses: docker/setup-buildx-action@v3
      - uses: docker/login-action@v3
        with:
          registry: ghcr.io
          username: ${{ github.actor }}
          password: ${{ secrets.GITHUB_TOKEN }}
      - uses: docker/build-push-action@v6
        with:
          context: container
          file: container/Dockerfile
          platforms: linux/amd64,linux/arm64
          push: ${{ github.event_name == 'push' && github.ref == 'refs/heads/main' }}
          tags: |
            ghcr.io/uukuguy/octo-sandbox:base
            ghcr.io/uukuguy/octo-sandbox:base-${{ github.sha }}
          cache-from: type=gha
          cache-to: type=gha,mode=max

  build-dev:
    needs: build-base
    runs-on: ubuntu-latest
    permissions:
      contents: read
      packages: write
    steps:
      - uses: actions/checkout@v4
      - uses: docker/setup-qemu-action@v3
      - uses: docker/setup-buildx-action@v3
      - uses: docker/login-action@v3
        with:
          registry: ghcr.io
          username: ${{ github.actor }}
          password: ${{ secrets.GITHUB_TOKEN }}
      - uses: docker/build-push-action@v6
        with:
          context: container
          file: container/Dockerfile.dev
          platforms: linux/amd64,linux/arm64
          push: ${{ github.event_name == 'push' && github.ref == 'refs/heads/main' }}
          tags: |
            ghcr.io/uukuguy/octo-sandbox:dev
            ghcr.io/uukuguy/octo-sandbox:dev-${{ github.sha }}
          cache-from: type=gha
          cache-to: type=gha,mode=max
```

### 5.4 架构兼容性注意事项

| 组件 | amd64 | arm64 | 备注 |
|------|-------|-------|------|
| node:22-slim | OK | OK | 官方多平台镜像 |
| Rust CLI tools | OK | OK | install-cli-tools.sh 已处理 |
| poppler-utils | OK | OK | apt 原生包 |
| tesseract-ocr | OK | OK | apt 原生包 |
| pandoc | OK | OK | apt 原生包 |
| pymupdf | OK | OK | pip wheel 支持双平台 |
| torch (CPU) | OK | OK | pip 有 aarch64 wheel |
| wasm-pack | OK | OK | 预编译二进制支持 |

---

## 六、本地模型集成方案

### 6.1 架构原则

**模型在宿主机运行，容器内仅调用 API**。

```
┌─────────────────────────────────────────────────┐
│                 宿主机 / 集群                     │
│                                                   │
│  ┌─────────────┐    ┌──────────────────────────┐ │
│  │ Octo Engine  │───►│ 本地模型服务              │ │
│  │ (AgentLoop)  │    │ ├─ Ollama (llama3/qwen)  │ │
│  │              │    │ ├─ vLLM (Qwen2.5-7B)     │ │
│  │              │    │ └─ TEI (embedding)        │ │
│  └──────┬───────┘    └──────────────────────────┘ │
│         │ docker exec                              │
│  ┌──────▼───────┐                                  │
│  │ octo-sandbox  │  容器内：httpx/ollama 客户端     │
│  │ :base         │  需要模型时 → 调宿主机 API       │
│  └──────────────┘                                  │
└─────────────────────────────────────────────────┘
```

**不在容器内跑模型的理由**：
1. GPU 透传复杂（nvidia-container-toolkit + CUDA runtime +3GB）
2. 每个 session 容器加载模型会内存爆炸
3. 与 ProviderChain 冲突——本地模型应走统一 provider 通道
4. 安全隔离——沙箱容器不应持有 GPU 设备访问权限

### 6.2 ProviderChain 配置本地模型

```yaml
# config.yaml — 通过已有 ProviderChain 机制，无需改代码
providers:
  - name: local-embedding
    provider: openai            # OpenAI-compatible
    base_url: "http://localhost:11434/v1"  # Ollama
    model: "nomic-embed-text"
    api_key: "not-needed"

  - name: local-llm
    provider: openai
    base_url: "http://localhost:11434/v1"
    model: "qwen2.5:7b"
    api_key: "not-needed"
```

### 6.3 沙箱容器内调用模型

`SessionSandboxManager` 创建容器时需注入模型服务地址（依赖 AB-D5 CredentialResolver）：

```rust
// 未来实现：容器环境变量注入
fn container_env_vars(&self) -> Vec<String> {
    vec![
        format!("OCTO_MODEL_API_URL=http://host.docker.internal:{}", self.model_api_port),
    ]
}
```

容器内的 skill/工具通过环境变量访问模型：

```python
import httpx, os

resp = httpx.post(
    f"{os.environ['OCTO_MODEL_API_URL']}/v1/embeddings",
    json={"input": "文档内容...", "model": "nomic-embed-text"}
)
embedding = resp.json()["data"][0]["embedding"]
```

### 6.4 推荐本地模型矩阵

| 用途 | 推荐模型 | 参数量 | 显存需求 | 部署方式 |
|------|---------|--------|---------|---------|
| Embedding | `bge-m3` (BAAI) | 568M | 2GB | Ollama / TEI |
| Embedding | `nomic-embed-text` | 137M | 1GB | Ollama |
| Embedding | `jina-embeddings-v3` | 570M | 2GB | TEI |
| 中文推理 | `Qwen2.5-7B-Instruct` | 7B | 6GB (Q4) | Ollama / vLLM |
| 代码 | `Qwen2.5-Coder-7B` | 7B | 6GB (Q4) | Ollama / vLLM |
| 轻量分类 | `Qwen2.5-3B-Instruct` | 3B | 3GB (Q4) | Ollama |
| Reranker | `bge-reranker-v2-m3` | 568M | 2GB | TEI |
| 英文推理 | `Llama-3.1-8B-Instruct` | 8B | 6GB (Q4) | Ollama / vLLM |

---

## 七、Makefile 集成

```makefile
# ── Container Images ──
container-build:        ## Build octo-sandbox:base image (current platform)
	docker buildx build --load -t octo-sandbox:base container/

container-build-dev:    ## Build octo-sandbox:dev image (current platform)
	docker buildx build --load -t octo-sandbox:dev -f container/Dockerfile.dev container/

container-build-multi:  ## Build multi-platform base image and push to GHCR
	docker buildx build \
		--platform linux/amd64,linux/arm64 \
		--tag ghcr.io/uukuguy/octo-sandbox:base \
		--push container/

container-build-multi-dev:  ## Build multi-platform dev image and push to GHCR
	docker buildx build \
		--platform linux/amd64,linux/arm64 \
		--tag ghcr.io/uukuguy/octo-sandbox:dev \
		--push -f container/Dockerfile.dev container/
```

---

## 八、与现有代码的集成点

| 组件 | 文件 | 改动内容 |
|------|------|---------|
| `ImageRegistry` | `sandbox/docker.rs` | 无需改动，`DEFAULT_SANDBOX_IMAGE` 仍为 `octo-sandbox:base` |
| `SessionSandboxManager` | `sandbox/session_sandbox.rs` | 无需改动，仅 Dockerfile 内容变化 |
| `octo sandbox build` | `commands/sandbox.rs` | 增加 `--multi-platform` 选项 |
| Makefile | `Makefile` | 新增 container-build 系列目标 |
| CI/CD | `.github/workflows/` | 新增 container-build.yml |

---

## 九、安全考量

1. **非 root 执行**：`sandbox` 用户 UID 1000（已实现）
2. **文档处理安全**：pymupdf 运行在非 root 用户下，处理不受信任的 PDF 不会影响系统
3. **OCR 安全**：tesseract 是纯 CPU 计算，无网络访问
4. **DB 客户端**：仅安装 CLI 工具，不包含服务端；连接凭据通过 CredentialResolver 注入
5. **镜像签名**（未来 AC-D1 扩展）：使用 cosign 对推送到 GHCR 的镜像签名

---

## 十、Deferred 项更新

本设计将以下 Deferred 项纳入实施范围：

| ID | 内容 | 本阶段处理 |
|----|------|-----------|
| AC-D1 | CI/CD 镜像构建流水线 | AD-T4: GitHub Actions workflow |
| AC-D4 | 多镜像支持 | 设计预留，不在本阶段实施 |

以下 Deferred 项保持挂起：

| ID | 内容 | 原因 |
|----|------|------|
| AC-D2 | 容器资源限制 | 独立于镜像内容，属于运行时配置 |
| AC-D3 | 容器网络隔离 | 需安全需求评估 |
| AC-D5 | 文件系统快照/恢复 | 需生产验证 |
| AC-D6 | Docker Compose 编排 | 需多 Agent 协作场景 |
