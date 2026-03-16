.PHONY: dev build check test clean fmt lint server web all install setup \
        verify verify-runtime verify-api verify-api-mcp \
        eval-list eval-run eval-compare eval-benchmark eval-benchmark-mini \
        eval-history eval-report eval-trace eval-diagnose eval-diff

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
	cargo run -p octo-server -- config-gen > config.yaml

# 编译检查 (最快, 不生成二进制)
check:
	cargo check --workspace

# 编译构建
build:
	cargo build

# 编译构建 (release)
release:
	cargo build --release

# 运行后端服务器
server:
	cargo run -p octo-server

# 运行测试
test:
	cargo test --workspace

# 单个 crate 测试
test-types:
	cargo test -p octo-types

test-engine:
	cargo test -p octo-engine

test-sandbox:
	cargo test -p octo-sandbox

test-server:
	cargo test -p octo-server

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
# 评估命令 (octo-eval)
# 注意: 所有命令从 workspace 根目录运行，输出写入 eval_output/
# ============================================================

EVAL_CONFIG     ?= crates/octo-eval/eval.benchmark.toml
EVAL_MINI_CONFIG ?= crates/octo-eval/eval.benchmark.mini.toml
EVAL_SUITE      ?= tool_call
EVAL_MAX_TASKS  ?= 0
EVAL_FORMAT     ?= both
EVAL_RUN_ID     ?=
EVAL_TASK_ID    ?=

# 列出可用 suite
eval-list:
	cargo run -p octo-eval -- list-suites

# 运行单个 suite（单模型）
# 用法: make eval-run EVAL_SUITE=resilience
eval-run:
	cargo run -p octo-eval -- run --suite $(EVAL_SUITE) \
	  $(if $(filter-out 0,$(EVAL_MAX_TASKS)),--max-tasks $(EVAL_MAX_TASKS),) \
	  --format $(EVAL_FORMAT)

# 多模型对比单个 suite
# 用法: make eval-compare EVAL_SUITE=security EVAL_CONFIG=crates/octo-eval/eval.benchmark.3model.toml
eval-compare:
	cargo run -p octo-eval -- compare --suite $(EVAL_SUITE) \
	  --config $(EVAL_CONFIG) \
	  $(if $(filter-out 0,$(EVAL_MAX_TASKS)),--max-tasks $(EVAL_MAX_TASKS),) \
	  --format $(EVAL_FORMAT)

# 完整 benchmark（全部 suite × 全部模型，并发）
# 用法: make eval-benchmark EVAL_CONFIG=crates/octo-eval/eval.benchmark.toml
eval-benchmark:
	cargo run -p octo-eval -- benchmark \
	  --config $(EVAL_CONFIG) \
	  $(if $(filter-out 0,$(EVAL_MAX_TASKS)),--max-tasks $(EVAL_MAX_TASKS),) \
	  --format $(EVAL_FORMAT)

# Mini benchmark：每 suite 3 个任务，快速冒烟测试
# 用法: make eval-benchmark-mini
eval-benchmark-mini:
	cargo run -p octo-eval -- benchmark \
	  --config $(EVAL_MINI_CONFIG) \
	  --max-tasks 3 \
	  --format $(EVAL_FORMAT)

# 列出历史运行记录
eval-history:
	cargo run -p octo-eval -- history

# 查看运行报告
# 用法: make eval-report EVAL_RUN_ID=2026-03-16-001
eval-report:
	@if [ -z "$(EVAL_RUN_ID)" ]; then echo "Usage: make eval-report EVAL_RUN_ID=<run_id>"; exit 1; fi
	cargo run -p octo-eval -- report $(EVAL_RUN_ID) --format $(EVAL_FORMAT)

# 查看任务 trace 时间线
# 用法: make eval-trace EVAL_RUN_ID=2026-03-16-001 EVAL_TASK_ID=tc-01
eval-trace:
	@if [ -z "$(EVAL_RUN_ID)" ]; then echo "Usage: make eval-trace EVAL_RUN_ID=<run_id> EVAL_TASK_ID=<task_id>"; exit 1; fi
	@if [ -z "$(EVAL_TASK_ID)" ]; then echo "Usage: make eval-trace EVAL_RUN_ID=<run_id> EVAL_TASK_ID=<task_id>"; exit 1; fi
	cargo run -p octo-eval -- trace $(EVAL_RUN_ID) $(EVAL_TASK_ID)

# 失败原因分类分析
# 用法: make eval-diagnose EVAL_RUN_ID=2026-03-16-001
eval-diagnose:
	@if [ -z "$(EVAL_RUN_ID)" ]; then echo "Usage: make eval-diagnose EVAL_RUN_ID=<run_id>"; exit 1; fi
	cargo run -p octo-eval -- diagnose $(EVAL_RUN_ID)

# 两次运行回归对比
# 用法: make eval-diff EVAL_RUN_A=2026-03-15-001 EVAL_RUN_B=2026-03-16-001
eval-diff:
	@if [ -z "$(EVAL_RUN_A)" ] || [ -z "$(EVAL_RUN_B)" ]; then \
	  echo "Usage: make eval-diff EVAL_RUN_A=<run_a> EVAL_RUN_B=<run_b>"; exit 1; fi
	cargo run -p octo-eval -- diff $(EVAL_RUN_A) $(EVAL_RUN_B)

