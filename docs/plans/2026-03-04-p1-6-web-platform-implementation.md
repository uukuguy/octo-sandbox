# P1-6 web-platform 前端实施计划

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to implement this plan task-by-task.

**Goal:** 创建完整的 web-platform 前端应用，包含登录页、Dashboard、聊天界面和会话管理

**Architecture:** 基于 React 19 + Vite + TailwindCSS 4 + Jotai，使用 REST API 认证和 WebSocket 进行实时聊天

**Tech Stack:** React 19, TypeScript, Vite 6, TailwindCSS 4, Jotai, Lucide React

---

## Task 1: 初始化项目结构

**Files:**
- Create: `web-platform/package.json`
- Create: `web-platform/tsconfig.json`
- Create: `web-platform/vite.config.ts`
- Create: `web-platform/index.html`
- Create: `web-platform/tailwind.config.js`
- Create: `web-platform/postcss.config.js`
- Create: `web-platform/src/main.tsx`
- Create: `web-platform/src/App.tsx`
- Create: `web-platform/src/globals.css`

**Step 1: 创建 package.json**

```json
{
  "name": "octo-web-platform",
  "private": true,
  "version": "0.1.0",
  "type": "module",
  "scripts": {
    "dev": "vite",
    "build": "tsc -b && vite build",
    "preview": "vite preview"
  },
  "dependencies": {
    "react": "^19.0.0",
    "react-dom": "^19.0.0",
    "react-router-dom": "^7.1.0",
    "jotai": "^2.16.0",
    "lucide-react": "^0.469.0",
    "clsx": "^2.1.1",
    "tailwind-merge": "^3.0.2"
  },
  "devDependencies": {
    "@types/react": "^19.0.0",
    "@types/react-dom": "^19.0.0",
    "@vitejs/plugin-react": "^4.4.0",
    "typescript": "~5.7.0",
    "vite": "^6.0.0",
    "tailwindcss": "^4.0.0",
    "@tailwindcss/vite": "^4.0.0"
  }
}
```

**Step 2: 创建 tsconfig.json**

```json
{
  "compilerOptions": {
    "target": "ES2020",
    "useDefineForClassFields": true,
    "lib": ["ES2020", "DOM", "DOM.Iterable"],
    "module": "ESNext",
    "skipLibCheck": true,
    "moduleResolution": "bundler",
    "allowImportingTsExtensions": true,
    "isolatedModules": true,
    "moduleDetection": "force",
    "noEmit": true,
    "jsx": "react-jsx",
    "strict": true,
    "noUnusedLocals": true,
    "noUnusedParameters": true,
    "noFallthroughCasesInSwitch": true,
    "noUncheckedSideEffectImports": true
  },
  "include": ["src"]
}
```

**Step 3: 创建 vite.config.ts**

```typescript
import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';
import tailwindcss from '@tailwindcss/vite';

export default defineConfig({
  plugins: [react(), tailwindcss()],
  server: {
    port: 5180,
    proxy: {
      '/api': {
        target: 'http://127.0.0.1:3002',
        changeOrigin: true,
      },
      '/ws': {
        target: 'ws://127.0.0.1:3002',
        ws: true,
      },
    },
  },
});
```

**Step 4: 创建 index.html**

```html
<!DOCTYPE html>
<html lang="en">
  <head>
    <meta charset="UTF-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1.0" />
    <title>Octo Platform</title>
  </head>
  <body>
    <div id="root"></div>
    <script type="module" src="/src/main.tsx"></script>
  </body>
</html>
```

**Step 5: 创建 globals.css**

```css
@import "tailwindcss";

:root {
  --color-primary: #6366f1;
  --color-secondary: #8b5cf6;
}

body {
  margin: 0;
  font-family: Inter, system-ui, sans-serif;
}
```

**Step 6: 创建 main.tsx**

```tsx
import React from 'react';
import ReactDOM from 'react-dom/client';
import App from './App';
import './globals.css';

ReactDOM.createRoot(document.getElementById('root')!).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>
);
```

**Step 7: 创建 App.tsx (临时占位)**

```tsx
import { BrowserRouter, Routes, Route, Navigate } from 'react-router-dom';

function App() {
  return (
    <BrowserRouter>
      <Routes>
        <Route path="/" element={<div>Loading...</div>} />
      </Routes>
    </BrowserRouter>
  );
}

export default App;
```

**Step 8: 安装依赖并验证**

Run: `cd web-platform && npm install`
Expected: 安装成功

**Step 9: 验证构建**

Run: `cd web-platform && npm run build`
Expected: 构建成功，无错误

---

## Task 2: 创建 API 层和类型定义

**Files:**
- Create: `web-platform/src/api/types.ts`
- Create: `web-platform/src/api/index.ts`
- Create: `web-platform/src/api/auth.ts`
- Create: `web-platform/src/api/sessions.ts`
- Create: `web-platform/src/api/users.ts`

**Step 1: 创建 types.ts**

