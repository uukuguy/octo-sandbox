# octo-workbench Phase 2.1: 核心闭环可用 Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** 验证后端服务运行时正确 + 基础调试能力（Timeline + JsonViewer）

**Architecture:** 后端服务已完成编码，需要运行时验证确保各模块正常工作；前端需要完善 TimelineView 和 JsonViewer 组件实现完整的工具执行检查器。

**Tech Stack:** Rust (tokio, axum, tokio-rusqlite), TypeScript (React, Jotai), SQLite (WAL mode)

---

## Batch 1.1: 运行时验证

### Task 1: 服务器启动验证

**Files:**
- Modify: `crates/octo-server/src/main.rs`
- Test: 手动启动验证

**Step 1: 验证编译**

```bash
cargo check --workspace
```

Expected: 编译通过

**Step 2: 启动服务器**

```bash
cargo run --bin octo-server
```

Expected: `octo-server listening on 127.0.0.1:3001`

**Step 3: 验证 Health 端点**

```bash
curl http://127.0.0.1:3001/api/health
```

Expected: `{"status":"ok"}`

**Step 4: 验证 WebSocket 连接**

使用 ws://127.0.0.1:3001/ws 连接

Expected: 连接成功

**Step 5: 提交**

```bash
git add -A
git commit -m "chore: verify server startup and health endpoint"
```

---

### Task 2: AI 对话功能验证

**Files:**
- Test: WebSocket 消息交互

**Step 1: 创建 Session**

```bash
curl -X POST http://127.0.0.1:3001/api/sessions
```

Expected: 返回 session_id

**Step 2: 发送聊天消息**

通过 WebSocket 发送：
```json
{"type":"chat","session_id":"xxx","message":"你好"}
```

Expected: 收到 Agent 回复

**Step 3: 验证工具执行**

发送需要工具的消息：
```json
{"type":"chat","session_id":"xxx","message":"列出当前目录文件"}
```

Expected: Agent 调用 bash/ls 工具，返回文件列表

**Step 4: 提交**

```bash
git commit -m "chore: verify AI chat and tool execution"
```

---

### Task 3: 工具执行记录验证

**Files:**
- Test: REST API 验证

**Step 1: 查看执行记录列表**

```bash
curl http://127.0.0.1:3001/api/sessions/{session_id}/executions
```

Expected: 返回工具执行记录列表

**Step 2: 查看单条执行详情**

```bash
curl http://127.0.0.1:3001/api/executions/{execution_id}
```

Expected: 返回完整执行信息（输入、输出、状态、时间）

**Step 3: 提交**

```bash
git commit -m "chore: verify tool execution recording API"
```

---

## Batch 1.2: 执行记录完善 (Timeline + JsonViewer)

### Task 4: 前端 TimelineView 组件

**Files:**
- Create: `web/src/components/tools/TimelineView.tsx`
- Modify: `web/src/pages/Tools.tsx`
- Modify: `web/src/components/tools/ExecutionDetail.tsx`

**Step 1: 创建 TimelineView 组件**

创建文件 `web/src/components/tools/TimelineView.tsx`：

```tsx
import { useMemo } from 'react';

interface TimelineEvent {
  id: string;
  timestamp: number;
  type: 'start' | 'tool_call' | 'tool_result' | 'end' | 'error';
  toolName?: string;
  duration?: number;
  data?: unknown;
}

interface TimelineViewProps {
  events: TimelineEvent[];
}

export function TimelineView({ events }: TimelineViewProps) {
  const sorted = useMemo(() =>
    [...events].sort((a, b) => a.timestamp - b.timestamp),
    [events]
  );

  return (
    <div className="timeline-view">
      {sorted.map((event, idx) => (
        <div key={event.id} className={`timeline-event timeline-${event.type}`}>
          <div className="timeline-marker" />
          <div className="timeline-content">
            <span className="timeline-time">
              {new Date(event.timestamp).toLocaleTimeString()}
            </span>
            <span className="timeline-label">
              {event.type === 'tool_call' && event.toolName}
              {event.type === 'tool_result' && `← ${event.toolName}`}
              {event.type === 'start' && '开始'}
              {event.type === 'end' && '结束'}
              {event.type === 'error' && '错误'}
            </span>
            {event.duration && (
              <span className="timeline-duration">{event.duration}ms</span>
            )}
          </div>
        </div>
      ))}
    </div>
  );
}
```

**Step 2: 添加 TimelineView 到 ExecutionDetail**

修改 `web/src/components/tools/ExecutionDetail.tsx`:

```tsx
import { TimelineView } from './TimelineView';

// 在组件中添加 TimelineView
<TimelineView events={executionTimeline} />
```

**Step 3: 样式补充**

添加 CSS 到 `web/src/globals.css`:

