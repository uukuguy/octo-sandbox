.PHONY: dev build check test clean fmt lint server web all install setup \
        cli cli-run cli-ask cli-agent cli-session cli-config cli-doctor \
        studio studio-tui studio-dashboard build-studio \
        verify verify-runtime verify-api verify-api-mcp \
        eval-list eval-run eval-compare eval-benchmark eval-benchmark-mini \
        eval-history eval-report eval-trace eval-diagnose eval-diff eval-progress \
        sandbox-status sandbox-dry-run sandbox-backends \
        sandbox-dev sandbox-staging sandbox-production sandbox-shell \
        container-build container-build-dev container-build-multi container-build-multi-dev \
        container-list container-clean container-test \
        docker-build docker-build-python docker-build-rust docker-build-nodejs \
        docker-build-bash docker-build-general docker-build-swebench docker-list docker-clean \
        claude-runtime-setup claude-runtime-proto claude-runtime-test claude-runtime-start \
        claude-runtime-build claude-runtime-run verify-dual-runtime

# Default test project for CLI commands
TEST_PROJECT ?= $(PWD)/examples/demo-project

# ============================================================
# 主要命令
# ============================================================

# 同时启动后端 + 前端开发服务器
dev:
	@echo "Starting backend and frontend..."
	@$(MAKE) -j2 server web

# 完整构建 (后端 + 前端)
all: build web-build

# 首次安装依赖
install: setup

setup:
	cd web && npm install

# ============================================================
# 后端命令
# ============================================================

# 生成默认配置文件 (config.yaml)
config-gen:
	cargo run -p grid-server -- config-gen > config.yaml

# 编译检查 (最快, 不生成二进制)
check:
	cargo check --workspace

autofix:
	cargo fix --workspace --allow-dirty --allow-staged

# 编译构建 (日常开发, 不含 WASM/Docker/PDF)
build:
	cargo build

# 完整构建 (含 WASM/Docker/PDF 等全部功能)
build-full:
	cargo build --features full

build-cli:
	cargo build -p grid-cli --bin grid

build-studio:
	cargo build -p grid-cli --features studio

build-cli-full:
	cargo build -p grid-cli --features full

# 编译构建 (release, 含全部功能)
release:
	cargo build --release --features full

# 运行后端服务器 (exec ensures Ctrl+C reaches the server directly)
server:
	@exec cargo run -p grid-server

# 运行测试
test:
	cargo test --workspace

# 单个 crate 测试
test-types:
	cargo test -p grid-types

test-engine:
	cargo test -p grid-engine

test-sandbox:
	cargo test -p grid-sandbox

test-server:
	cargo test -p grid-server

# 代码格式化
fmt:
	cargo fmt --all

# 格式化检查 (CI 用)
fmt-check:
	cargo fmt --all -- --check

# Lint
lint:
	cargo clippy --workspace -- -D warnings

# 编译时间分析 (生成 HTML 报告)
timings:
	cargo build --timings

# ============================================================
# 前端命令
# ============================================================

# 前端开发服务器
web:
	cd web && npm run dev

# 前端生产构建
web-build:
	cd web && npm run build

# 前端类型检查
web-check:
	cd web && npx tsc --noEmit

# 前端 lint
web-lint:
	cd web && npx eslint src/

# ============================================================
# 清理命令
# ============================================================

# 清理后端构建产物
clean:
	cargo clean

# 清理前端构建产物
clean-web:
	cd web && rm -rf node_modules dist .vite

# 清理全部
clean-all: clean clean-web

# ============================================================
# CLI 命令 (grid-cli)
# ============================================================

CLI_ARGS ?=
QUERY    ?=

# 显示 CLI 帮助
cli:
	cargo run -p grid-cli --bin grid -- --help

# 交互式 REPL 会话
cli-run:
	@cargo run --quiet -p grid-cli --bin grid -- --project $(TEST_PROJECT) run $(CLI_ARGS)

# 单次提问 (headless 模式)
# 用法: make cli-ask QUERY="你的问题"
cli-ask:
	@if [ -z "$(QUERY)" ]; then echo "Usage: make cli-ask QUERY=\"your question\""; exit 1; fi
	@cargo run --quiet -p grid-cli --bin grid -- --project $(TEST_PROJECT) ask "$(QUERY)" $(CLI_ARGS)