```typescript
// User types
export interface User {
  id: string;
  email: string;
  display_name: string;
  role: 'admin' | 'member' | 'viewer';
  created_at: string;
}

export interface LoginRequest {
  email: string;
  password: string;
}

export interface LoginResponse {
  access_token: string;
  refresh_token: string;
  user: User;
}

export interface RegisterRequest {
  email: string;
  password: string;
  display_name?: string;
}

export interface RegisterResponse {
  user: User;
}

// Session types
export interface Session {
  id: string;
  user_id: string;
  name: string | null;
  status: 'active' | 'paused' | 'completed';
  created_at: string;
  updated_at: string;
}

export interface CreateSessionRequest {
  name?: string;
}

// WebSocket types
export type WsEventType =
  | 'message'
  | 'streaming_start'
  | 'streaming_chunk'
  | 'streaming_end'
  | 'error';

export interface WsMessage {
  type: WsEventType;
  session_id?: string;
  content?: string;
  delta?: string;
  message?: string;
}

export interface ChatMessage {
  id: string;
  role: 'user' | 'assistant';
  content: string;
  created_at: string;
}

// API Error
export interface ApiError {
  error: string;
}
```

**Step 2: 创建 api/index.ts (fetch 封装)**

```typescript
import { ApiError } from './types';

const API_BASE = '/api';

class ApiClient {
  private token: string | null = null;
  private refreshToken: string | null = null;

  setToken(token: string | null) {
    this.token = token;
    if (token) {
      localStorage.setItem('access_token', token);
    } else {
      localStorage.removeItem('access_token');
    }
  }

  setRefreshToken(token: string | null) {
    this.refreshToken = token;
    if (token) {
      localStorage.setItem('refresh_token', token);
    } else {
      localStorage.removeItem('refresh_token');
    }
  }

  loadFromStorage() {
    this.token = localStorage.getItem('access_token');
    this.refreshToken = localStorage.getItem('refresh_token');
  }

  getToken() {
    return this.token;
  }

  async request<T>(
    endpoint: string,
    options: RequestInit = {}
  ): Promise<T> {
    const headers: HeadersInit = {
      'Content-Type': 'application/json',
      ...options.headers,
    };

    if (this.token) {
      (headers as Record<string, string>)['Authorization'] = `Bearer ${this.token}`;
    }

    const response = await fetch(`${API_BASE}${endpoint}`, {
      ...options,
      headers,
    });

    if (response.status === 401 && this.refreshToken) {
      const refreshed = await this.refreshAccessToken();
      if (refreshed) {
        (headers as Record<string, string>)['Authorization'] = `Bearer ${this.token}`;
        const retryResponse = await fetch(`${API_BASE}${endpoint}`, {
          ...options,
          headers,
        });
        if (!retryResponse.ok) {
          const error: ApiError = await retryResponse.json();
          throw new Error(error.error);
        }
        return retryResponse.json();
      }
    }

    if (!response.ok) {
      const error: ApiError = await response.json();
      throw new Error(error.error);
    }

    return response.json();
  }

  async refreshAccessToken(): Promise<boolean> {
    if (!this.refreshToken) return false;

    try {
      const response = await fetch(`${API_BASE}/auth/refresh`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ refresh_token: this.refreshToken }),
      });

      if (!response.ok) {
        this.clearTokens();
        return false;
      }

      const data = await response.json();
      this.setToken(data.access_token);
      this.setRefreshToken(data.refresh_token);
      return true;
    } catch {
      this.clearTokens();
      return false;
    }
  }

  clearTokens() {
    this.token = null;
    this.refreshToken = null;
    localStorage.removeItem('access_token');
    localStorage.removeItem('refresh_token');
  }
}

export const api = new ApiClient();
```

**Step 3: 创建 api/auth.ts**

```typescript
import { api } from './index';
import { LoginRequest, LoginResponse, RegisterRequest, RegisterResponse } from './types';

export const authApi = {
  async login(req: LoginRequest): Promise<LoginResponse> {
    const data = await api.request<LoginResponse>('/auth/login', {
      method: 'POST',
      body: JSON.stringify(req),
    });
    api.setToken(data.access_token);
    api.setRefreshToken(data.refresh_token);
    return data;
  },

  async register(req: RegisterRequest): Promise<RegisterResponse> {
    return api.request<RegisterResponse>('/auth/register', {
      method: 'POST',
      body: JSON.stringify(req),
    });
  },

  async refresh(refreshToken: string): Promise<LoginResponse> {
    const data = await api.request<LoginResponse>('/auth/refresh', {
      method: 'POST',
      body: JSON.stringify({ refresh_token: refreshToken }),
    });
    api.setToken(data.access_token);
    api.setRefreshToken(data.refresh_token);
    return data;
  },

  logout() {
    api.clearTokens();
  },
};
```

**Step 4: 创建 api/sessions.ts**

```typescript
import { api } from './index';
import { Session, CreateSessionRequest } from './types';

export const sessionsApi = {
  async list(): Promise<Session[]> {
    return api.request<Session[]>('/sessions', { method: 'GET' });
  },

  async create(req?: CreateSessionRequest): Promise<Session> {
    return api.request<Session>('/sessions', {
      method: 'POST',
      body: JSON.stringify(req || {}),
    });
  },

  async get(sessionId: string): Promise<Session> {
    return api.request<Session>(`/sessions/${sessionId}`, { method: 'GET' });
  },

  async delete(sessionId: string): Promise<void> {
    await api.request(`/sessions/${sessionId}`, { method: 'DELETE' });
  },
};
```

