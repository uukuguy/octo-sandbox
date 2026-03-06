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
