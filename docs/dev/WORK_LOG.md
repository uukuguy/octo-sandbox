# octo-sandbox 工作日志

## Phase H — Eval Capstone (2026-03-14)

### 完成内容

**H1: Resilience Suite + 新行为类型**
- 在 BehaviorScorer 中新增 4 种行为模式: retry_success, emergency_stopped, canary_detected, text_tool_recovered
- 同步更新 loader.rs 中的 score_behavior() 函数
- 创建 ResilienceSuite 模块 (resilience.rs) 和 20 条 JSONL 评估任务
- 注册到 mod.rs / main.rs / CLI help

**H2: Context 扩充**
- octo_context.jsonl 从 14 扩充到 50 条任务
- 新增 8 个评估维度: CX5 (degradation), CX6 (token budget), CX7 (long prompt), CX8 (multi-turn), CX9 (prioritization), CX10 (recovery), CX11 (format consistency), CX12 (information density)

**H3: AstMatch Scorer**
- 实现 AstMatchScorer，支持深层 JSON 结构比较
- 功能: 嵌套对象递归比较、数组顺序无关匹配、类型强转 (strict_types=false)、null=缺失语义、额外字段容忍
- 新增 AstMatch variant 到 ScoreDetails enum
- 在 auto_scorer() 中集成 "ast_match" scorer 覆盖
- 10 条 AST 匹配测试用例添加到 octo_tool_call.jsonl

**H4: 验证与 CI**
- eval-ci.yml 新增 resilience suite 运行步骤
- CLI list-suites 帮助文本更新
- 全量测试通过: 1979 tests (+17)

### 技术变更

| 文件 | 变更 |
|------|------|
| `crates/octo-eval/src/scorer.rs` | +4 behavior branches, +AstMatchScorer (~130 LOC), +16 tests |
| `crates/octo-eval/src/score.rs` | +AstMatch ScoreDetails variant |
| `crates/octo-eval/src/datasets/loader.rs` | +score_ast_match(), +strict_types field, +4 behaviors |
| `crates/octo-eval/src/suites/resilience.rs` | 新文件, ResilienceSuite 实现 |
| `crates/octo-eval/src/suites/mod.rs` | +resilience 导出 |
| `crates/octo-eval/src/main.rs` | +resilience import/load/help |
| `crates/octo-eval/datasets/octo_resilience.jsonl` | 新文件, 20 tasks |
| `crates/octo-eval/datasets/octo_context.jsonl` | 14→50 tasks |
| `crates/octo-eval/datasets/octo_tool_call.jsonl` | +10 AST tasks |
| `.github/workflows/eval-ci.yml` | +resilience suite step |

### 测试结果

- 全量: 1979 tests passing (was 1962)
- Docker tests: 5 excluded (Docker daemon not running)
- 编译无 warning

### 遗留问题

- 无

### 下一步

- Phase I: SWE-bench 适配 (12 tasks)
- Phase J: Docker 测试修复 (8 tasks)
- Phase K: 完整模型对比报告 (10 tasks)