**Step 5: 创建 api/users.ts**

```typescript
import { api } from './index';
import { User } from './types';

export const usersApi = {
  async me(): Promise<User> {
    return api.request<User>('/auth/me', { method: 'GET' });
  },

  async list(): Promise<User[]> {
    return api.request<User[]>('/users', { method: 'GET' });
  },

  async get(userId: string): Promise<User> {
    return api.request<User>(`/users/${userId}`, { method: 'GET' });
  },

  async update(userId: string, data: Partial<User>): Promise<User> {
    return api.request<User>(`/users/${userId}`, {
      method: 'PUT',
      body: JSON.stringify(data),
    });
  },

  async updateRole(userId: string, role: string): Promise<User> {
    return api.request<User>(`/users/${userId}/role`, {
      method: 'PATCH',
      body: JSON.stringify({ role }),
    });
  },

  async delete(userId: string): Promise<void> {
    await api.request(`/users/${userId}`, { method: 'DELETE' });
  },
};
```

**Step 6: 验证编译**

Run: `cd web-platform && npx tsc --noEmit`
Expected: 无类型错误

---

## Task 3: 创建 Jotai 状态 atoms

**Files:**
- Create: `web-platform/src/atoms/auth.ts`
- Create: `web-platform/src/atoms/sessions.ts`
- Create: `web-platform/src/atoms/chat.ts`
- Create: `web-platform/src/atoms/ui.ts`
- Create: `web-platform/src/atoms/index.ts`

**Step 1: 创建 atoms/auth.ts**

```typescript
import { atom } from 'jotai';
import { User } from '../api/types';

// User atom
export const userAtom = atom<User | null>(null);

// Token atoms
export const accessTokenAtom = atom<string | null>(null);
export const refreshTokenAtom = atom<string | null>(null);

// Derived: is authenticated
export const isAuthenticatedAtom = atom((get) => !!get(userAtom));
```

**Step 2: 创建 atoms/sessions.ts**

```typescript
import { atom } from 'jotai';
import { Session } from '../api/types';

// Sessions list
export const sessionsAtom = atom<Session[]>([]);

// Current session
export const currentSessionIdAtom = atom<string | null>(null);

// Derived: current session
export const currentSessionAtom = atom((get) => {
  const sessions = get(sessionsAtom);
  const currentId = get(currentSessionIdAtom);
  return sessions.find((s) => s.id === currentId) || null;
});
```

**Step 3: 创建 atoms/chat.ts**

```typescript
import { atom } from 'jotai';
import { ChatMessage } from '../api/types';

// Messages for current session
export const messagesAtom = atom<ChatMessage[]>([]);

// Input text
export const inputAtom = atom<string>('');

// Streaming state
export const isStreamingAtom = atom<boolean>(false);

// Connection state
export const isConnectedAtom = atom<boolean>(false);
```

**Step 4: 创建 atoms/ui.ts**

```typescript
import { atom } from 'jotai';

// Sidebar collapsed
export const sidebarCollapsedAtom = atom<boolean>(false);

// Loading states
export const isLoadingAtom = atom<boolean>(false);

// Error message
export const errorAtom = atom<string | null>(null);
```

**Step 5: 创建 atoms/index.ts**

```typescript
export * from './auth';
export * from './sessions';
export * from './chat';
export * from './ui';
```

---

## Task 4: 创建认证组件

**Files:**
- Create: `web-platform/src/components/auth/LoginForm.tsx`
- Create: `web-platform/src/components/auth/ProtectedRoute.tsx`
- Create: `web-platform/src/pages/Login.tsx`

**Step 1: 创建 LoginForm.tsx**

```tsx
import { useState } from 'react';
import { useSetAtom } from 'jotai';
import { userAtom, accessTokenAtom, refreshTokenAtom } from '../atoms';
import { authApi } from '../api/auth';
import { User } from '../api/types';

interface LoginFormProps {
  onSuccess?: () => void;
}

export function LoginForm({ onSuccess }: LoginFormProps) {
  const [email, setEmail] = useState('');
  const [password, setPassword] = useState('');
  const [error, setError] = useState('');
  const [loading, setLoading] = useState(false);

  const setUser = useSetAtom(userAtom);
  const setAccessToken = useSetAtom(accessTokenAtom);
  const setRefreshToken = useSetAtom(refreshTokenAtom);

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    setError('');
    setLoading(true);

    try {
      const response = await authApi.login({ email, password });
      setUser(response.user);
      setAccessToken(response.access_token);
      setRefreshToken(response.refresh_token);
      onSuccess?.();
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Login failed');
    } finally {
      setLoading(false);
    }
  };

  return (
    <form onSubmit={handleSubmit} className="space-y-4">
      {error && (
        <div className="bg-red-50 text-red-600 p-3 rounded-lg text-sm">
          {error}
        </div>
      )}
      <div>
        <label className="block text-sm font-medium mb-1">Email</label>
        <input
          type="email"
          value={email}
          onChange={(e) => setEmail(e.target.value)}
          className="w-full px-3 py-2 border rounded-lg"
          required
        />
      </div>
      <div>
        <label className="block text-sm font-medium mb-1">Password</label>
        <input
          type="password"
          value={password}
          onChange={(e) => setPassword(e.target.value)}
          className="w-full px-3 py-2 border rounded-lg"
          required
        />
      </div>
      <button
        type="submit"
        disabled={loading}
        className="w-full bg-primary text-white py-2 rounded-lg hover:opacity-90 disabled:opacity-50"
      >
        {loading ? 'Signing in...' : 'Sign in'}
      </button>
    </form>
  );
}
```

