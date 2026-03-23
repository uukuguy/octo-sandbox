# Phase AD — 沙箱容器镜像增强（Container Image Enhancement）

> 增强 base 镜像的企业级工具覆盖，重构 dev 镜像为全功能智能体开发环境，实现多平台构建与 CI/CD 流水线。
> 对应 Deferred: AC-D1（CI/CD 镜像构建）+ 企业级工具需求

## 背景

Phase AC 建立了 `container/` 目录结构和基础的 `octo-sandbox:base` / `octo-sandbox:dev` 两层镜像。但当前镜像存在三个短板：

1. **Base 镜像缺少企业级工具** — 无文档处理（PDF/Word/Excel/PPT）、无数据库客户端、网络诊断工具不足
2. **Dev 镜像缺少智能体开发能力** — 无 MCP SDK、无 Octo CLI 集成、无 LLM SDK、无性能分析工具
3. **构建仍是单平台** — 缺少 buildx 多平台支持和 CI/CD 自动构建

## 设计约束

1. **Base 镜像体积控制在 ~850MB 以内** — 不预装 LibreOffice（+400MB）和云 CLI（+300MB）
2. **Dev 镜像合并为一个** — 不区分 dev 和 agent-dev（开发场景无干净边界）
3. **不在容器内运行模型** — 模型通过宿主机服务提供，容器内仅需客户端 SDK
4. **向后兼容** — 所有改动在 Dockerfile/scripts 内部，不改变 Rust 引擎代码
5. **双架构支持** — 所有新增工具必须同时支持 amd64 和 arm64

## 基线

- **Tests**: 2467 @ commit `b623239`
- **Branch**: `main`
- **前置 Phase**: AC（沙箱容器）已完成

---

## Task 分组

### G1: Base 镜像企业级工具增强

#### AD-T1: 文档处理工具集

**目标**: 为 base 镜像增加 PDF/Word/Excel/PPT/OCR 处理能力

**产出文件**: `container/Dockerfile`, `container/scripts/requirements-base.txt`

**系统包新增**:
```
poppler-utils        # pdftotext, pdfinfo, pdfimages
pandoc               # 万能文档格式转换
tesseract-ocr        # OCR 引擎
tesseract-ocr-chi-sim  # 中文简体 OCR
tesseract-ocr-chi-tra  # 中文繁体 OCR
```

**Python 包新增** (requirements-base.txt):
```
pymupdf>=1.24        # PDF 精确解析
python-docx>=1.1     # Word (.docx)
openpyxl>=3.1        # Excel (.xlsx)
python-pptx>=1.0     # PowerPoint (.pptx)
chardet>=5.2         # 编码检测
tabulate>=0.9        # 表格格式化
markitdown>=0.1      # Office/PDF/HTML → Markdown
```

**验收标准**:
- [ ] `docker build` 成功
- [ ] 容器内 `pdftotext --version` 可用
- [ ] 容器内 `tesseract --version` 可用且支持 chi_sim
- [ ] 容器内 `python3 -c "import pymupdf, docx, openpyxl, pptx"` 成功
- [ ] 镜像大小增量 < 200MB

#### AD-T2: 数据库客户端与网络工具

**目标**: 为 base 镜像增加数据库 CLI 和网络诊断工具

**产出文件**: `container/Dockerfile`

**系统包新增**:
```
postgresql-client     # psql
default-mysql-client  # mysql CLI
sqlite3              # sqlite3 CLI
dnsutils             # dig, nslookup
netcat-openbsd       # nc
openssl              # TLS 诊断（可能已有）
zip unzip            # 压缩/解压
file                 # MIME 类型检测
tree                 # 目录结构可视化
```

**验收标准**:
- [ ] `docker build` 成功
- [ ] 容器内 `psql --version`, `mysql --version`, `sqlite3 --version` 可用
- [ ] 容器内 `dig`, `nc`, `openssl version` 可用
- [ ] 镜像大小增量 < 30MB

### G2: Dev 镜像智能体开发增强

#### AD-T3: MCP + LLM + Skill 开发工具链

**目标**: 将 dev 镜像升级为全功能智能体工具开发环境

**产出文件**: `container/Dockerfile.dev`, `container/scripts/requirements-dev.txt`

**Python 包新增** (requirements-dev.txt):
```
# MCP 开发
mcp>=1.0             # 官方 Python MCP SDK
fastmcp>=2.0         # 快速 MCP server 开发框架
httpx-sse>=0.4       # SSE 客户端

# LLM 调试
tiktoken>=0.7        # OpenAI tokenizer
anthropic>=0.40      # Anthropic SDK
openai>=1.50         # OpenAI SDK
litellm>=1.50        # 统一 LLM API
ollama>=0.4          # Ollama 客户端

# Skill 开发
yamllint>=1.35       # YAML 校验
jsonschema>=4.21     # JSON Schema 验证

# 本地模型客户端 (CPU-only)
sentence-transformers>=3.0
transformers>=4.45
onnxruntime>=1.18
```

**Node.js 包新增** (Dockerfile.dev):
```
@modelcontextprotocol/sdk
@modelcontextprotocol/inspector
```

**PyTorch CPU-only 安装**（Dockerfile.dev 内）:
```dockerfile
RUN pip3 install --no-cache-dir --break-system-packages \
    torch --index-url https://download.pytorch.org/whl/cpu
```

