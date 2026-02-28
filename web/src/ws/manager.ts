import type { ClientMessage, ServerMessage } from "./types";
import { isConfigReady, getWsUrl } from "../config";

type MessageHandler = (msg: ServerMessage) => void;

class WsManager {
  private ws: WebSocket | null = null;
  private url: string = '';
  private handler: MessageHandler | null = null;
  private reconnectAttempts = 0;
  private maxReconnectAttempts = 5;
  private reconnectTimer: ReturnType<typeof setTimeout> | null = null;
  private intentionalDisconnect = false;

  constructor() {}

  /**
   * Get WebSocket URL from config or fallback
   */
  private getUrl(): string {
    if (isConfigReady()) {
      try {
        return getWsUrl() + '/ws';
      } catch {
        // Config not ready, use fallback
      }
    }
    // Fallback to window.location if config not ready
    const proto = window.location.protocol === "https:" ? "wss:" : "ws:";
    return `${proto}//${window.location.host}/ws`;
  }

  connect() {
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
      console.log("[WS] Connected");
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
      this.scheduleReconnect();
    };

    this.ws.onerror = (err) => {
      console.error("[WS] Error:", err);
    };
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