**Step 2: 创建 ProtectedRoute.tsx**

```tsx
import { ReactNode } from 'react';
import { Navigate, useLocation } from 'react-router-dom';
import { useAtomValue } from 'jotai';
import { userAtom } from '../atoms';

interface ProtectedRouteProps {
  children: ReactNode;
}

export function ProtectedRoute({ children }: ProtectedRouteProps) {
  const user = useAtomValue(userAtom);
  const location = useLocation();

  if (!user) {
    return <Navigate to="/login" state={{ from: location }} replace />;
  }

  return <>{children}</>;
}
```

**Step 3: 创建 Login.tsx**

```tsx
import { useNavigate } from 'react-router-dom';
import { LoginForm } from '../components/auth/LoginForm';

export function LoginPage() {
  const navigate = useNavigate();

  const handleSuccess = () => {
    navigate('/dashboard');
  };

  return (
    <div className="min-h-screen flex items-center justify-center bg-gray-50">
      <div className="w-full max-w-md p-8 bg-white rounded-xl shadow-sm">
        <h1 className="text-2xl font-bold text-center mb-6">Octo Platform</h1>
        <LoginForm onSuccess={handleSuccess} />
      </div>
    </div>
  );
}
```

---

## Task 5: 创建布局组件

**Files:**
- Create: `web-platform/src/components/layout/AppLayout.tsx`
- Create: `web-platform/src/components/layout/NavRail.tsx`
- Create: `web-platform/src/components/layout/Header.tsx`

**Step 1: 创建 AppLayout.tsx**

```tsx
import { ReactNode } from 'react';
import { NavRail } from './NavRail';
import { Header } from './Header';

interface AppLayoutProps {
  children: ReactNode;
}

export function AppLayout({ children }: AppLayoutProps) {
  return (
    <div className="flex h-screen">
      <NavRail />
      <div className="flex-1 flex flex-col overflow-hidden">
        <Header />
        <main className="flex-1 overflow-auto p-6">
          {children}
        </main>
      </div>
    </div>
  );
}
```

**Step 2: 创建 NavRail.tsx**

```tsx
import { NavLink } from 'react-router-dom';
import { Home, MessageSquare, History, Settings } from 'lucide-react';

const navItems = [
  { to: '/dashboard', icon: Home, label: 'Home' },
  { to: '/chat', icon: MessageSquare, label: 'Chat' },
  { to: '/sessions', icon: History, label: 'History' },
];

export function NavRail() {
  return (
    <nav className="w-16 bg-white border-r flex flex-col py-4">
      <div className="px-3 mb-6">
        <span className="text-xl font-bold">O</span>
      </div>
      <div className="flex-1 space-y-2">
        {navItems.map((item) => (
          <NavLink
            key={item.to}
            to={item.to}
            className={({ isActive }) =>
              `flex flex-col items-center py-2 px-3 text-xs ${
                isActive ? 'text-primary' : 'text-gray-500'
              }`
            }
          >
            <item.icon className="w-5 h-5 mb-1" />
            <span>{item.label}</span>
          </NavLink>
        ))}
      </div>
    </nav>
  );
}
```

**Step 3: 创建 Header.tsx**

```tsx
import { useNavigate } from 'react-router-dom';
import { useAtomValue, useSetAtom } from 'jotai';
import { LogOut, User } from 'lucide-react';
import { userAtom } from '../atoms';
import { authApi } from '../api/auth';

export function Header() {
  const user = useAtomValue(userAtom);
  const setUser = useSetAtom(userAtom);
  const navigate = useNavigate();

  const handleLogout = () => {
    authApi.logout();
    setUser(null);
    navigate('/login');
  };

  return (
    <header className="h-14 bg-white border-b flex items-center justify-between px-4">
      <div className="text-sm text-gray-500">
        Welcome, <span className="font-medium">{user?.display_name || user?.email}</span>
      </div>
      <div className="flex items-center gap-3">
        <button className="p-2 hover:bg-gray-100 rounded-lg">
          <User className="w-5 h-5" />
        </button>
        <button
          onClick={handleLogout}
          className="p-2 hover:bg-gray-100 rounded-lg text-gray-500"
        >
          <LogOut className="w-5 h-5" />
        </button>
      </div>
    </header>
  );
}
```

---

## Task 6: 创建 Dashboard 页面

**Files:**
- Create: `web-platform/src/pages/Dashboard.tsx`
- Create: `web-platform/src/components/dashboard/StatsCard.tsx`
- Create: `web-platform/src/components/dashboard/RecentSessions.tsx`

**Step 1: 创建 StatsCard.tsx**

