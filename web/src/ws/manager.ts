import type { ClientMessage, ServerMessage } from "./types";
import { isConfigReady, getWsUrl } from "../config";

type MessageHandler = (msg: ServerMessage) => void;
type DisconnectHandler = () => void;

class WsManager {
  private ws: WebSocket | null = null;
  private url: string = '';
  private handler: MessageHandler | null = null;
  private disconnectHandler: DisconnectHandler | null = null;
  private reconnectAttempts = 0;
  private maxReconnectAttempts = 5;
  private reconnectTimer: ReturnType<typeof setTimeout> | null = null;
  private intentionalDisconnect = false;
  private currentSessionId: string | null = null;

  constructor() {}

  /**
   * Get WebSocket URL from config or fallback.
   * Appends ?session_id=xxx when a session is active.
   */
  private getUrl(sessionId?: string | null): string {
    let base: string;
    if (isConfigReady()) {
      try {
        base = getWsUrl() + '/ws';
      } catch {
        const proto = window.location.protocol === "https:" ? "wss:" : "ws:";
        base = `${proto}//${window.location.host}/ws`;
      }
    } else {
      const proto = window.location.protocol === "https:" ? "wss:" : "ws:";
      base = `${proto}//${window.location.host}/ws`;
    }
    const sid = sessionId ?? this.currentSessionId;
    if (sid) {
      base += `?session_id=${encodeURIComponent(sid)}`;
    }
    return base;
  }

  connect(sessionId?: string | null) {
    if (sessionId !== undefined) {
      this.currentSessionId = sessionId ?? null;
    }
    // Get URL on each connect to support dynamic config
    this.url = this.getUrl();

    // Already connected or connecting - don't create another connection
    if (this.ws && (this.ws.readyState === WebSocket.OPEN || this.ws.readyState === WebSocket.CONNECTING)) {
      return;
    }

    // Close any existing connection before creating a new one
    if (this.ws) {
      this.ws.close();
    }

    // Reset intentional disconnect flag on new connect attempt
    this.intentionalDisconnect = false;

    this.ws = new WebSocket(this.url);

    this.ws.onopen = () => {
      console.log("[WS] Connected", this.currentSessionId ? `(session: ${this.currentSessionId})` : "");
      this.reconnectAttempts = 0;
    };

    this.ws.onmessage = (event) => {
      try {
        const msg = JSON.parse(event.data) as ServerMessage;
        this.handler?.(msg);
      } catch (e) {
        console.error("[WS] Parse error:", e);
      }
    };

    this.ws.onclose = () => {
      console.log("[WS] Disconnected");
      if (!this.intentionalDisconnect) {
        this.disconnectHandler?.();
      }
      this.scheduleReconnect();
    };

    this.ws.onerror = (err) => {
      console.error("[WS] Error:", err);
    };
  }

  /**
   * Switch to a different session. Disconnects the current WS and
   * reconnects with the new session_id query parameter.
   */
  switchSession(sessionId: string) {
    console.log(`[WS] Switching to session ${sessionId}`);
    this.currentSessionId = sessionId;

    // Tear down the current connection without triggering auto-reconnect
    if (this.reconnectTimer) {
      clearTimeout(this.reconnectTimer);
      this.reconnectTimer = null;
    }
    this.intentionalDisconnect = true;
    this.ws?.close();
    this.ws = null;

    // Reset reconnect state and connect fresh
    this.reconnectAttempts = 0;
    this.intentionalDisconnect = false;
    this.connect(sessionId);
  }

  disconnect() {
    if (this.reconnectTimer) {
      clearTimeout(this.reconnectTimer);
      this.reconnectTimer = null;
    }
    // Mark as intentional disconnect to prevent reconnection
    this.intentionalDisconnect = true;
    this.ws?.close();
    this.ws = null;
  }

  send(msg: ClientMessage) {
    if (this.ws?.readyState !== WebSocket.OPEN) {
      console.warn("[WS] Not connected, cannot send");
      return;
    }
    this.ws.send(JSON.stringify(msg));
  }

  onMessage(handler: MessageHandler) {
    this.handler = handler;
  }

  onDisconnect(handler: DisconnectHandler) {
    this.disconnectHandler = handler;
  }

  get connected(): boolean {
    return this.ws?.readyState === WebSocket.OPEN;
  }

  private scheduleReconnect() {
    // Don't reconnect if this was an intentional disconnect
    if (this.intentionalDisconnect) return;

    if (this.reconnectAttempts >= this.maxReconnectAttempts) return;

    const delay = Math.min(1000 * Math.pow(2, this.reconnectAttempts), 30000);
    this.reconnectAttempts++;

    console.log(`[WS] Reconnecting in ${delay}ms (attempt ${this.reconnectAttempts})`);
    this.reconnectTimer = setTimeout(() => this.connect(), delay);
  }
}

export const wsManager = new WsManager();
