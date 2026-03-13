# Octo CLI 端评估手册

> 本手册定义 Octo CLI (`octo`) 的 6 项功能评估任务，覆盖文件读写、命令执行、多步操作链、代码生成、记忆一致性和 MCP 工具集成。
> 每项任务均包含精确的执行步骤、预期输出和评判标准，可由评估人员在真实环境中独立执行。

---

## 目录

- [环境准备](#环境准备)
- [评估概览](#评估概览)
- [C1: 文件读写往返 (Easy)](#c1-文件读写往返)
- [C2: Bash 命令执行与输出解析 (Easy)](#c2-bash-命令执行与输出解析)
- [C3: 多步文件操作链 (Medium)](#c3-多步文件操作链)
- [C4: 代码生成与验证 (Medium)](#c4-代码生成与验证)
- [C5: 记忆存取一致性 (Medium)](#c5-记忆存取一致性)
- [C6: MCP 工具发现与调用 (Hard)](#c6-mcp-工具发现与调用)
- [评估结果汇总模板](#评估结果汇总模板)

---

## 环境准备

### 系统要求

| 项目 | 要求 |
|------|------|
| Rust 版本 | 1.75+ |
| 操作系统 | macOS / Linux |
| Node.js | 18+（仅 C6 MCP 任务需要） |
| 磁盘空间 | 500MB（编译产物） |

### 构建与配置

```bash
# 1. 编译 CLI
cargo build -p octo-cli

# 2. 验证二进制可用
./target/debug/octo --version
# 预期输出: octo <version>

# 3. 配置环境变量（复制模板并填写 API Key）
cp .env.example .env
# 编辑 .env，填入 ANTHROPIC_API_KEY=sk-ant-xxxxx

# 4. 运行健康检查
./target/debug/octo doctor
# 预期: 所有检查项显示 [PASS] 或 [WARN]，LLM Provider 为 [PASS]
```

### 评估用临时目录

所有评估任务的文件操作均在临时目录中进行，避免污染项目文件：

```bash
export EVAL_DIR="/tmp/octo-eval-$(date +%s)"
mkdir -p "$EVAL_DIR"
echo "评估目录: $EVAL_DIR"
```

### 便捷别名

```bash
alias octo="./target/debug/octo"
```

> **注意**: 以下所有命令中的 `octo` 均指向 `./target/debug/octo` 二进制。若未设置别名，请替换为完整路径。

---

## 评估概览

| 任务 ID | 名称 | 难度 | 评估维度 | 预计时间 |
|---------|------|------|---------|---------|
| C1 | 文件读写往返 | Easy | 工具调用能力、file_write / file_read 协同 | 3 分钟 |
| C2 | Bash 命令执行与输出解析 | Easy | bash 工具调用、输出理解与总结 | 3 分钟 |
| C3 | 多步文件操作链 | Medium | 多工具协调、任务规划、file_write + file_edit + grep | 5 分钟 |
| C4 | 代码生成与验证 | Medium | 代码生成质量、bash 编译/运行验证 | 5 分钟 |
| C5 | 记忆存取一致性 | Medium | memory add / search 跨会话持久化 | 5 分钟 |
| C6 | MCP 工具发现与调用 | Hard | MCP 服务器注册、工具发现、端到端调用链 | 10 分钟 |

---

## C1: 文件读写往返

### 基本信息

| 属性 | 值 |
|------|---|
| 任务 ID | C1 |
| 名称 | 文件读写往返 |
| 难度 | Easy |
| 评估维度 | Agent 正确调用 `file_write` 和 `file_read` 内置工具的能力 |

### 前置条件

- CLI 已编译并通过 `octo doctor` 健康检查
- `ANTHROPIC_API_KEY` 已配置
- `$EVAL_DIR` 临时目录已创建

### 执行步骤

**步骤 1 — 让 Agent 创建文件**

```bash
octo ask "请在 $EVAL_DIR/hello.txt 中写入以下内容（精确三行）：
第一行：Hello from Octo CLI
第二行：当前时间戳占位符 TIMESTAMP
第三行：评估任务 C1 完成
写完后请读取该文件并确认内容。"
```

**步骤 2 — 独立验证文件**

```bash
# 验证文件存在
test -f "$EVAL_DIR/hello.txt" && echo "FILE_EXISTS=true" || echo "FILE_EXISTS=false"

# 验证内容
cat "$EVAL_DIR/hello.txt"

# 验证行数
wc -l < "$EVAL_DIR/hello.txt"
```

### 预期输出

1. Agent 响应中包含调用了 `file_write` 工具的证据（如提到"已创建文件"或"已写入"）
2. Agent 响应中包含调用了 `file_read` 工具的证据（如显示了文件内容）
3. `$EVAL_DIR/hello.txt` 文件存在
4. 文件包含 3 行，第一行为 `Hello from Octo CLI`，第三行为 `评估任务 C1 完成`

### 评判标准

| 条件 | Pass/Fail |
|------|-----------|
| 文件 `$EVAL_DIR/hello.txt` 存在 | **必须 Pass** |
| 文件内容包含 `Hello from Octo CLI` | **必须 Pass** |
| 文件内容包含 `评估任务 C1 完成` | **必须 Pass** |
| Agent 响应中体现了读取确认（读回内容） | **必须 Pass** |
| 命令退出码为 0 | **必须 Pass** |

**总评**: 5 项全部 Pass 则任务通过。

### 清理

```bash
rm -f "$EVAL_DIR/hello.txt"
```

### 故障排除

| 症状 | 原因 | 解决方案 |
|------|------|---------|
| Agent 拒绝写文件 | 安全策略阻止路径 | 检查 SecurityPolicy 中 `allowed_write_dirs` 是否包含 `/tmp` |
| Agent 返回错误但未创建文件 | API Key 无效或额度用尽 | 检查 `.env` 中 `ANTHROPIC_API_KEY` |
| 文件存在但内容不对 | Agent 理解偏差 | 非阻塞问题，检查内容是否语义等价 |
| `octo ask` 超时 | 网络问题 | 检查网络连接，重试 |

---

## C2: Bash 命令执行与输出解析

### 基本信息

| 属性 | 值 |
|------|---|
| 任务 ID | C2 |
| 名称 | Bash 命令执行与输出解析 |
| 难度 | Easy |
| 评估维度 | Agent 正确调用 `bash` 工具、解析输出并生成结构化总结 |

### 前置条件

- CLI 已编译并通过健康检查
- `ANTHROPIC_API_KEY` 已配置

### 执行步骤

**步骤 1 — 让 Agent 执行系统信息命令**

```bash
octo ask "请执行以下 bash 命令并报告结果：
1. uname -a （显示系统信息）
2. whoami （当前用户）
3. ls -la /tmp | head -5 （/tmp 目录前5项）
请为每条命令分别报告执行结果。"
```

**步骤 2 — 验证输出合理性**

检查 Agent 响应中：
- 是否包含系统内核信息（如 `Darwin` 或 `Linux`）
- 是否包含当前用户名
- 是否包含 `/tmp` 目录列表片段

### 预期输出

1. Agent 响应中包含调用 `bash` 工具的证据
2. 响应中包含 `uname -a` 的实际输出（如 `Darwin` / `Linux` + 内核版本）
3. 响应中包含 `whoami` 的实际输出（当前用户名）
4. 响应中包含 `/tmp` 目录列表信息
5. Agent 对结果进行了总结或结构化展示

### 评判标准

| 条件 | Pass/Fail |
|------|-----------|
| Agent 调用了 `bash` 工具（至少 1 次） | **必须 Pass** |
| 响应中包含真实系统信息（非虚构） | **必须 Pass** |
| 响应中包含真实用户名 | **必须 Pass** |
| 命令退出码为 0 | **必须 Pass** |
| Agent 对输出进行了结构化总结 | 加分项 |

**总评**: 前 4 项全部 Pass 则任务通过。

### 故障排除

| 症状 | 原因 | 解决方案 |
|------|------|---------|
| Agent 拒绝执行 bash 命令 | AutonomyLevel 设置过低 | 检查配置中 `security.autonomy_level`，应至少为 `supervised` |
| Agent 虚构输出而非实际执行 | LLM 未调用工具直接编造 | 使用 `--output json` 检查工具调用记录 |
| bash 命令被安全策略拦截 | CommandRiskLevel 评估为高风险 | `uname` / `whoami` / `ls` 应为低风险，检查 SecurityPolicy 配置 |

---

## C3: 多步文件操作链

### 基本信息

| 属性 | 值 |
|------|---|
| 任务 ID | C3 |
| 名称 | 多步文件操作链 |
| 难度 | Medium |
| 评估维度 | 多工具协调（file_write + file_edit + grep + file_read）、任务分解与规划 |

### 前置条件

- CLI 已编译并通过健康检查
- `ANTHROPIC_API_KEY` 已配置
- `$EVAL_DIR` 临时目录已创建

### 执行步骤

**步骤 1 — 给出多步任务指令**

```bash
octo ask "请完成以下多步文件操作任务，所有文件放在 $EVAL_DIR/ 目录下：

1. 创建文件 config.toml，内容为：
[server]
host = \"127.0.0.1\"
port = 3000
debug = true

[database]
url = \"sqlite:///tmp/test.db\"
max_connections = 5

2. 使用 grep 查找 config.toml 中包含数字的所有行
3. 将 config.toml 中的 port 从 3000 修改为 8080，debug 从 true 改为 false
4. 读取修改后的文件，确认修改已生效

请按顺序执行，每步报告结果。"
```

**步骤 2 — 独立验证最终文件状态**

```bash
# 验证文件存在
test -f "$EVAL_DIR/config.toml" && echo "FILE_EXISTS=true" || echo "FILE_EXISTS=false"

# 验证端口已修改
grep "port = 8080" "$EVAL_DIR/config.toml" && echo "PORT_UPDATED=true" || echo "PORT_UPDATED=false"

# 验证 debug 已修改
grep "debug = false" "$EVAL_DIR/config.toml" && echo "DEBUG_UPDATED=true" || echo "DEBUG_UPDATED=false"

# 验证其他内容未被破坏
grep "max_connections = 5" "$EVAL_DIR/config.toml" && echo "INTACT=true" || echo "INTACT=false"
```

### 预期输出

1. Agent 创建了 `config.toml` 文件
2. Agent 执行了 grep 搜索并报告了包含数字的行（如 `port = 3000`, `max_connections = 5` 等）
3. Agent 将 `port` 修改为 `8080`，`debug` 修改为 `false`
4. Agent 读取并确认了最终文件内容
5. 其余内容（如 `host`, `url`, `max_connections`）保持不变

### 评判标准

| 条件 | Pass/Fail |
|------|-----------|
| 文件 `$EVAL_DIR/config.toml` 存在 | **必须 Pass** |
| 文件中 `port = 8080` | **必须 Pass** |
| 文件中 `debug = false` | **必须 Pass** |
| `max_connections = 5` 和 `host = "127.0.0.1"` 未被破坏 | **必须 Pass** |
| Agent 使用了至少 3 种不同的工具 | **必须 Pass** |
| Agent 对 grep 结果进行了正确报告 | 加分项 |
| 命令退出码为 0 | **必须 Pass** |

**总评**: 前 5 项必须项全部 Pass 则任务通过。

### 清理

```bash
rm -f "$EVAL_DIR/config.toml"
```

### 故障排除

| 症状 | 原因 | 解决方案 |
|------|------|---------|
| Agent 一步完成所有修改（未分步） | Agent 优化了执行路径 | 可接受，只要最终结果正确 |
| file_edit 失败 | 文件路径错误或工具参数格式不对 | 检查 Agent 传给 file_edit 的 JSON 参数 |
| grep 结果为空 | Agent 使用了错误的路径 | 确认 `$EVAL_DIR` 变量在 Agent 上下文中正确展开 |
| 修改后内容格式被破坏 | file_edit 操作范围过大 | 这是 Agent 质量问题，标记为 Fail |

---

## C4: 代码生成与验证

### 基本信息

| 属性 | 值 |
|------|---|
| 任务 ID | C4 |
| 名称 | 代码生成与验证 |
| 难度 | Medium |
| 评估维度 | 代码生成质量、自我验证能力（生成 -> 编译/运行 -> 确认） |

### 前置条件

- CLI 已编译并通过健康检查
- `ANTHROPIC_API_KEY` 已配置
- `$EVAL_DIR` 临时目录已创建
- 系统中安装了 Python 3（`python3 --version` 可用）

### 执行步骤

**步骤 1 — 让 Agent 生成并验证代码**

```bash
octo ask "请完成以下任务：

1. 在 $EVAL_DIR/fibonacci.py 中编写一个 Python 脚本，包含：
   - 一个函数 fibonacci(n) 返回第 n 个斐波那契数（从 0 开始，fibonacci(0)=0, fibonacci(1)=1）
   - 主程序打印 fibonacci(0) 到 fibonacci(10) 的值，每行格式为 'fib(N) = VALUE'
2. 用 bash 执行 python3 $EVAL_DIR/fibonacci.py 并报告输出
3. 确认输出中 fib(10) = 55"
```

**步骤 2 — 独立验证**

```bash
# 验证文件存在
test -f "$EVAL_DIR/fibonacci.py" && echo "FILE_EXISTS=true" || echo "FILE_EXISTS=false"

# 独立运行
python3 "$EVAL_DIR/fibonacci.py"

# 验证关键输出
python3 "$EVAL_DIR/fibonacci.py" | grep "fib(10)" | grep "55" && echo "CORRECT=true" || echo "CORRECT=false"

# 验证所有值（可选）
python3 "$EVAL_DIR/fibonacci.py" | head -11
# 预期:
# fib(0) = 0
# fib(1) = 1
# fib(2) = 1
# fib(3) = 2
# fib(4) = 3
# fib(5) = 5
# fib(6) = 8
# fib(7) = 13
# fib(8) = 21
# fib(9) = 34
# fib(10) = 55
```

### 预期输出

1. Agent 生成了 `fibonacci.py` 文件
2. Agent 使用 bash 工具运行了该脚本
3. Agent 报告了运行输出，其中 `fib(10) = 55`
4. 独立运行脚本输出正确的斐波那契数列

### 评判标准

| 条件 | Pass/Fail |
|------|-----------|
| 文件 `$EVAL_DIR/fibonacci.py` 存在 | **必须 Pass** |
| 独立运行 `python3 fibonacci.py` 无错误退出 | **必须 Pass** |
| 输出中 `fib(0) = 0` | **必须 Pass** |
| 输出中 `fib(10) = 55` | **必须 Pass** |
| Agent 在响应中自行运行了代码并报告结果 | **必须 Pass** |
| 代码风格合理（有函数定义、非硬编码） | 加分项 |

**总评**: 前 5 项必须项全部 Pass 则任务通过。

### 清理

```bash
rm -f "$EVAL_DIR/fibonacci.py"
```

### 故障排除

| 症状 | 原因 | 解决方案 |
|------|------|---------|
| Agent 未运行代码就声称正确 | Agent 跳过了 bash 调用 | 使用 `--output json` 检查是否确实调用了 bash 工具 |
| Python 脚本语法错误 | 代码生成质量问题 | 标记为 Fail，记录具体错误 |
| 输出格式不完全匹配 | Agent 使用了不同的格式字符串 | 只要数值正确，格式轻微偏差可接受（如 `fib(10)=55` vs `fib(10) = 55`） |
| `python3` 未找到 | 环境缺失 | 安装 Python 3 或改用 `python` 命令 |

---

## C5: 记忆存取一致性

### 基本信息

| 属性 | 值 |
|------|---|
| 任务 ID | C5 |
| 名称 | 记忆存取一致性 |
| 难度 | Medium |
| 评估维度 | `memory add` / `memory search` / `memory list` 命令的数据持久化和检索一致性 |

### 前置条件

- CLI 已编译并通过健康检查
- 使用独立的测试数据库以避免污染：

```bash
export OCTO_DB_PATH="$EVAL_DIR/eval_memory.db"
```

### 执行步骤

**步骤 1 — 存储记忆条目**

```bash
# 存储三条不同类别的记忆
octo memory add "用户偏好: 喜欢暗色主题，字体大小 14px" --tags "preferences"
octo memory add "调试记录: API 超时问题已通过增加 retry 解决" --tags "debug"
octo memory add "技术模式: 使用 Repository Pattern 管理数据访问层" --tags "patterns"
```

记录每条命令返回的 ID。

**步骤 2 — 列出记忆条目**

```bash
octo memory list --limit 10
```

**步骤 3 — 搜索记忆条目**

```bash
# 搜索与"主题"相关的记忆
octo memory search "暗色主题"

# 搜索与"API"相关的记忆
octo memory search "API 超时"

# 搜索与 "Repository" 相关的记忆
octo memory search "Repository Pattern"
```

**步骤 4 — 通过 Agent 检索记忆（可选，需要 ANTHROPIC_API_KEY）**

```bash
octo ask "请使用 memory_search 工具搜索与'暗色主题'相关的记忆，并报告搜索结果。"
```

### 预期输出

1. **步骤 1**: 每条 `memory add` 返回 `Memory added (id: <uuid>): <content>`
2. **步骤 2**: `memory list` 显示至少 3 条记忆，包含 ID、Category、Importance、Created、Content 列
3. **步骤 3**: 每次 `memory search` 返回包含对应内容的搜索结果
4. **步骤 4**: Agent 通过工具找到并报告了相关记忆

### 评判标准

| 条件 | Pass/Fail |
|------|-----------|
| 3 条 `memory add` 全部成功（返回 ID） | **必须 Pass** |
| `memory list` 显示至少 3 条记忆 | **必须 Pass** |
| `memory search "暗色主题"` 返回包含"暗色主题"的结果 | **必须 Pass** |
| `memory search "API 超时"` 返回包含"API"相关的结果 | **必须 Pass** |
| `memory search "Repository Pattern"` 返回包含"Repository"的结果 | **必须 Pass** |
| 命令退出码均为 0 | **必须 Pass** |
| Agent 能通过 `memory_search` 工具检索到记忆（步骤 4） | 加分项 |

**总评**: 前 6 项必须项全部 Pass 则任务通过。

### 清理

```bash
rm -f "$EVAL_DIR/eval_memory.db"
unset OCTO_DB_PATH
```

### 故障排除

| 症状 | 原因 | 解决方案 |
|------|------|---------|
| `memory add` 报 database error | SQLite 文件路径不可写 | 确认 `$EVAL_DIR` 目录存在且可写 |
| `memory search` 返回 "No results" | 全文搜索未匹配 | 尝试用更短的关键词搜索（如单个词"主题"） |
| `memory list` 显示 0 条 | 数据库路径不一致 | 确认所有命令使用相同的 `OCTO_DB_PATH` |
| 搜索结果 score 过低 | 中文分词效果有限 | 属于已知限制，只要能返回结果即 Pass |

---

## C6: MCP 工具发现与调用

### 基本信息

| 属性 | 值 |
|------|---|
| 任务 ID | C6 |
| 名称 | MCP 工具发现与调用 |
| 难度 | Hard |
| 评估维度 | MCP 服务器注册、工具发现、工具列表集成、Agent 端到端调用 |

### 前置条件

- CLI 已编译并通过健康检查
- `ANTHROPIC_API_KEY` 已配置
- Node.js 18+ 已安装（用于运行 MCP 测试服务器）
- 安装 MCP 测试服务器：

```bash
# 安装 everything MCP server（官方测试服务器）
npm install -g @anthropic-ai/mcp-everything
# 或使用 npx 方式（无需全局安装，在 add 命令中直接使用 npx）
```

### 执行步骤

**步骤 1 — 注册 MCP 服务器**

```bash
octo mcp add everything npx @anthropic-ai/mcp-everything
```

**步骤 2 — 验证服务器注册成功**

```bash
# 列出所有 MCP 服务器
octo mcp list

# 查看服务器状态和工具列表
octo mcp status everything
```

**步骤 3 — 验证工具已集成到工具注册表**

```bash
# 列出所有工具（应包含 MCP 工具）
octo tool list
```

**步骤 4 — 直接调用 MCP 工具**

```bash
# 使用 tool invoke 直接调用 MCP 服务器提供的 echo 工具
octo tool invoke echo '{"message": "hello from octo cli"}'
```

**步骤 5 — 通过 Agent 调用 MCP 工具**

```bash
octo ask "请调用 echo 工具，传入 message 参数为 'MCP integration test'，并报告返回结果。"
```

**步骤 6 — 清理 MCP 服务器**

```bash
octo mcp remove everything
octo mcp list
# 预期: 列表中不再包含 everything
```

### 预期输出

1. **步骤 1**: 输出 `Added MCP server 'everything' (N tools)`，列出发现的工具名称
2. **步骤 2**: `mcp list` 显示 everything 服务器及其工具数量；`mcp status` 显示 Running 状态和工具列表
3. **步骤 3**: `tool list` 结果中包含 MCP 服务器提供的工具（如 `echo`, `get-sum` 等）
4. **步骤 4**: `tool invoke echo` 返回 `[OK] echo: hello from octo cli`（或类似的成功响应）
5. **步骤 5**: Agent 成功调用了 echo 工具并返回了结果
6. **步骤 6**: 服务器被移除，`mcp list` 不再显示 everything

### 评判标准

| 条件 | Pass/Fail |
|------|-----------|
| `mcp add` 成功注册服务器并发现工具 | **必须 Pass** |
| `mcp list` 显示 everything 服务器 | **必须 Pass** |
| `mcp status everything` 显示 Running 状态 | **必须 Pass** |
| `tool list` 中出现 MCP 服务器提供的工具 | **必须 Pass** |
| `tool invoke echo` 返回正确结果 | **必须 Pass** |
| Agent 通过 `octo ask` 成功调用 MCP 工具 | **必须 Pass** |
| `mcp remove` 成功移除服务器 | **必须 Pass** |
| 所有命令退出码为 0 | **必须 Pass** |

**总评**: 全部 8 项 Pass 则任务通过。

### 备选 MCP 服务器

如果 `@anthropic-ai/mcp-everything` 不可用，可使用其他 stdio MCP 服务器：

```bash
# 方案 B: 使用 fetch MCP server
octo mcp add fetch-server npx @anthropic-ai/mcp-fetch

# 方案 C: 使用自定义 echo 服务器（Python）
# 先创建一个最简 MCP 服务器脚本:
cat > "$EVAL_DIR/echo_server.py" << 'PYEOF'
import sys, json

def main():
    for line in sys.stdin:
        req = json.loads(line.strip())
        if req.get("method") == "initialize":
            resp = {"jsonrpc": "2.0", "id": req["id"], "result": {"capabilities": {"tools": {}}}}
        elif req.get("method") == "tools/list":
            resp = {"jsonrpc": "2.0", "id": req["id"], "result": {"tools": [
                {"name": "echo", "description": "Echo a message", "inputSchema": {"type": "object", "properties": {"message": {"type": "string"}}}}
            ]}}
        elif req.get("method") == "tools/call":
            msg = req.get("params", {}).get("arguments", {}).get("message", "")
            resp = {"jsonrpc": "2.0", "id": req["id"], "result": {"content": [{"type": "text", "text": msg}]}}
        else:
            resp = {"jsonrpc": "2.0", "id": req.get("id"), "result": {}}
        sys.stdout.write(json.dumps(resp) + "\n")
        sys.stdout.flush()

if __name__ == "__main__":
    main()
PYEOF

octo mcp add echo-local python3 "$EVAL_DIR/echo_server.py"
```

### 故障排除

| 症状 | 原因 | 解决方案 |
|------|------|---------|
| `mcp add` 超时 | MCP 服务器启动慢或 npx 下载中 | 先手动运行 `npx @anthropic-ai/mcp-everything` 确认可启动 |
| `mcp add` 报 "connection refused" | 服务器进程立即退出 | 检查 Node.js 版本；手动运行命令查看错误 |
| `tool list` 未显示 MCP 工具 | 工具桥接注册失败 | 查看 `octo mcp status everything` 确认状态 |
| `tool invoke echo` 返回 "Tool not found" | 工具名称不匹配 | 先用 `octo tool list` 确认实际工具名（可能带前缀） |
| Agent 未调用 MCP 工具 | Agent 选择了内置工具 | 在 prompt 中明确指定工具名（如 `echo`） |
| `npx` 命令不存在 | Node.js 未安装 | 安装 Node.js 18+，或使用备选 Python 方案 |

---

## 评估结果汇总模板

使用以下模板记录评估结果：

```markdown
# Octo CLI 评估报告

- 评估日期: YYYY-MM-DD
- CLI 版本: (octo --version 输出)
- 操作系统: (uname -a 输出)
- LLM 提供商: anthropic / openai
- 模型: claude-sonnet-4-20250514 / 其他

## 评估结果

| 任务 | 难度 | 结果 | 耗时 | 备注 |
|------|------|------|------|------|
| C1: 文件读写往返 | Easy | Pass/Fail | Xm | |
| C2: Bash 命令执行 | Easy | Pass/Fail | Xm | |
| C3: 多步文件操作链 | Medium | Pass/Fail | Xm | |
| C4: 代码生成与验证 | Medium | Pass/Fail | Xm | |
| C5: 记忆存取一致性 | Medium | Pass/Fail | Xm | |
| C6: MCP 工具发现与调用 | Hard | Pass/Fail | Xm | |

## 总评

- 通过: X/6
- 通过率: XX%

## 发现的问题

1. (问题描述)
2. (问题描述)

## 改进建议

1. (建议描述)
2. (建议描述)
```

---

## 附录: 全局清理

评估完成后清理所有临时文件：

```bash
rm -rf "$EVAL_DIR"
unset EVAL_DIR
unset OCTO_DB_PATH
```