```tsx
interface StatsCardProps {
  title: string;
  value: number;
  icon: React.ReactNode;
}

export function StatsCard({ title, value, icon }: StatsCardProps) {
  return (
    <div className="bg-white p-4 rounded-xl border">
      <div className="flex items-center justify-between">
        <div>
          <p className="text-sm text-gray-500">{title}</p>
          <p className="text-2xl font-bold mt-1">{value}</p>
        </div>
        <div className="text-primary">{icon}</div>
      </div>
    </div>
  );
}
```

**Step 2: 创建 RecentSessions.tsx**

```tsx
import { useNavigate } from 'react-router-dom';
import { Session } from '../api/types';

interface RecentSessionsProps {
  sessions: Session[];
}

export function RecentSessions({ sessions }: RecentSessionsProps) {
  const navigate = useNavigate();

  if (sessions.length === 0) {
    return (
      <div className="text-center py-8 text-gray-500">
        No sessions yet. Start a new chat!
      </div>
    );
  }

  return (
    <div className="space-y-2">
      {sessions.slice(0, 5).map((session) => (
        <button
          key={session.id}
          onClick={() => navigate(`/chat/${session.id}`)}
          className="w-full text-left p-3 rounded-lg hover:bg-gray-50 border"
        >
          <div className="font-medium">{session.name || 'Untitled'}</div>
          <div className="text-sm text-gray-500">
            {new Date(session.updated_at).toLocaleDateString()}
          </div>
        </button>
      ))}
    </div>
  );
}
```

**Step 3: 创建 Dashboard.tsx**

```tsx
import { useEffect, useState } from 'react';
import { useNavigate } from 'react-router-dom';
import { MessageSquare, Users, Bot } from 'lucide-react';
import { useSetAtom } from 'jotai';
import { sessionsAtom } from '../atoms';
import { sessionsApi } from '../api/sessions';
import { StatsCard } from '../components/dashboard/StatsCard';
import { RecentSessions } from '../components/dashboard/RecentSessions';

export function DashboardPage() {
  const [loading, setLoading] = useState(true);
  const setSessions = useSetAtom(sessionsAtom);
  const navigate = useNavigate();

  useEffect(() => {
    sessionsApi.list()
      .then(setSessions)
      .catch(console.error)
      .finally(() => setLoading(false));
  }, [setSessions]);

  const handleNewChat = async () => {
    try {
      const session = await sessionsApi.create();
      navigate(`/chat/${session.id}`);
    } catch (err) {
      console.error(err);
    }
  };

  if (loading) {
    return <div>Loading...</div>;
  }

  return (
    <div className="max-w-4xl mx-auto">
      <h1 className="text-2xl font-bold mb-6">Dashboard</h1>

      <div className="grid grid-cols-3 gap-4 mb-8">
        <StatsCard title="Sessions" value={0} icon={<MessageSquare className="w-6 h-6" />} />
        <StatsCard title="Messages" value={0} icon={<Users className="w-6 h-6" />} />
        <StatsCard title="Agents" value={0} icon={<Bot className="w-6 h-6" />} />
      </div>

      <div className="mb-6">
        <h2 className="text-lg font-semibold mb-3">Recent Sessions</h2>
        <RecentSessions sessions={[]} />
      </div>

      <button
        onClick={handleNewChat}
        className="w-full bg-primary text-white py-3 rounded-lg hover:opacity-90"
      >
        + New Chat
      </button>
    </div>
  );
}
```

---

## Task 7: 创建 WebSocket 管理器

**Files:**
- Create: `web-platform/src/ws/manager.ts`
- Create: `web-platform/src/ws/types.ts`

**Step 1: 创建 ws/manager.ts**

```typescript
import { WsMessage } from './types';

type MessageHandler = (message: WsMessage) => void;
type ConnectionHandler = () => void;

class WebSocketManager {
  private ws: WebSocket | null = null;
  private sessionId: string | null = null;
  private token: string | null = null;
  private messageHandlers: MessageHandler[] = [];
  private connectHandlers: ConnectionHandler[] = [];
  private disconnectHandlers: ConnectionHandler[] = [];
  private reconnectAttempts = 0;
  private maxReconnectAttempts = 5;
  private reconnectTimer: number | null = null;
  private heartbeatTimer: number | null = null;

  connect(sessionId: string, token: string) {
    this.sessionId = sessionId;
    this.token = token;
    this.reconnectAttempts = 0;
    this.doConnect();
  }

  private doConnect() {
    if (!this.sessionId || !this.token) return;

    const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
    const wsUrl = `${protocol}//${window.location.host}/ws/${this.sessionId}`;

    this.ws = new WebSocket(wsUrl);

    this.ws.onopen = () => {
      this.reconnectAttempts = 0;
      this.startHeartbeat();
      this.connectHandlers.forEach((h) => h());
    };

    this.ws.onmessage = (event) => {
      try {
        const message: WsMessage = JSON.parse(event.data);
        this.messageHandlers.forEach((h) => h(message));
      } catch (err) {
        console.error('Failed to parse WS message:', err);
      }
    };

    this.ws.onclose = () => {
      this.stopHeartbeat();
      this.disconnectHandlers.forEach((h) => h());
      this.attemptReconnect();
    };

    this.ws.onerror = (err) => {
      console.error('WebSocket error:', err);
    };
  }

  private attemptReconnect() {
    if (this.reconnectAttempts >= this.maxReconnectAttempts) {
      console.log('Max reconnection attempts reached');
      return;
    }

    const delay = Math.pow(2, this.reconnectAttempts) * 1000;
    this.reconnectAttempts++;

    this.reconnectTimer = window.setTimeout(() => {
      this.doConnect();
    }, delay);
  }

  private startHeartbeat() {
    this.heartbeatTimer = window.setInterval(() => {
      if (this.ws?.readyState === WebSocket.OPEN) {
        this.ws.send(JSON.stringify({ type: 'ping' }));
      }
    }, 30000);
  }

  private stopHeartbeat() {
    if (this.heartbeatTimer) {
      clearInterval(this.heartbeatTimer);
      this.heartbeatTimer = null;
    }
  }

  disconnect() {
    if (this.reconnectTimer) {
      clearTimeout(this.reconnectTimer);
    }
    this.stopHeartbeat();
    this.ws?.close();
    this.ws = null;
    this.sessionId = null;
    this.token = null;
  }

  send(content: string) {
    if (this.ws?.readyState === WebSocket.OPEN) {
      this.ws.send(JSON.stringify({ type: 'message', content }));
    }
  }

  onMessage(handler: MessageHandler) {
    this.messageHandlers.push(handler);
  }

  onConnect(handler: ConnectionHandler) {
    this.connectHandlers.push(handler);
  }

  onDisconnect(handler: ConnectionHandler) {
    this.disconnectHandlers.push(handler);
  }

  isConnected() {
    return this.ws?.readyState === WebSocket.OPEN;
  }
}