**Rust 组件新增**:
```
rust-analyzer, clippy, rustfmt  # 开发体验
wasm-pack                       # WASM 开发
wasm32-wasip1 target            # WASI 编译
```

**系统工具新增**:
```
strace, ltrace, tcpdump         # 调试工具
```

**Cargo 安装**:
```
flamegraph                      # Rust 火焰图
```

**验收标准**:
- [ ] `docker build -f Dockerfile.dev` 成功
- [ ] 容器内 `python3 -c "import mcp, fastmcp, anthropic, openai, litellm, torch"` 成功
- [ ] 容器内 `npx @modelcontextprotocol/inspector --version` 可用
- [ ] 容器内 `wasm-pack --version` 可用
- [ ] 容器内 `rust-analyzer --version` 可用
- [ ] torch 确认为 CPU 版本（无 CUDA）
- [ ] dev 镜像总大小 < 2.5GB

### G3: 多平台构建与 CI/CD

#### AD-T4: GitHub Actions 构建流水线

**目标**: 实现多平台自动构建，推送到 GHCR

**产出文件**: `.github/workflows/container-build.yml`

**设计**:
- 触发条件: `container/**` 路径变更推送到 main，或手动触发
- 构建平台: `linux/amd64,linux/arm64`
- 推送目标: `ghcr.io/uukuguy/octo-sandbox:{base,dev}`
- 缓存策略: `type=gha` (GitHub Actions 缓存)
- 构建顺序: base → dev（dev 依赖 base）

**验收标准**:
- [ ] workflow 文件语法正确（`act` 或 dry-run 验证）
- [ ] base 和 dev 镜像分阶段构建
- [ ] PR 场景不推送，仅构建验证
- [ ] main 推送场景自动推送到 GHCR

#### AD-T5: Makefile 与 CLI 集成

**目标**: 更新 Makefile 和 `octo sandbox build` 支持新的构建选项

**产出文件**: `Makefile`, `crates/octo-cli/src/commands/sandbox.rs`

**Makefile 新增目标**:
```makefile
container-build          # 本地构建 base
container-build-dev      # 本地构建 dev
container-build-multi    # 多平台构建 base + 推送
container-build-multi-dev # 多平台构建 dev + 推送
```

**CLI 增强**:
- `octo sandbox build --dev` — 构建 dev 镜像
- `octo sandbox build --multi-platform` — 多平台构建（需 buildx）

**验收标准**:
- [ ] `make container-build` 成功（需 Docker daemon）
- [ ] `cargo check --workspace` 通过
- [ ] `cargo test --workspace -- --test-threads=1` 全部通过

---

## Deferred（暂缓项）

> 本阶段已知但暂未实现的功能点。每次开始新 Task 前先检查此列表。

| ID | 内容 | 前置条件 | 状态 |
|----|------|---------|------|
| AD-D1 | LibreOffice headless 集成（octo-sandbox:base-office 变体） | 用户需求验证 | ⏳ |
| AD-D2 | 云 CLI 变体镜像（base-aws, base-gcp, base-azure） | AC-D4 多镜像支持 | ⏳ |
| AD-D3 | cosign 镜像签名 | CI/CD 流水线稳定 | ⏳ |
| AD-D4 | 容器内 Octo CLI 编译安装（vs 宿主机挂载） | CI 场景需求 | ⏳ |
| AD-D5 | OCTO_MODEL_API_URL 环境变量注入 | AB-D5 CredentialResolver 完成 | ⏳ |
| AD-D6 | Docling（IBM 文档智能解析）集成评估 | 企业用户需求验证 | ⏳ |

---

## 风险与注意事项

1. **tesseract OCR 语言包体积** — chi_sim + chi_tra 各 ~20MB，如需更多语言包会膨胀；可用环境变量控制安装哪些语言
2. **PyTorch CPU-only 体积** — ~200MB，是 dev 镜像增量最大项；但原型验证必需
3. **ARM64 构建速度** — GitHub Actions 用 QEMU 模拟 ARM64，构建 dev 镜像可能 30-45 分钟；可用 ARM runner 加速
4. **MCP Inspector 依赖** — 可能需要 Chromium/headless browser；如体积过大可降级为 CLI 模式
5. **向后兼容** — 本阶段仅改 Dockerfile 和 scripts，不改 Rust 代码，零测试回归风险

## 验证标准

- [ ] `docker build -t octo-sandbox:base container/` 成功
- [ ] `docker build -f container/Dockerfile.dev -t octo-sandbox:dev container/` 成功
- [ ] 多平台构建 `--platform linux/amd64,linux/arm64` 成功
- [ ] `cargo check --workspace` 通过
- [ ] `cargo test --workspace -- --test-threads=1` 全部通过，无回归
- [ ] base 镜像 < 900MB
- [ ] dev 镜像 < 2.5GB
- [ ] GitHub Actions workflow 语法验证通过

## 依赖关系

```
AD-T1 (文档处理) ──────┐
                        ├─► 独立，可并行
AD-T2 (DB/网络工具) ───┘

AD-T3 (Dev 智能体工具) ─────► 独立于 G1

AD-T4 (CI/CD) ─────────────► 依赖 T1+T2（base 镜像内容确定后）
AD-T5 (Makefile/CLI) ──────► 依赖 T4（workflow 确定后）
```

**并行可能性**:
- G1 (T1, T2) 和 G2 (T3) 完全可并行
- G3 (T4, T5) 在 G1+G2 完成后顺序执行
