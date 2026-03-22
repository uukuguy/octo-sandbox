# octo-sandbox 下一会话指南

**最后更新**: 2026-03-23 05:45 GMT+8
**当前分支**: `main`
**当前状态**: Phase AA COMPLETE + AA-D2 补齐

---

## 项目状态

```
Phase AA: Octo 部署配置架构 (6/6+D2)  → COMPLETE @ 4fbc30d
Phase Z:  Landmine Scan & Fix (2/2)   → COMPLETE @ 81fa923
Phase Y:  Playbook Skill SubAgent (1/1)→ COMPLETE @ c0f92b4
Phase X:  TUI 运行状态增强 (4/4)       → COMPLETE
Phase W:  OctoRoot 统一目录管理 (10/10) → COMPLETE
Phase V:  Agent Skills 完整实现 (11/12) → COMPLETE @ 19d3f30
Phase U:  TUI Production Hardening     → COMPLETE @ 77c2297
Phase T:  TUI OpenDev 整合 (24/24)     → COMPLETE @ 74464b9
Phase S:  Agent Capability Boost       → COMPLETE @ 68ad13e
Phase R:  GAIA Filtered Eval (8/8)     → COMPLETE @ 50df5e6
Phase Q-A: 评估框架+基准               → ALL COMPLETE
Wave 1-10: Core Engine + CLI          → COMPLETE @ 675155d
```

### 基线数据

- **Tests**: 2394 passing (workspace)
- **评估任务**: ~297 个 (内部 167 + 外部 130)
- **GAIA 结果**: MiniMax-M2.1 41.6%, Qwen3.5-27B 39.2%
- **测试命令**: `cargo test --workspace -- --test-threads=1`

---

## Deferred 未清项（下次 session 启动时必查）

| 来源 | ID | 内容 | 前置条件 | 状态 |
|------|----|----|---------|------|
| Phase AA | AA-D1 | `octo auth login/status/logout` CLI 命令 | UX 设计（交互式凭据设置） | ⏳ |
| Phase AA | AA-D3 | XDG Base Directory 支持 | 低优先级，OCTO_GLOBAL_ROOT 已覆盖 | ⏳ |
| Phase AA | AA-D4 | Config 热重载 | 未来增强 | ⏳ |
| Phase Z | Z-D1 | CredentialResolver → provider chain 对接 | Config 加载稳定后 | 🟡 部分满足 |
| Phase U | U-D1 | Agent Debug Panel 重设计 | Phase U G3 完成 | 前置已满足 |
| Phase S | S-D1 | Agent Skills 规范研究 | Phase S 完成 | 前置已满足 |

---

## 下一步建议

### 方向 1: AA-D1 octo auth 交互式凭据管理

实现 `octo auth login/status/logout` 命令，让用户通过交互式 CLI 设置 API keys（写入 ~/.octo/credentials.yaml）。

### 方向 2: U-D1 Agent Debug Panel 重设计

StatusBar 已整合 brand/tokens/elapsed/context%/git 信息，原有 Debug Panel 需重新设计。

### 方向 3: 更强模型评估

使用 Claude/GPT-4o 级模型跑 GAIA 对比，突破 Qwen3.5 瓶颈。

### 方向 4: Agent 工具链增强

更多内置工具、更好的搜索策略、文件解析能力。

---

## Phase AA 新增 CLI 命令

| 命令 | 功能 |
|------|------|
| `octo init` | 初始化项目 .octo/ 目录结构 |
| `octo config show` | 显示分层配置源链 + 生效配置 |
| `octo config paths` | 列出所有配置文件位置 |

## 配置文件优先级（Phase AA 实现）

```
1. 代码默认值
2. ~/.octo/config.yaml          (全局配置)
3. $PWD/.octo/config.yaml       (项目配置)
4. $PWD/.octo/config.local.yaml (本地覆盖，git-ignored)
5. ~/.octo/credentials.yaml     (凭据，mode 600)
6. CLI 参数 (--port, --host)
7. 环境变量 (OCTO_*, ANTHROPIC_*, OPENAI_*)
```

---

## 快速启动

```bash
# 初始化项目
cargo run -p octo-cli -- init

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
```