```css
.timeline-view {
  @apply flex flex-col gap-1 py-2;
}

.timeline-event {
  @apply flex items-center gap-2 text-sm;
}

.timeline-marker {
  @apply w-2 h-2 rounded-full;
}

.timeline-start .timeline-marker { @apply bg-green-500; }
.timeline-tool_call .timeline-marker { @apply bg-blue-500; }
.timeline-tool_result .timeline-marker { @apply bg-gray-500; }
.timeline-end .timeline-marker { @apply bg-gray-400; }
.timeline-error .timeline-marker { @apply bg-red-500; }

.timeline-time { @apply text-muted-foreground; }
.timeline-duration { @apply ml-auto text-muted-foreground; }
```

**Step 4: 提交**

```bash
git add web/src/components/tools/TimelineView.tsx web/src/components/tools/ExecutionDetail.tsx web/src/globals.css
git commit -m "feat(web): add TimelineView component for tool execution"
```

---

### Task 5: JsonViewer 组件

**Files:**
- Create: `web/src/components/tools/JsonViewer.tsx`
- Modify: `web/src/components/tools/ExecutionDetail.tsx`

**Step 1: 创建 JsonViewer 组件**

创建文件 `web/src/components/tools/JsonViewer.tsx`:

```tsx
import { useState } from 'react';

interface JsonViewerProps {
  data: unknown;
  name?: string;
}

function JsonValue({ value, depth = 0 }: { value: unknown; depth?: number }) {
  const [expanded, setExpanded] = useState(depth < 2);

  if (value === null) return <span className="text-muted-foreground">null</span>;
  if (typeof value === 'boolean') return <span className="text-blue-500">{value.toString()}</span>;
  if (typeof value === 'number') return <span className="text-green-600">{value}</span>;
  if (typeof value === 'string') return <span className="text-orange-600">"{value}"</span>;

  if (Array.isArray(value)) {
    if (value.length === 0) return <span>[]</span>;
    return (
      <span>
        <button onClick={() => setExpanded(!expanded)} className="hover:underline">
          [{expanded ? '▼' : '▶'} {value.length} items]
        </button>
        {expanded && (
          <div className="ml-4 border-l border-border">
            {value.map((item, i) => (
              <div key={i}>
                <span className="text-muted-foreground">{i}: </span>
                <JsonValue value={item} depth={depth + 1} />
              </div>
            ))}
          </div>
        )}
      </span>
    );
  }

  if (typeof value === 'object') {
    const entries = Object.entries(value);
    if (entries.length === 0) return <span>{'{}'}</span>;
    return (
      <span>
        <button onClick={() => setExpanded(!expanded)} className="hover:underline">
          {{'{'}}{expanded ? '▼' : '▶'} {entries.length} keys{{'}'}}
        </button>
        {expanded && (
          <div className="ml-4 border-l border-border">
            {entries.map(([k, v]) => (
              <div key={k}>
                <span className="text-purple-600">"{k}"</span>: <JsonValue value={v} depth={depth + 1} />
              </div>
            ))}
          </div>
        )}
      </span>
    );
  }

  return <span>{String(value)}</span>;
}

export function JsonViewer({ data, name }: JsonViewerProps) {
  return (
    <div className="json-viewer font-mono text-sm bg-muted p-2 rounded overflow-auto">
      {name && <div className="text-purple-600 mb-1">"{name}": </div>}
      <JsonValue value={data} />
    </div>
  );
}
```

**Step 2: 集成到 ExecutionDetail**

修改 `web/src/components/tools/ExecutionDetail.tsx`:

```tsx
import { JsonViewer } from './JsonViewer';

// 在 Input/Output 区域使用
<div className="space-y-2">
  <div className="text-sm font-medium">Input</div>
  <JsonViewer data={execution.input} />
</div>

<div className="space-y-2">
  <div className="text-sm font-medium">Output</div>
  <JsonViewer data={execution.output} />
</div>
```

**Step 3: 提交**

```bash
git add web/src/components/tools/JsonViewer.tsx web/src/components/tools/ExecutionDetail.tsx
git commit -m "feat(web): add JsonViewer component for structured data display"
```

---

### Task 6: 构建验证

**Step 1: 前端类型检查**

```bash
cd web && npx tsc --noEmit
```

Expected: 无错误

**Step 2: 前端构建**

```bash
npx vite build
```

Expected: 构建成功

**Step 3: 提交**

```bash
git commit -m "chore: verify frontend build"
```

---

## Phase 2.1 完成

**预期提交清单**:
1. `chore: verify server startup and health endpoint`
2. `chore: verify AI chat and tool execution`
3. `chore: verify tool execution recording API`
4. `feat(web): add TimelineView component for tool execution`
5. `feat(web): add JsonViewer component for structured data display`
6. `chore: verify frontend build`

---

## 验证命令

```bash
# 编译检查
cargo check --workspace

# 前端检查
cd web && npx tsc --noEmit

# 前端构建
cd web && npx vite build
```

---

## 后续批次

Batch 1.2 完成后，继续 **Batch 2.1: 5 memory tools（recall + forget）**