# ============================================================
# Studio 命令 (grid-studio = TUI + Dashboard, 需要 studio feature)
# ============================================================

# TUI 全屏模式
studio-tui: build-studio
	@if [ -f target/debug/grid-studio ]; then \
		target/debug/grid-studio --project $(TEST_PROJECT) $(CLI_ARGS); \
	else \
		cargo run --quiet -p grid-cli --features studio --bin grid-studio -- --project $(TEST_PROJECT) $(CLI_ARGS); \
	fi

# 启动 Web Dashboard
studio-dashboard: build-studio
	@cargo run --quiet -p grid-cli --features studio --bin grid-studio -- --project $(TEST_PROJECT) dashboard $(CLI_ARGS)

# 别名: studio = studio-tui
studio: studio-tui

# Agent 管理
cli-agent:
	cargo run -p grid-cli --bin grid -- --project $(TEST_PROJECT) agent list

# Session 管理
cli-session:
	cargo run -p grid-cli --bin grid -- --project $(TEST_PROJECT) session list

# 配置管理
cli-config:
	cargo run -p grid-cli --bin grid -- --project $(TEST_PROJECT) config show

# 健康诊断
cli-doctor:
	cargo run -p grid-cli --bin grid -- --project $(TEST_PROJECT) doctor

# ============================================================
# 评估命令 (grid-eval)
# 注意: 所有命令从 workspace 根目录运行，输出写入 eval_output/
# ============================================================

EVAL_CONFIG     ?= config/eval/benchmark.toml
EVAL_MINI_CONFIG ?= config/eval/benchmark.toml
EVAL_SUITE      ?= tool_call
EVAL_MAX_TASKS  ?= 0
EVAL_FORMAT     ?= both
EVAL_RUN_ID     ?=
EVAL_TASK_ID    ?=

# 列出可用 suite
eval-list:
	cargo run -p grid-eval -- list-suites

# 运行单个 suite（单模型）
# 用法: make eval-run EVAL_SUITE=resilience
eval-run:
	cargo run -p grid-eval -- run --suite $(EVAL_SUITE) \
	  $(if $(filter-out 0,$(EVAL_MAX_TASKS)),--max-tasks $(EVAL_MAX_TASKS),) \
	  --format $(EVAL_FORMAT)

# 多模型对比单个 suite
# 用法: make eval-compare EVAL_SUITE=security EVAL_CONFIG=config/eval/benchmark.toml
eval-compare:
	cargo run -p grid-eval -- compare --suite $(EVAL_SUITE) \
	  --config $(EVAL_CONFIG) \
	  $(if $(filter-out 0,$(EVAL_MAX_TASKS)),--max-tasks $(EVAL_MAX_TASKS),) \
	  --format $(EVAL_FORMAT)

# 完整 benchmark（全部 suite × 全部模型，并发）
# 用法: make eval-benchmark EVAL_CONFIG=config/eval/benchmark.toml
eval-benchmark:
	cargo run -p grid-eval -- benchmark \
	  --config $(EVAL_CONFIG) \
	  $(if $(filter-out 0,$(EVAL_MAX_TASKS)),--max-tasks $(EVAL_MAX_TASKS),) \
	  --format $(EVAL_FORMAT)

# Mini benchmark：每 suite 3 个任务，快速冒烟测试
# 用法: make eval-benchmark-mini
eval-benchmark-mini:
	cargo run -p grid-eval -- benchmark \
	  --config $(EVAL_MINI_CONFIG) \
	  --max-tasks 3 \
	  --format $(EVAL_FORMAT)

# 列出历史运行记录
eval-history:
	cargo run -p grid-eval -- history

# 查看运行报告
# 用法: make eval-report EVAL_RUN_ID=2026-03-16-001
eval-report:
	@if [ -z "$(EVAL_RUN_ID)" ]; then echo "Usage: make eval-report EVAL_RUN_ID=<run_id>"; exit 1; fi
	cargo run -p grid-eval -- report $(EVAL_RUN_ID) --format $(EVAL_FORMAT)