# ============================================================
# 手工验证命令 (octo-workbench)
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
	@echo "=== octo-workbench 运行时验证步骤 ==="
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
# 路由说明:
#   /api/config                    — 前端配置 (统一配置管理)
#   /api/sessions/{id}/executions  — 按 session 查工具执行历史
#   /api/executions/{id}           — 按 execution id 查单条记录
#   /api/mcp/servers/{id}/tools    — 按 server id 查 MCP 工具列表
#   /api/mcp/servers/{id}/logs     — 按 server id 查 MCP 日志
verify-api:
	@echo "=== REST API 端点验证 (需先 make server) ==="
	@echo ""
	@echo "[Health]"
	curl -sf http://localhost:3001/api/health && echo " ✅ GET /api/health" || echo " ❌ GET /api/health"
	@echo ""
	@echo "[Frontend Config]"
	curl -sf http://localhost:3001/api/config && echo " ✅ GET /api/config" || echo " ❌ GET /api/config"
	@echo ""
	@echo "[Sessions - list]"
	curl -sf http://localhost:3001/api/sessions && echo " ✅ GET /api/sessions" || echo " ❌ GET /api/sessions"
	@echo ""
	@echo "[Memories - list all]"
	curl -sf http://localhost:3001/api/memories && echo " ✅ GET /api/memories" || echo " ❌ GET /api/memories"
	@echo ""
	@echo "[Working Memory]"
	curl -sf http://localhost:3001/api/memories/working && echo " ✅ GET /api/memories/working" || echo " ❌ GET /api/memories/working"
	@echo ""
	@echo "[Tool Executions - by session]"
	@FIRST_SID=$$(curl -sf http://localhost:3001/api/sessions | python3 -c "import sys,json; d=json.load(sys.stdin); print(d[0]['session_id'] if d else '')" 2>/dev/null); \
	if [ -n "$$FIRST_SID" ]; then \
	  curl -sf "http://localhost:3001/api/sessions/$$FIRST_SID/executions" && echo " ✅ GET /api/sessions/{id}/executions (session=$$FIRST_SID)" || echo " ❌ GET /api/sessions/{id}/executions"; \
	else \
	  echo " ⚠️  No sessions found — start a conversation first"; \
	fi
	@echo ""
	@echo "[MCP Servers - list]"
	curl -sf http://localhost:3001/api/mcp/servers && echo " ✅ GET /api/mcp/servers" || echo " ❌ GET /api/mcp/servers"
	@echo ""
	@echo "[Built-in Tools - list]"
	curl -sf http://localhost:3001/api/tools && echo " ✅ GET /api/tools" || echo " ❌ GET /api/tools"
	@echo ""
	@echo "[Budget]"
	curl -sf http://localhost:3001/api/budget && echo " ✅ GET /api/budget" || echo " ❌ GET /api/budget"
	@echo ""
	@echo "Note: /api/mcp/servers/{id}/tools and /api/mcp/servers/{id}/logs"
	@echo "      require a server id — use 'make verify-api-mcp ID=<server_id>'"

# MCP server-specific endpoint check (requires server ID)
# Usage: make verify-api-mcp ID=<server_id>
verify-api-mcp:
	@if [ -z "$(ID)" ]; then echo "Usage: make verify-api-mcp ID=<server_id>"; exit 1; fi
	@echo "=== MCP Server $(ID) 端点验证 ==="
	curl -sf "http://localhost:3001/api/mcp/servers/$(ID)" && echo " ✅ GET /api/mcp/servers/$(ID)" || echo " ❌ GET /api/mcp/servers/$(ID)"
	@echo ""
	curl -sf "http://localhost:3001/api/mcp/servers/$(ID)/tools" && echo " ✅ GET /api/mcp/servers/$(ID)/tools" || echo " ❌ GET /api/mcp/servers/$(ID)/tools"
	@echo ""
	curl -sf "http://localhost:3001/api/mcp/servers/$(ID)/logs" && echo " ✅ GET /api/mcp/servers/$(ID)/logs" || echo " ❌ GET /api/mcp/servers/$(ID)/logs"

# ============================================================
# 帮助
# ============================================================

help:
	@echo "octo-sandbox Makefile"
	@echo ""
	@echo "常用命令:"
	@echo "  make dev              同时启动后端 + 前端开发服务器"
	@echo "  make check            Rust 编译检查 (最快)"
	@echo "  make build            Rust 编译构建"
	@echo "  make test             运行全部测试"
	@echo "  make server           启动后端服务器 (端口 3001)"
	@echo "  make web              启动前端开发服务器 (端口 5173)"
	@echo ""
	@echo "手工验证 (octo-workbench):"
	@echo "  make verify           静态验证: cargo check + tsc + vite build"
	@echo "  make verify-runtime   打印运行时验证步骤清单 (需先启动服务)"
	@echo "  make verify-api       REST API 端点可用性检查 (需先 make server)"
	@echo "  make verify-api-mcp ID=<id>  MCP server 专属端点检查"
	@echo ""
	@echo "构建:"
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
	@echo "评估 (octo-eval):"
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
	@echo ""
	@echo "首次使用:"
	@echo "  make setup            安装前端依赖"
	@echo "  cp .env.example .env  然后填入 ANTHROPIC_API_KEY"