export const wsManager = new WebSocketManager();
```

**Step 2: 创建 ws/types.ts**

```typescript
export * from '../api/types';
```

---

## Task 8: 创建 Chat 页面

**Files:**
- Create: `web-platform/src/pages/Chat.tsx`
- Create: `web-platform/src/components/chat/MessageList.tsx`
- Create: `web-platform/src/components/chat/MessageBubble.tsx`
- Create: `web-platform/src/components/chat/ChatInput.tsx`

**Step 1: 创建 MessageBubble.tsx**

```tsx
import { ChatMessage } from '../api/types';

interface MessageBubbleProps {
  message: ChatMessage;
}

export function MessageBubble({ message }: MessageBubbleProps) {
  const isUser = message.role === 'user';

  return (
    <div className={`flex ${isUser ? 'justify-end' : 'justify-start'}`}>
      <div
        className={`max-w-[70%] p-3 rounded-lg ${
          isUser
            ? 'bg-primary text-white'
            : 'bg-gray-100 text-gray-900'
        }`}
      >
        <div className="whitespace-pre-wrap">{message.content}</div>
      </div>
    </div>
  );
}
```

**Step 2: 创建 MessageList.tsx**

```tsx
import { ChatMessage } from '../api/types';
import { MessageBubble } from './MessageBubble';

interface MessageListProps {
  messages: ChatMessage[];
}

export function MessageList({ messages }: MessageListProps) {
  return (
    <div className="space-y-3">
      {messages.map((message) => (
        <MessageBubble key={message.id} message={message} />
      ))}
    </div>
  );
}
```

**Step 3: 创建 ChatInput.tsx**

```tsx
import { useState, useRef, useEffect } from 'react';
import { Send, Square } from 'lucide-react';

interface ChatInputProps {
  value: string;
  onChange: (value: string) => void;
  onSend: () => void;
  onStop?: () => void;
  disabled?: boolean;
  isStreaming?: boolean;
}

export function ChatInput({
  value,
  onChange,
  onSend,
  onStop,
  disabled,
  isStreaming,
}: ChatInputProps) {
  const textareaRef = useRef<HTMLTextAreaElement>(null);

  useEffect(() => {
    if (textareaRef.current) {
      textareaRef.current.style.height = 'auto';
      textareaRef.current.style.height = `${Math.min(textareaRef.current.scrollHeight, 150)}px`;
    }
  }, [value]);

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      onSend();
    }
  };

  return (
    <div className="flex items-end gap-2 p-4 bg-white border-t">
      <textarea
        ref={textareaRef}
        value={value}
        onChange={(e) => onChange(e.target.value)}
        onKeyDown={handleKeyDown}
        placeholder="Type a message..."
        className="flex-1 resize-none border rounded-lg p-3 max-h-[150px]"
        disabled={disabled}
        rows={1}
      />
      {isStreaming ? (
        <button
          onClick={onStop}
          className="p-3 bg-red-500 text-white rounded-lg hover:bg-red-600"
        >
          <Square className="w-5 h-5" />
        </button>
      ) : (
        <button
          onClick={onSend}
          disabled={disabled || !value.trim()}
          className="p-3 bg-primary text-white rounded-lg hover:opacity-90 disabled:opacity-50"
        >
          <Send className="w-5 h-5" />
        </button>
      )}
    </div>
  );
}
```

**Step 4: 创建 Chat.tsx**

```tsx
import { useEffect, useCallback } from 'react';
import { useParams, useNavigate } from 'react-router-dom';
import { useAtom, useSetAtom } from 'jotai';
import { ArrowLeft } from 'lucide-react';
import { messagesAtom, inputAtom, isStreamingAtom, isConnectedAtom, sessionsAtom } from '../atoms';
import { sessionsApi } from '../api/sessions';
import { wsManager } from '../ws/manager';
import { ChatMessage, WsMessage } from '../api/types';
import { MessageList } from '../components/chat/MessageList';
import { ChatInput } from '../components/chat/ChatInput';

