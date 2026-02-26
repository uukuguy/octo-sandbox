.PHONY: dev build check test clean fmt lint server web all install setup

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
# 帮助
# ============================================================

help:
	@echo "octo-sandbox Makefile"
	@echo ""
	@echo "常用命令:"
	@echo "  make dev          同时启动后端 + 前端开发服务器"
	@echo "  make check        Rust 编译检查 (最快)"
	@echo "  make build        Rust 编译构建"
	@echo "  make test         运行全部测试"
	@echo "  make server       启动后端服务器"
	@echo "  make web          启动前端开发服务器"
	@echo ""
	@echo "构建:"
	@echo "  make all          完整构建 (后端 + 前端)"
	@echo "  make release      Release 构建"
	@echo "  make web-build    前端生产构建"
	@echo ""
	@echo "代码质量:"
	@echo "  make fmt          代码格式化"
	@echo "  make lint         Clippy lint"
	@echo "  make web-check    TypeScript 类型检查"
	@echo ""
	@echo "清理:"
	@echo "  make clean        清理 Rust 构建产物"
	@echo "  make clean-all    清理全部构建产物"
	@echo ""
	@echo "首次使用:"
	@echo "  make setup        安装前端依赖"
	@echo "  cp .env.example .env  然后填入 ANTHROPIC_API_KEY"
