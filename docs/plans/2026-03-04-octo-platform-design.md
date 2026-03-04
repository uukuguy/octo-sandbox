# octo-platform 设计方案

> 日期：2026-03-04
> 状态：设计进行中（已确认：产品定位、架构总览、目录结构、前端策略）
> 基础：octo-workbench v1.0 冲刺完成后开始实施

---

## 一、产品定位

**octo-platform** 是基于 `octo-engine` 的企业级多租户多 Agent 平台，与 octo-workbench 独立演进，共享核心引擎。

### 三种部署模式

| 模式 | 场景 | 租户概念 |
|------|------|---------|
| **SaaS 平台** | 多企业订阅，云端部署 | 租户 = 企业/组织 |
| **企业私有部署** | 单企业内网，多团队使用 | 租户 = 部门/团队（可退化为单租户） |
| **开发者 API 平台** | 开发者通过 API Key 调用 | 租户 = 开发者账号 |

### 与 octo-workbench 的关系

```
octo-types    ← 共享类型（两产品都用，不修改）
octo-engine   ← 共享核心引擎（两产品都用，持续完善）
     ↙                    ↘
octo-workbench            octo-platform
（单用户单实例）            （多租户多用户多Agent）
branch: octo-workbench    branch: octo-platform
独立演进                   独立演进
```

---

## 二、两级隔离架构

### 场景对应关系

**场景 A：SaaS 部署**（需两级隔离）
```
octo-platform (单实例)
├── 租户 A（公司甲）—— 独立 DB、独立 MCP 配置、独立配额
│   ├── 用户1：独立 AgentRuntime、记忆、会话
│   └── 用户2：独立 AgentRuntime、记忆、会话
└── 租户 B（公司乙）—— 与租户A完全隔离
```

**场景 B：企业私有部署**（租户退化为单个组织）
```
octo-platform（公司内网）
└── 租户：公司（单一）
    ├── 用户1（工程团队）
    └── 用户2（产品团队）
```

**场景 C：开发者 API 平台**（需两级隔离）
```
octo-platform
├── 开发者 A（租户）→ API Key 调用，终端用户是其用户
└── 开发者 B（租户）→ API Key 调用，终端用户是其用户
```

### 架构总览

```
┌─────────────────────────────────────────────────────┐
│                  octo-platform                       │
│  ┌──────────────┬──────────────┬──────────────────┐  │
│  │  SaaS 模式   │  企业私有部署 │  开发者 API 平台  │  │
│  └──────────────┴──────────────┴──────────────────┘  │
│                                                       │
│  ┌─────────────────────────────────────────────────┐  │
│  │  租户层（TenantRuntime）                         │  │
│  │  ├── 独立 DB schema / SQLite 文件               │  │
│  │  ├── 独立 MCP 服务器配置                        │  │
│  │  ├── 资源配额（Agent数/调用数/存储）             │  │
│  │  └── 租户级 API Key 管理                        │  │
│  └─────────────────────────────────────────────────┘  │
│  ┌─────────────────────────────────────────────────┐  │
│  │  用户层（UserRuntime）                           │  │
│  │  ├── 独立 AgentRuntime（复用 octo-engine）       │  │
│  │  ├── 独立会话历史 + WorkingMemory               │  │
│  │  ├── 独立长期记忆空间                            │  │
│  │  └── 个人 Agent 配置和工具过滤                  │  │
│  └─────────────────────────────────────────────────┘  │
│                                                       │
│  Multi-Agent 编排层（AgentOrchestrator）              │
│  ├── Supervisor + Workers 模式                        │
│  ├── Peer-to-Peer 消息路由                            │
│  └── Pipeline DAG 执行引擎                            │
└─────────────────────────────────────────────────────┘
```

---

## 三、Mono-Repo 目录结构