export function ChatPage() {
  const { sessionId } = useParams<{ sessionId?: string }>();
  const navigate = useNavigate();
  const [messages, setMessages] = useAtom(messagesAtom);
  const [input, setInput] = useAtom(inputAtom);
  const [isStreaming, setIsStreaming] = useAtom(isStreamingAtom);
  const [isConnected, setIsConnected] = useAtom(isConnectedAtom);
  const setSessions = useSetAtom(sessionsAtom);

  // Connect to WebSocket
  useEffect(() => {
    if (!sessionId) return;

    const token = localStorage.getItem('access_token');
    if (!token) {
      navigate('/login');
      return;
    }

    wsManager.connect(sessionId, token);

    wsManager.onConnect(() => setIsConnected(true));
    wsManager.onDisconnect(() => setIsConnected(false));

    const handleMessage = (msg: WsMessage) => {
      switch (msg.type) {
        case 'message':
          setMessages((prev) => [
            ...prev,
            {
              id: Date.now().toString(),
              role: 'assistant',
              content: msg.content || '',
              created_at: new Date().toISOString(),
            },
          ]);
          break;
        case 'streaming_start':
          setIsStreaming(true);
          setMessages((prev) => [
            ...prev,
            {
              id: 'streaming',
              role: 'assistant',
              content: '',
              created_at: new Date().toISOString(),
            },
          ]);
          break;
        case 'streaming_chunk':
          setMessages((prev) =>
            prev.map((m) =>
              m.id === 'streaming'
                ? { ...m, content: m.content + (msg.delta || '') }
                : m
            )
          );
          break;
        case 'streaming_end':
          setIsStreaming(false);
          setMessages((prev) =>
            prev.map((m) =>
              m.id === 'streaming'
                ? { ...m, id: Date.now().toString() }
                : m
            )
          );
          break;
        case 'error':
          setIsStreaming(false);
          console.error('WS Error:', msg.message);
          break;
      }
    };

    wsManager.onMessage(handleMessage);

    return () => {
      wsManager.disconnect();
    };
  }, [sessionId, navigate, setIsConnected, setIsStreaming, setMessages]);

  const handleSend = useCallback(() => {
    if (!input.trim() || !sessionId) return;

    const userMessage: ChatMessage = {
      id: Date.now().toString(),
      role: 'user',
      content: input,
      created_at: new Date().toISOString(),
    };

    setMessages((prev) => [...prev, userMessage]);
    wsManager.send(input);
    setInput('');
  }, [input, sessionId, setMessages, setInput]);

  const handleStop = useCallback(() => {
    setIsStreaming(false);
  }, [setIsStreaming]);

  const handleNewChat = async () => {
    try {
      const session = await sessionsApi.create();
      setSessions((prev) => [session, ...prev]);
      navigate(`/chat/${session.id}`);
    } catch (err) {
      console.error(err);
    }
  };

  return (
    <div className="flex flex-col h-full">
      <div className="flex items-center gap-3 p-4 border-b bg-white">
        <button
          onClick={() => navigate('/dashboard')}
          className="p-2 hover:bg-gray-100 rounded-lg"
        >
          <ArrowLeft className="w-5 h-5" />
        </button>
        <div className="flex-1">
          <h1 className="font-semibold">Chat</h1>
          <div className="text-xs text-gray-500">
            {isConnected ? 'Connected' : 'Disconnected'}
          </div>
        </div>
      </div>

      <div className="flex-1 overflow-auto p-4">
        {messages.length === 0 ? (
          <div className="text-center text-gray-500 mt-8">
            <p>No messages yet.</p>
            <button
              onClick={handleNewChat}
              className="text-primary hover:underline mt-2"
            >
              Start a new conversation
            </button>
          </div>
        ) : (
          <MessageList messages={messages} />
        )}
      </div>

      <ChatInput
        value={input}
        onChange={setInput}
        onSend={handleSend}
        onStop={handleStop}
        disabled={!isConnected}
        isStreaming={isStreaming}
      />
    </div>
  );
}
```

---

## Task 9: 创建 Sessions 页面

**Files:**
- Create: `web-platform/src/pages/Sessions.tsx`

**Step 1: 创建 Sessions.tsx**

```tsx
import { useEffect, useState } from 'react';
import { useNavigate } from 'react-router-dom';
import { useSetAtom } from 'jotai';
import { Plus, Trash2 } from 'lucide-react';
import { sessionsAtom, currentSessionIdAtom } from '../atoms';
import { sessionsApi } from '../api/sessions';
import { Session } from '../api/types';