# 查看任务 trace 时间线
# 用法: make eval-trace EVAL_RUN_ID=2026-03-16-001 EVAL_TASK_ID=tc-01
eval-trace:
	@if [ -z "$(EVAL_RUN_ID)" ]; then echo "Usage: make eval-trace EVAL_RUN_ID=<run_id> EVAL_TASK_ID=<task_id>"; exit 1; fi
	@if [ -z "$(EVAL_TASK_ID)" ]; then echo "Usage: make eval-trace EVAL_RUN_ID=<run_id> EVAL_TASK_ID=<task_id>"; exit 1; fi
	cargo run -p grid-eval -- trace $(EVAL_RUN_ID) $(EVAL_TASK_ID)

# 失败原因分类分析
# 用法: make eval-diagnose EVAL_RUN_ID=2026-03-16-001
eval-diagnose:
	@if [ -z "$(EVAL_RUN_ID)" ]; then echo "Usage: make eval-diagnose EVAL_RUN_ID=<run_id>"; exit 1; fi
	cargo run -p grid-eval -- diagnose $(EVAL_RUN_ID)

# 两次运行回归对比
# 用法: make eval-diff EVAL_RUN_A=2026-03-15-001 EVAL_RUN_B=2026-03-16-001
eval-diff:
	@if [ -z "$(EVAL_RUN_A)" ] || [ -z "$(EVAL_RUN_B)" ]; then \
	  echo "Usage: make eval-diff EVAL_RUN_A=<run_a> EVAL_RUN_B=<run_b>"; exit 1; fi
	cargo run -p grid-eval -- diff $(EVAL_RUN_A) $(EVAL_RUN_B)

