# P1-6 web-platform 前端设计方案

> 日期：2026-03-04
> 状态：设计完成

---

## 一、技术栈

- **框架**: React 19 + TypeScript
- **构建**: Vite 6
- **样式**: TailwindCSS 4 + clsx + tailwind-merge
- **状态**: Jotai (与 workbench 统一)
- **图标**: Lucide React
- **HTTP**: Fetch API + 拦截器
- **WebSocket**: 原生 WebSocket + 重连逻辑

---

## 二、目录结构

```
web-platform/
├── src/
│   ├── main.tsx                 # 入口
│   ├── App.tsx                  # 根组件 + 路由
│   ├── globals.css              # 全局样式
│   │
│   ├── api/                     # API 层
│   │   ├── index.ts            # fetch 封装 + 拦截器
│   │   ├── auth.ts             # 认证 API
│   │   ├── sessions.ts         # 会话 API
│   │   └── users.ts            # 用户 API
│   │
│   ├── ws/                     # WebSocket
│   │   ├── manager.ts          # 连接管理 + 重连
│   │   ├── events.ts           # 事件类型
│   │   └── types.ts            # 类型定义
│   │
│   ├── atoms/                  # Jotai 状态
│   │   ├── auth.ts             # 认证状态 (token, user)
│   │   ├── sessions.ts         # 会话列表
│   │   ├── chat.ts             # 当前会话消息
│   │   └── ui.ts               # UI 状态
│   │
│   ├── components/             # 组件
│   │   ├── layout/
│   │   │   ├── AppLayout.tsx  # 主布局
│   │   │   ├── NavRail.tsx    # 左侧导航
│   │   │   └── Header.tsx     # 顶部 header
│   │   │
│   │   ├── auth/
│   │   │   ├── LoginForm.tsx  # 登录表单
│   │   │   └── ProtectedRoute.tsx # 路由守卫
│   │   │
│   │   ├── dashboard/
│   │   │   ├── Welcome.tsx    # 欢迎区域
│   │   │   ├── StatsCard.tsx  # 统计卡片
│   │   │   ├── RecentSessions.tsx # 最近会话
│   │   │   └── QuickActions.tsx # 快捷操作
│   │   │
│   │   └── chat/
│   │       ├── ChatPage.tsx   # 聊天页面
│   │       ├── MessageList.tsx # 消息列表
│   │       ├── MessageBubble.tsx # 单条消息
│   │       ├── ChatInput.tsx  # 输入框
│   │       └── StreamingDisplay.tsx # 流式响应
│   │
│   └── pages/                  # 页面
│       ├── Login.tsx          # 登录页
│       ├── Dashboard.tsx      # Dashboard 页
│       ├── Chat.tsx           # 聊天页
│       └── Sessions.tsx       # 会话列表页
│
├── index.html
├── package.json
├── tsconfig.json
├── vite.config.ts
└── tailwind.config.js
```

---

## 三、认证流程

1. **登录** POST /api/auth/login
   - 返回 access_token + refresh_token + user

2. **存储 token** (localStorage)
   - access_token: 短期 (15min)
   - refresh_token: 长期 (7d)

3. **请求拦截器**
   - 每次请求带 Authorization: Bearer {token}

4. **Token 过期处理**
   - 401 时自动用 refresh_token 刷新
   - 刷新成功继续原请求
   - 刷新失败跳转登录页

5. **页面加载恢复**
   - 检查 localStorage 中的 token
   - 验证 token 有效性
   - 有效则自动登录

---

## 四、核心模块设计

### 4.1 Auth 模块

**atoms/auth.ts**
- authState: atom<User | null>
- tokenAtom: atom<string | null>
- isAuthenticated: atom<boolean>
- refreshTokenAtom: atom<string | null>

**api/auth.ts**
- login(email, password) → LoginResponse
- register(email, password, displayName?)
- refresh(refreshToken) → LoginResponse
- logout() → void

### 4.2 Session 模块

**atoms/sessions.ts**
- sessionsAtom: atom<Session[]>
- currentSessionIdAtom: atom<string | null>
- currentSessionAtom: atom<Session | null>

**api/sessions.ts**
- listSessions() → Session[]
- createSession(name?) → Session
- getSession(id) → Session
- deleteSession(id) → void

### 4.3 Chat + WebSocket 模块

**atoms/chat.ts**
- messagesAtom: atom<Message[]>
- inputAtom: atom<string>
- isStreamingAtom: atom<boolean>
- isConnectedAtom: atom<boolean>

**ws/manager.ts**
- connect(sessionId, token)
- disconnect()
- send(message)
- onMessage(handler)
- onDisconnect(handler)

**自动重连逻辑**
- 断线后 1s, 2s, 4s, 8s, 16s 指数退避
- 最大重试 5 次
- 心跳: 30s 发送 ping

**WebSocket 消息格式**
```typescript
{ type: "message", content: "...", role: "user" | "assistant" }
{ type: "streaming_start" }
{ type: "streaming_chunk", delta: "..." }
{ type: "streaming_end" }
{ type: "error", message: "..." }
```

---

## 五、页面布局

### Dashboard

- Header: "Welcome, {name}" + [Avatar] [Logout]
- 统计卡片: Sessions / Messages / Agents 数量
- 最近会话列表
- [+ New Chat] 按钮

### Chat

- Header: [←Back] Session Name [⋮]
- MessageList: 消息气泡 + 流式响应
- ChatInput: 输入框 + [Send] + 停止按钮

---

## 六、API 类型定义

```typescript
// User
interface User {
  id: string;
  email: string;
  display_name: string;
  role: 'admin' | 'member' | 'viewer';
  created_at: string;
}

interface LoginResponse {
  access_token: string;
  refresh_token: string;
  user: User;
}

// Session
interface Session {
  id: string;
  user_id: string;
  name: string | null;
  status: 'active' | 'paused' | 'completed';
  created_at: string;
  updated_at: string;
}

// WebSocket
type WsEventType = 'message' | 'streaming_start' | 'streaming_chunk' | 'streaming_end' | 'error';
```

---

## 七、Vite 配置

```typescript
// vite.config.ts
export default defineConfig({
  server: {
    port: 5180,
    proxy: {
      '/api': { target: 'http://127.0.0.1:3002' },
      '/ws': { target: 'ws://127.0.0.1:3002', ws: true },
    },
  },
});
```

---

## 八、实施任务

| ID | 任务 | 内容 |
|----|------|------|
| P1-6-T1 | 项目初始化 | package.json, vite.config, tsconfig, 目录结构 |
| P1-6-T2 | API 层 | fetch 封装, auth/sessions/users API |
| P1-6-T3 | 认证模块 | atoms/auth, LoginForm, ProtectedRoute |
| P1-6-T4 | Dashboard | 页面 + 组件 |
| P1-6-T5 | WebSocket | manager + 重连 + 心跳 |
| P1-6-T6 | Chat | 页面 + 消息列表 + 流式响应 |
| P1-6-T7 | Sessions | 会话列表页面 |
| P1-6-T8 | 集成测试 | 端到端测试 |