export function SessionsPage() {
  const [sessions, setSessions] = useState<Session[]>([]);
  const [loading, setLoading] = useState(true);
  const setSessionsAtom = useSetAtom(sessionsAtom);
  const setCurrentSessionId = useSetAtom(currentSessionIdAtom);
  const navigate = useNavigate();

  useEffect(() => {
    sessionsApi.list()
      .then((data) => {
        setSessions(data);
        setSessionsAtom(data);
      })
      .catch(console.error)
      .finally(() => setLoading(false));
  }, [setSessionsAtom]);

  const handleCreate = async () => {
    try {
      const session = await sessionsApi.create();
      setSessions((prev) => [session, ...prev]);
      setCurrentSessionId(session.id);
      navigate(`/chat/${session.id}`);
    } catch (err) {
      console.error(err);
    }
  };

  const handleDelete = async (e: React.MouseEvent, sessionId: string) => {
    e.stopPropagation();
    if (!confirm('Delete this session?')) return;

    try {
      await sessionsApi.delete(sessionId);
      setSessions((prev) => prev.filter((s) => s.id !== sessionId));
    } catch (err) {
      console.error(err);
    }
  };

  if (loading) {
    return <div>Loading...</div>;
  }

  return (
    <div className="max-w-2xl mx-auto">
      <div className="flex items-center justify-between mb-6">
        <h1 className="text-2xl font-bold">Sessions</h1>
        <button
          onClick={handleCreate}
          className="flex items-center gap-2 bg-primary text-white px-4 py-2 rounded-lg"
        >
          <Plus className="w-5 h-5" />
          New
        </button>
      </div>

      {sessions.length === 0 ? (
        <div className="text-center py-8 text-gray-500">
          No sessions yet
        </div>
      ) : (
        <div className="space-y-2">
          {sessions.map((session) => (
            <button
              key={session.id}
              onClick={() => {
                setCurrentSessionId(session.id);
                navigate(`/chat/${session.id}`);
              }}
              className="w-full text-left p-4 rounded-lg border hover:bg-gray-50 flex items-center justify-between"
            >
              <div>
                <div className="font-medium">
                  {session.name || 'Untitled'}
                </div>
                <div className="text-sm text-gray-500">
                  {new Date(session.updated_at).toLocaleString()}
                </div>
              </div>
              <button
                onClick={(e) => handleDelete(e, session.id)}
                className="p-2 text-gray-400 hover:text-red-500"
              >
                <Trash2 className="w-5 h-5" />
              </button>
            </button>
          ))}
        </div>
      )}
    </div>
  );
}
```

---

## Task 10: 更新 App.tsx 路由

**Files:**
- Modify: `web-platform/src/App.tsx`

**Step 1: 更新 App.tsx**

```tsx
import { useEffect } from 'react';
import { BrowserRouter, Routes, Route, Navigate } from 'react-router-dom';
import { useAtom } from 'jotai';
import { userAtom, accessTokenAtom } from './atoms';
import { authApi } from './api/auth';
import { LoginPage } from './pages/Login';
import { DashboardPage } from './pages/Dashboard';
import { ChatPage } from './pages/Chat';
import { SessionsPage } from './pages/Sessions';
import { AppLayout } from './components/layout/AppLayout';
import { ProtectedRoute } from './components/auth/ProtectedRoute';

function AuthInitializer({ children }: { children: React.ReactNode }) {
  const [, setUser] = useAtom(userAtom);
  const [, setToken] = useAtom(accessTokenAtom);

  useEffect(() => {
    const token = localStorage.getItem('access_token');
    if (token) {
      setToken(token);
      authApi.refresh(localStorage.getItem('refresh_token') || '')
        .then((data) => {
          setUser(data.user);
        })
        .catch(() => {
          localStorage.removeItem('access_token');
          localStorage.removeItem('refresh_token');
        });
    }
  }, [setUser, setToken]);

  return <>{children}</>;
}

function App() {
  return (
    <BrowserRouter>
      <AuthInitializer>
        <Routes>
          <Route path="/login" element={<LoginPage />} />
          <Route
            path="/"
            element={
              <ProtectedRoute>
                <AppLayout />
              </ProtectedRoute>
            }
          >
            <Route index element={<Navigate to="/dashboard" replace />} />
            <Route path="dashboard" element={<DashboardPage />} />
            <Route path="chat" element={<ChatPage />} />
            <Route path="chat/:sessionId" element={<ChatPage />} />
            <Route path="sessions" element={<SessionsPage />} />
          </Route>
        </Routes>
      </AuthInitializer>
    </BrowserRouter>
  );
}

export default App;
```

**Step 2: 验证构建**

Run: `cd web-platform && npm run build`
Expected: 构建成功

---

## Task 11: 集成测试

**Step 1: 验证开发服务器启动**

Run: `cd web-platform && npm run dev`
Expected: Vite 开发服务器启动在 http://localhost:5180

**Step 2: 验证路由**

1. 访问 /login - 应显示登录表单
2. 登录后访问 /dashboard - 应显示 Dashboard
3. 访问 /chat - 应显示聊天页面
4. 访问 /sessions - 应显示会话列表

---

## 实施计划完成

**Plan complete and saved to `docs/plans/2026-03-04-p1-6-web-platform-implementation.md`.**

### 两个执行选项：

**1. Subagent-Driven (本会话)** - 我为每个任务分配新的子代理，任务之间进行审查，快速迭代

**2. Parallel Session (独立会话)** - 在新会话中使用 executing-plans，批量执行带检查点

你选择哪种方式？