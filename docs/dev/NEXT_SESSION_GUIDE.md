# octo-sandbox 下一会话指南

**最后更新**: 2026-03-28 11:30 GMT+8
**当前分支**: `main` (ahead of origin by 10 commits)
**当前状态**: Tier1+Tier2 暂缓项批量完成

---

## 项目状态

```
CI Fix + Z-D1 + AA-D1 + AB-D1(partial)  -> COMPLETE @ 9f7c163
Scheduler Tool (schedule_task)           -> COMPLETE @ a922159
SubAgent Streaming Events                -> COMPLETE @ cc05eeb
Builtin Commands Redesign                -> COMPLETE @ 1916320
Custom Commands + TUI Fixes              -> COMPLETE @ 263eeb2
Post-AF: Builtin Skills + Config + TUI Fix -> COMPLETE @ 072c15b
Phase AF-AE-AD-AC-AB-AA-Z-Y-X-W-V-U-T  -> ALL COMPLETE
```

### 基线数据

- **Tests**: 2476 passing (+ auth tests)
- **测试命令**: `cargo test --workspace -- --test-threads=1`
- **DB migrations**: CURRENT_VERSION=11

---

## 本次完成的暂缓项

### Z-D1: CredentialResolver → Provider chain ✅
- `AgentRuntime` 不再 hardcode `std::env::var("ANTHROPIC_API_KEY")`
- 使用 `resolve_api_key_env()` 自动匹配 20+ provider 的 env var
- 通过 `CredentialResolver` 优先级链解析: Vault > env > credentials.yaml > .env

### AA-D1: octo auth login/status/logout ✅
- `octo auth login --provider anthropic --key sk-...` 存储到 `~/.octo/credentials.yaml`
- `octo auth status` 显示已存储凭证（脱敏）+ 环境变量覆盖
- `octo auth logout --provider openai` 移除凭证
- `CredentialResolver` 新增 `with_credentials()` 支持 YAML 文件
- 文件权限自动设为 0600

### AB-D1: CI/CD 修复 ✅
- 移除 `.cargo/config.toml`（macOS sccache 路径破坏 Linux CI）
- 修复 `install-cli-tools.sh` ripgrep 15.1.0 URL（amd64 改用 musl）
- 容器构建触发条件: push main + `container/**` 路径变更（不是每次推送）

---

## Deferred 未清项

| 来源 | ID | 内容 | 状态 |
|------|----|----|------|
| Phase AB | AB-D2 | E2B provider 完整实现 | 可实施 |
| Phase AB | AB-D3 | WASM plugin loading | 待定 |
| Phase AB | AB-D4 | Session Sandbox 持久化 | 可实施 |
| Phase AB | AB-D5 | CredentialResolver -> sandbox env 注入 | 可实施（Z-D1 已完成） |
| Phase AB | AB-D6 | gVisor / Firecracker provider | 可实施 |
| Phase AC | AC-D4~D6 | Multi-image, snapshots, compose | 低优先级 |
| Phase AD | AD-D1~D6 | LibreOffice, cloud, cosign, CLI, docling | 低优先级 |
| Phase AA | AA-D3 | XDG Base Directory | 低优先级 |
| Phase AA | AA-D4 | Config 热重载 | 未来增强 |

---

## 关键代码路径

| 文件 | 作用 |
|------|------|
| `crates/octo-cli/src/commands/auth.rs` | octo auth 凭证管理 (NEW) |
| `crates/octo-engine/src/secret/resolver.rs` | CredentialResolver 优先级链 |
| `crates/octo-engine/src/tools/scheduler.rs` | scheduler tool |
| `crates/octo-engine/src/agent/runtime.rs` | AgentRuntime 初始化 |
| `crates/octo-engine/src/skills/execute_tool.rs` | sub-agent 事件转发 |
| `crates/octo-engine/src/sandbox/` | SandboxProfile, SSM, Docker, 路由 |
| `crates/octo-cli/src/tui/` | TUI 核心 |
| `config.default.yaml` | 全量配置参考 |

---

## 快速启动

```bash
# 编译检查
cargo check --workspace

# 全量测试
cargo test --workspace -- --test-threads=1

# TUI 模式
make cli-tui

# CLI 交互模式
make cli-run

# 启动 server + web
make dev

# 凭证管理
octo auth login --provider anthropic --key sk-ant-xxx
octo auth status
```