# 即时进度：查看正在运行的 benchmark 每个 suite/model 的完成情况
# 用法: make eval-progress              (查看 latest 运行)
#       make eval-progress EVAL_RUN_ID=2026-03-16-007
eval-progress:
	@RUN=$$([ -n "$(EVAL_RUN_ID)" ] && echo "eval_output/runs/$(EVAL_RUN_ID)" || readlink -f eval_output/latest 2>/dev/null || echo "eval_output/latest"); \
	echo "=== Benchmark progress: $$RUN ==="; \
	echo ""; \
	echo "--- Suite completion (model_result.json) ---"; \
	for suite in bfcl context gaia resilience security swe_bench tau_bench terminal_bench; do \
	  total=$$(ls "$$RUN/$$suite"/*/model_result.json 2>/dev/null | wc -l | tr -d ' '); \
	  printf "  %-20s %s/4\n" "$$suite" "$$total"; \
	done; \
	echo ""; \
	echo "--- Per-model task progress (tasks_progress.json or traces) ---"; \
	for suite in bfcl context gaia resilience security swe_bench tau_bench terminal_bench; do \
	  for mdir in "$$RUN/$$suite"/*/; do \
	    [ -d "$$mdir" ] || continue; \
	    model=$$(basename "$$mdir"); \
	    if [ -f "$$mdir/model_result.json" ]; then \
	      result=$$(python3 -c "import json; d=json.load(open('$$mdir/model_result.json')); print(f\"{d['total']} tasks done, {d['passed']} passed ({d['pass_rate']*100:.0f}%)\")" 2>/dev/null); \
	      printf "  %-20s %-30s DONE %s\n" "$$suite" "$$model" "$$result"; \
	    elif [ -f "$$mdir/tasks_progress.json" ]; then \
	      result=$$(python3 -c "import json; d=json.load(open('$$mdir/tasks_progress.json')); print(f\"{d['completed']}/{d['total']} tasks, {d['passed']} passed\")" 2>/dev/null); \
	      printf "  %-20s %-30s IN PROGRESS %s\n" "$$suite" "$$model" "$$result"; \
	    else \
	      traces=$$(ls "$$mdir/traces/" 2>/dev/null | wc -l | tr -d ' '); \
	      printf "  %-20s %-30s running (%s traces)\n" "$$suite" "$$model" "$$traces"; \
	    fi; \
	  done; \
	done; \
	echo ""; \
	if [ -f "$$RUN/benchmark.md" ]; then \
	  echo "--- Final benchmark report ---"; \
	  cat "$$RUN/benchmark.md"; \
	fi

# ============================================================
# 手工验证命令 (grid-workbench)
# ============================================================

# 静态验证: 编译检查 + TS 类型 + Vite 生产构建 (无需运行服务)
verify:
	@echo "=== [1/3] Rust 编译检查 ==="
	cargo check --workspace
	@echo ""
	@echo "=== [2/3] TypeScript 类型检查 ==="
	cd web && npx tsc --noEmit
	@echo ""
	@echo "=== [3/3] Vite 生产构建 ==="
	cd web && npm run build
	@echo ""
	@echo "✅ 静态验证全部通过"

# 运行时验证指南 (需先 make server + make web 分两个终端)
verify-runtime:
	@echo "=== grid-workbench 运行时验证步骤 ==="
	@echo ""
	@echo "前置条件:"
	@echo "  1. 确认 .env 已配置 ANTHROPIC_API_KEY"
	@echo "  2. 终端A: make server    (后端, 端口 3001)"
	@echo "  3. 终端B: make web       (前端, 端口 5173)"
	@echo ""
	@echo "核心功能验证清单:"
	@echo ""
	@echo "  [Chat Tab]"
	@echo "  □ 发送消息 → 收到流式回复"
	@echo "  □ 发送消息包含文件路径 → Agent 调用 file_read 工具"
	@echo "  □ 发送 'run: echo hello' → Agent 调用 bash 工具"
	@echo "  □ 连续对话 5+ 轮 → 上下文保持正确"
	@echo ""
	@echo "  [Tools Tab (工具执行历史)]"
	@echo "  □ 工具调用后列表出现新条目"
	@echo "  □ 点击条目 → 详情面板展示输入/输出/耗时"
	@echo ""
	@echo "  [Debug Tab]"
	@echo "  □ Token 预算进度条随对话更新"
	@echo "  □ EventBus 事件流显示 (loop_start / tool_call 等)"
	@echo ""
	@echo "  [Memory Explorer]"
	@echo "  □ Working Memory 内容可见"
	@echo "  □ 对话后 Session Memory 有新增记录"
	@echo ""
	@echo "  [MCP Workbench]"
	@echo "  □ 可通过 UI 添加 Stdio MCP Server"
	@echo "  □ 可通过 UI 添加 SSE MCP Server (transport=sse, url 字段)"
	@echo "  □ Server 日志实时显示"
	@echo ""
	@echo "  [API 验证]"
	@echo "  □ make verify-api   (自动检查所有 REST 端点)"
	@echo ""
	@echo "  [Engine Hardening]"
	@echo "  □ 发送 10+ 轮重复消息 → Loop Guard 触发 (日志中可见 circuit_breaker)"
	@echo "  □ 上下文超 60% → 自动降级 (日志可见 context_pruner)"
	@echo ""
	@echo "完成后记录结果到 docs/main/WORK_LOG.md"

# API 端点可用性检查 (需服务器运行在 3001)
# 路由说明 (所有业务端点统一在 /api/v1/ 下):
#   /api/health                         — 健康检查 (readiness, 无版本前缀)
#   /api/health/live                    — 存活探针 (liveness, 无版本前缀)
#   /api/v1/config                      — 前端配置 (统一配置管理)
#   /api/v1/sessions/{id}/executions    — 按 session 查工具执行历史
#   /api/v1/executions/{id}             — 按 execution id 查单条记录
#   /api/v1/mcp/servers/{id}/tools      — 按 server id 查 MCP 工具列表
#   /api/v1/mcp/servers/{id}/logs       — 按 server id 查 MCP 日志
verify-api:
	@echo "=== REST API 端点验证 (需先 make server) ==="
	@echo ""
	@echo "[Health - readiness]"
	curl -sf http://localhost:3001/api/health && echo " ✅ GET /api/health" || echo " ❌ GET /api/health"
	@echo ""
	@echo "[Health - liveness]"
	curl -sf http://localhost:3001/api/health/live && echo " ✅ GET /api/health/live" || echo " ❌ GET /api/health/live"
	@echo ""
	@echo "[Frontend Config]"
	curl -sf http://localhost:3001/api/v1/config && echo " ✅ GET /api/v1/config" || echo " ❌ GET /api/v1/config"
	@echo ""
	@echo "[Sessions - list]"
	curl -sf http://localhost:3001/api/v1/sessions && echo " ✅ GET /api/v1/sessions" || echo " ❌ GET /api/v1/sessions"
	@echo ""
	@echo "[Memories - list all]"
	curl -sf http://localhost:3001/api/v1/memories && echo " ✅ GET /api/v1/memories" || echo " ❌ GET /api/v1/memories"
	@echo ""
	@echo "[Working Memory]"
	curl -sf http://localhost:3001/api/v1/memories/working && echo " ✅ GET /api/v1/memories/working" || echo " ❌ GET /api/v1/memories/working"
	@echo ""
	@echo "[Tool Executions - by session]"
	@FIRST_SID=$$(curl -sf http://localhost:3001/api/v1/sessions | python3 -c "import sys,json; d=json.load(sys.stdin); print(d[0]['session_id'] if d else '')" 2>/dev/null); \
	if [ -n "$$FIRST_SID" ]; then \
	  curl -sf "http://localhost:3001/api/v1/sessions/$$FIRST_SID/executions" && echo " ✅ GET /api/v1/sessions/{id}/executions (session=$$FIRST_SID)" || echo " ❌ GET /api/v1/sessions/{id}/executions"; \
	else \
	  echo " ⚠️  No sessions found — start a conversation first"; \
	fi
	@echo ""
	@echo "[MCP Servers - list]"
	curl -sf http://localhost:3001/api/v1/mcp/servers && echo " ✅ GET /api/v1/mcp/servers" || echo " ❌ GET /api/v1/mcp/servers"
	@echo ""
	@echo "[Built-in Tools - list]"
	curl -sf http://localhost:3001/api/v1/tools && echo " ✅ GET /api/v1/tools" || echo " ❌ GET /api/v1/tools"
	@echo ""
	@echo "[Budget]"
	curl -sf http://localhost:3001/api/v1/budget && echo " ✅ GET /api/v1/budget" || echo " ❌ GET /api/v1/budget"
	@echo ""
	@echo "Note: /api/v1/mcp/servers/{id}/tools and /api/v1/mcp/servers/{id}/logs"
	@echo "      require a server id — use 'make verify-api-mcp ID=<server_id>'"

# MCP server-specific endpoint check (requires server ID)
# Usage: make verify-api-mcp ID=<server_id>
verify-api-mcp:
	@if [ -z "$(ID)" ]; then echo "Usage: make verify-api-mcp ID=<server_id>"; exit 1; fi
	@echo "=== MCP Server $(ID) 端点验证 ==="
	curl -sf "http://localhost:3001/api/v1/mcp/servers/$(ID)" && echo " ✅ GET /api/v1/mcp/servers/$(ID)" || echo " ❌ GET /api/v1/mcp/servers/$(ID)"
	@echo ""
	curl -sf "http://localhost:3001/api/v1/mcp/servers/$(ID)/tools" && echo " ✅ GET /api/v1/mcp/servers/$(ID)/tools" || echo " ❌ GET /api/v1/mcp/servers/$(ID)/tools"
	@echo ""
	curl -sf "http://localhost:3001/api/v1/mcp/servers/$(ID)/logs" && echo " ✅ GET /api/v1/mcp/servers/$(ID)/logs" || echo " ❌ GET /api/v1/mcp/servers/$(ID)/logs"

# ============================================================
# 沙箱环境切换 (sandbox profile / run mode)
# 详细指南: docs/design/SANDBOX_ENVIRONMENT_GUIDE.md
# ============================================================

# 查看当前沙箱状态 (RunMode, Profile, Policy 等)
sandbox-status:
	cargo run -p grid-cli --bin grid -- --project $(TEST_PROJECT) sandbox status

# 预览所有工具类别的路由决策 (不实际执行)
sandbox-dry-run:
	cargo run -p grid-cli --bin grid -- --project $(TEST_PROJECT) sandbox dry-run

# 列出已注册的沙箱后端
sandbox-backends:
	cargo run -p grid-cli --bin grid -- --project $(TEST_PROJECT) sandbox list-backends

# Development 模式运行 CLI (默认, 所有工具本地执行)
sandbox-dev:
	GRID_SANDBOX_PROFILE=dev cargo run --quiet -p grid-cli --bin grid -- --project $(TEST_PROJECT) run $(CLI_ARGS)

# Staging 模式运行 CLI (强制容器, 无后端时报错)
sandbox-staging:
	GRID_SANDBOX_PROFILE=staging cargo run --quiet -p grid-cli --bin grid -- --project $(TEST_PROJECT) run $(CLI_ARGS)

# Production 模式运行 CLI (强制容器隔离)
sandbox-production:
	GRID_SANDBOX_PROFILE=production cargo run --quiet -p grid-cli --bin grid -- --project $(TEST_PROJECT) run $(CLI_ARGS)

# 进入容器内交互式 shell (自动检测为 Sandboxed 模式)
# API keys 从宿主机环境透传 (AD-D5)
sandbox-shell:
	@if ! docker image inspect grid-sandbox:dev >/dev/null 2>&1; then \
		echo "镜像 grid-sandbox:dev 不存在，先构建..."; \
		$(MAKE) container-build-dev; \
	fi
	docker run -it --rm \
		-v $(PWD):/workspace \
		-w /workspace \
		$(if $(ANTHROPIC_API_KEY),-e ANTHROPIC_API_KEY,) \
		$(if $(OPENAI_API_KEY),-e OPENAI_API_KEY,) \
		$(if $(OPENAI_BASE_URL),-e OPENAI_BASE_URL,) \
		grid-sandbox:dev bash

# ============================================================
# Container images (grid-sandbox base/dev)
# ============================================================

# Build base image (local, single platform)
container-build:
	docker build -t grid-sandbox:base container/

# Build dev image (local, single platform)
container-build-dev: container-build
	docker build -f container/Dockerfile.dev -t grid-sandbox:dev container/

# Build base image (multi-platform, push to GHCR)
container-build-multi:
	docker buildx build --platform linux/amd64,linux/arm64 \
	  -t ghcr.io/uukuguy/grid-sandbox:base \
	  --push container/

# Build dev image (multi-platform, push to GHCR)
container-build-multi-dev: container-build-multi
	docker buildx build --platform linux/amd64,linux/arm64 \
	  -f container/Dockerfile.dev \
	  -t ghcr.io/uukuguy/grid-sandbox:dev \
	  --push container/

# List grid-sandbox container images
container-list:
	@echo "=== Grid Sandbox Images ==="
	@docker images 'grid-sandbox' --format 'table {{.Repository}}:{{.Tag}}\t{{.Size}}\t{{.CreatedSince}}' 2>/dev/null || echo "  (none)"
	@echo ""
	@echo "=== Running Grid Sandbox Containers ==="
	@docker ps --filter 'label=grid.sandbox=true' --format 'table {{.ID}}\t{{.Names}}\t{{.Status}}\t{{.Image}}' 2>/dev/null || echo "  (none)"

# Remove grid-sandbox images and stopped containers
container-clean:
	@echo "Removing stopped Grid sandbox containers..."
	@docker ps -a --filter 'label=grid.sandbox=true' --filter 'status=exited' -q | xargs -r docker rm -f 2>/dev/null || true
	@echo "Removing Grid sandbox images..."
	@docker images 'grid-sandbox' -q | xargs -r docker rmi -f 2>/dev/null || true
	@echo "Done."

# Smoke-test: build base image and verify key tools
container-test: container-build
	@echo "=== Container Smoke Test ==="
	@docker run --rm --entrypoint sh grid-sandbox:base -c '\
	  echo "--- System tools ---" && \
	  pdftotext -v 2>&1 | head -1 && \
	  tesseract --version 2>&1 | head -1 && \
	  pandoc --version | head -1 && \
	  psql --version && \
	  sqlite3 --version && \
	  dig -v 2>&1 | head -1 && \
	  echo "--- Python packages ---" && \
	  python3 -c "import pymupdf, docx, openpyxl, pptx, chardet, tabulate; print(\"All document processing packages OK\")" && \
	  echo "--- CLI tools ---" && \
	  rg --version | head -1 && \
	  fd --version && \
	  bat --version | head -1 && \
	  echo "" && \
	  echo "All checks passed."'

# ============================================================
# grid-runtime container (EAASP L1 Tier 1 Harness)
# ============================================================

# Build grid-runtime release binary
runtime-build-binary:
	@echo "Building grid-runtime binary..."
	cargo build -p grid-runtime --release

# Build grid-runtime container image
runtime-build:
	@echo "Building grid-runtime container image..."
	docker build -f crates/grid-runtime/Dockerfile -t grid-runtime:latest .

# Start grid-runtime container
runtime-run:
	@echo "Starting grid-runtime container..."
	docker run --rm -p 50051:50051 \
		-e ANTHROPIC_API_KEY=$${ANTHROPIC_API_KEY} \
		grid-runtime:latest

# ============================================================
# Docker sandbox images (legacy per-language images from Phase J)
# NOTE: These use docker/sandbox-images/ — the older per-language approach.
#       Prefer container-* targets above for the unified base/dev images.
# ============================================================

docker-build:
	docker/sandbox-images/build.sh all

docker-build-python:
	docker/sandbox-images/build.sh python

docker-build-rust:
	docker/sandbox-images/build.sh rust

docker-build-nodejs:
	docker/sandbox-images/build.sh nodejs

docker-build-bash:
	docker/sandbox-images/build.sh bash

docker-build-general:
	docker/sandbox-images/build.sh general

docker-build-swebench:
	docker/sandbox-images/build.sh swebench

docker-list:
	@docker images 'grid-sandbox/*' --format 'table {{.Repository}}:{{.Tag}}\t{{.Size}}\t{{.CreatedSince}}'

docker-clean:
	@docker images 'grid-sandbox/*' -q | xargs -r docker rmi -f

# ============================================================
# 帮助
# ============================================================

help:
	@echo "Grid Platform Makefile"
	@echo ""
	@echo "常用命令:"
	@echo "  make dev              同时启动后端 + 前端开发服务器"
	@echo "  make check            Rust 编译检查 (最快)"
	@echo "  make build            Rust 编译构建"
	@echo "  make test             运行全部测试"
	@echo "  make server           启动后端服务器 (端口 3001)"
	@echo "  make web              启动前端开发服务器 (端口 5173)"
	@echo ""
	@echo "手工验证 (grid-workbench):"
	@echo "  make verify           静态验证: cargo check + tsc + vite build"
	@echo "  make verify-runtime   打印运行时验证步骤清单 (需先启动服务)"
	@echo "  make verify-api       REST API 端点可用性检查 (需先 make server)"
	@echo "  make verify-api-mcp ID=<id>  MCP server 专属端点检查"
	@echo ""
	@echo "CLI (grid — 轻量命令行):"
	@echo "  make cli                             显示 CLI 帮助"
	@echo "  make cli-run                         交互式 REPL 会话"
	@echo "  make cli-ask QUERY=\"问题\"             单次提问 (headless)"
	@echo "  make cli-agent                       列出 agents"
	@echo "  make cli-session                     列出 sessions"
	@echo "  make cli-config                      显示配置"
	@echo "  make cli-doctor                      健康诊断"
	@echo ""
	@echo "Studio (grid-studio — TUI + Dashboard 工作台):"
	@echo "  make studio                          TUI 全屏模式 (默认)"
	@echo "  make studio-tui                      TUI 全屏模式"
	@echo "  make studio-dashboard                Web Dashboard"
	@echo ""
	@echo "沙箱环境切换 (详细指南: docs/design/SANDBOX_ENVIRONMENT_GUIDE.md):"
	@echo "  make sandbox-status                  查看当前沙箱状态"
	@echo "  make sandbox-dry-run                 预览工具路由决策"
	@echo "  make sandbox-backends                列出已注册后端"
	@echo "  make sandbox-dev                     Development 模式 (默认, 本地执行)"
	@echo "  make sandbox-staging                 Staging 模式 (优先容器)"
	@echo "  make sandbox-production              Production 模式 (强制容器)"
	@echo "  make sandbox-shell                   进入容器内交互式 shell"
	@echo ""
	@echo "构建:"
	@echo "  make build            Rust 编译构建 (日常开发)"
	@echo "  make build-cli        仅构建 grid CLI (轻量, 无 TUI)"
	@echo "  make build-studio     构建 grid-studio (含 TUI + Dashboard)"
	@echo "  make build-cli-full   完整构建 (含 WASM/Docker/PDF + Studio)"
	@echo "  make all              完整构建 (后端 + 前端)"
	@echo "  make release          Release 构建"
	@echo "  make web-build        前端生产构建"
	@echo ""
	@echo "代码质量:"
	@echo "  make fmt              代码格式化"
	@echo "  make lint             Clippy lint"
	@echo "  make web-check        TypeScript 类型检查"
	@echo ""
	@echo "清理:"
	@echo "  make clean            清理 Rust 构建产物"
	@echo "  make clean-all        清理全部构建产物"
	@echo ""
	@echo "评估 (grid-eval):"
	@echo "  make eval-list                       列出可用 suite"
	@echo "  make eval-benchmark-mini             快速冒烟: 3 tasks/suite × 3 模型"
	@echo "  make eval-benchmark                  完整 benchmark (全 suite × 全模型)"
	@echo "  make eval-run EVAL_SUITE=resilience  单 suite 单模型运行"
	@echo "  make eval-compare EVAL_SUITE=bfcl    单 suite 多模型对比"
	@echo "  make eval-history                    列出历史运行记录"
	@echo "  make eval-report EVAL_RUN_ID=<id>    查看运行报告"
	@echo "  make eval-trace EVAL_RUN_ID=<id> EVAL_TASK_ID=<tid>  查看 task trace"
	@echo "  make eval-diagnose EVAL_RUN_ID=<id>  失败原因分类"
	@echo "  make eval-diff EVAL_RUN_A=<a> EVAL_RUN_B=<b>  两次运行对比"
	@echo "  make eval-progress                   即时查看正在运行的 benchmark 进度"
	@echo "  make eval-progress EVAL_RUN_ID=<id>  查看指定运行的进度"
	@echo ""
	@echo "Container images (grid-sandbox base/dev):"
	@echo "  make container-build           构建 base 镜像 (本地单平台)"
	@echo "  make container-build-dev       构建 dev 镜像 (本地单平台)"
	@echo "  make container-build-multi     构建 base 镜像 (多平台, 推送 GHCR)"
	@echo "  make container-build-multi-dev 构建 dev 镜像 (多平台, 推送 GHCR)"
	@echo "  make container-list            列出镜像和运行中容器"
	@echo "  make container-clean           清理停止的容器和镜像"
	@echo "  make container-test            构建并验证 base 镜像工具可用"
	@echo ""
	@echo "Docker sandbox images (legacy, per-language):"
	@echo "  make docker-build              构建全部 sandbox Docker 镜像"
	@echo "  make docker-build-python       构建 Python sandbox 镜像"
	@echo "  make docker-build-rust         构建 Rust sandbox 镜像"
	@echo "  make docker-build-nodejs       构建 Node.js sandbox 镜像"
	@echo "  make docker-build-bash         构建 Bash sandbox 镜像"
	@echo "  make docker-build-general      构建 General sandbox 镜像"
	@echo "  make docker-build-swebench     构建 SWE-bench sandbox 镜像"
	@echo "  make docker-list               列出已构建的 sandbox 镜像"
	@echo "  make docker-clean              删除全部 sandbox 镜像"
	@echo ""
	@echo "首次使用:"
	@echo "  make setup            安装前端依赖"
	@echo "  cp .env.example .env  然后填入 ANTHROPIC_API_KEY"

# ============================================================
# claude-code-runtime (Python EAASP L1 Runtime)
# ============================================================

claude-runtime-setup:
	cd lang/claude-code-runtime-python && uv sync --extra dev

claude-runtime-proto:
	cd lang/claude-code-runtime-python && uv run python build_proto.py

claude-runtime-test:
	cd lang/claude-code-runtime-python && uv run pytest tests/ -xvs

claude-runtime-start:
	cd lang/claude-code-runtime-python && uv run python -m claude_code_runtime --port 50052 --env-file ../../.env

# 容器构建 (从项目根目录构建，包含 proto/)
claude-runtime-build:
	docker build -f lang/claude-code-runtime-python/Dockerfile -t claude-code-runtime:latest .

# 容器运行 (需要 ANTHROPIC_API_KEY 环境变量)
claude-runtime-run:
	docker run --rm -p 50052:50052 \
		-e ANTHROPIC_API_KEY \
		-e ANTHROPIC_BASE_URL \
		-e ANTHROPIC_MODEL_NAME \
		claude-code-runtime:latest

# 双 runtime 集成验证 (先 build 再启动，需要 ANTHROPIC_API_KEY)
verify-dual-runtime:
	./scripts/verify-dual-runtime.sh