```
octo-sandbox/                        ← git 仓库根
├── crates/
│   ├── octo-types/                  ← 共享类型（两产品）
│   ├── octo-engine/                 ← 共享核心引擎（两产品）
│   ├── octo-server/                 ← workbench 专用（workbench 分支）
│   └── octo-platform-server/        ← platform 专用（platform 分支，新增）
│       ├── api/                     # REST + WebSocket
│       ├── tenant/                  # TenantManager, TenantRuntime
│       ├── user/                    # UserManager, UserRuntime
│       ├── quota/                   # 资源配额
│       ├── auth/                    # JWT + OAuth2/OIDC
│       ├── orchestrator/            # Multi-Agent DAG 执行
│       └── audit/                   # 操作审计
│
├── web/                             ← workbench 前端（workbench 分支）
├── web-platform/                    ← platform 前端（platform 分支，新增）
│   └── src/
│       ├── admin/                   # 租户/用户/配额管理
│       ├── workspace/               # 用户 Agent 工作空间
│       └── orchestrator/            # DAG 可视化
│
└── design/                          ← 共享前端设计 token（两分支共用）
    ├── tailwind.base.ts             # 共享 Tailwind 基础配置
    └── tokens.css                   # CSS 变量（颜色、字体、间距）
```

---

## 四、认证系统

### 两阶段实现

**开发阶段：** 自建用户系统
- 用户名/密码 + JWT
- 平台自己管理账号
- 简单可控，快速启动

**企业部署阶段：** 插拔式 OAuth2/OIDC
- 对接 Google、GitHub、Okta、Azure AD 等企业 SSO
- 配置式接入，不重复造轮子
- 本地账号 + 外部 SSO 可同时存在

### 权限分层

```
超级管理员（Platform Admin）
  └── 租户管理员（Tenant Admin）
        └── 普通用户（Member）
              └── 只读用户（Viewer）
```

---

## 五、前端策略

### 原则

- **不共享业务组件**：Chat、Memory、Tools 等在两个产品中逻辑有差异，各自独立实现
- **共享设计 token**：颜色、字体、间距通过 `design/` 目录统一
- **复制基础 UI 原语**：Button、Input、Badge、Modal 等无业务逻辑的组件，platform 启动时从 workbench 复制一次，之后独立演进

### 设计 Token 结构

**`design/tailwind.base.ts`（共享）：**
```typescript
export const baseConfig = {
  theme: {
    extend: {
      colors: {
        primary:   { DEFAULT: 'var(--color-primary)' },
        secondary: { DEFAULT: 'var(--color-secondary)' },
        surface:   { DEFAULT: 'var(--color-surface)' },
        border:    { DEFAULT: 'var(--color-border)' },
      },
      fontFamily: {
        sans: ['Inter', 'system-ui', 'sans-serif'],
        mono: ['JetBrains Mono', 'monospace'],
      },
      borderRadius: {
        sm: '4px', md: '8px', lg: '12px',
      },
    },
  },
}
```

**`design/tokens.css`（共享）：**
```css
:root {
  --color-primary:   #6366f1;
  --color-secondary: #8b5cf6;
  --color-surface:   #ffffff;
  --color-border:    #e5e7eb;
  --color-text:      #111827;
}
[data-theme="dark"] {
  --color-surface: #1f2937;
  --color-border:  #374151;
  --color-text:    #f9fafb;
}
```

各自前端的 `tailwind.config.ts` extends `design/tailwind.base.ts`，无需 npm 包，相对路径引入。

---

## 六、待设计（下一节）

- [ ] Multi-Agent 编排层（三种模式详细设计）
- [ ] 核心数据模型（Tenant、PlatformUser、AgentGraph）
- [ ] API 设计（REST + WebSocket）
- [ ] 实施计划（Phase 1-4）

---

## 设计决策记录

| 决策 | 选择 | 理由 |
|------|------|------|
| 产品关系 | 独立产品共享 octo-engine | 避免向下兼容负担，各自独立演进 |
| 仓库结构 | Mono-repo，未来可拆 | 引擎未成熟时共同演进更高效 |
| 前端共享策略 | 不共享业务组件，只共享设计 token | 避免 props 爆炸，YAGNI |
| 认证 | 自建 + 插拔式 SSO | 开发快，企业部署灵活 |
| 隔离粒度 | 租户级 + 用户级两层 | 覆盖三种部署模式 |
| 编排模式 | Supervisor/Peer/Pipeline 三种 | 按场景选择，用户自由配置拓扑 |
