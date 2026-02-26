import { useState, useCallback } from "react";
import { useAtom, useAtomValue, useSetAtom } from "jotai";
import { Send } from "lucide-react";
import {
  sessionIdAtom,
  messagesAtom,
  isStreamingAtom,
} from "@/atoms/session";
import { wsManager } from "@/ws/manager";

export function ChatInput() {
  const [input, setInput] = useState("");
  const [sessionId] = useAtom(sessionIdAtom);
  const isStreaming = useAtomValue(isStreamingAtom);
  const setMessages = useSetAtom(messagesAtom);
  const setIsStreaming = useSetAtom(isStreamingAtom);

  const handleSend = useCallback(() => {
    const text = input.trim();
    if (!text || isStreaming) return;

    // Add user message locally
    setMessages((prev) => [
      ...prev,
      {
        id: crypto.randomUUID(),
        role: "user" as const,
        content: text,
        timestamp: Date.now(),
      },
    ]);

    setIsStreaming(true);

    // Send via WebSocket
    wsManager.send({
      type: "send_message",
      session_id: sessionId ?? undefined,
      content: text,
    });

    setInput("");
  }, [input, isStreaming, sessionId, setMessages, setIsStreaming]);

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      handleSend();
    }
  };

  return (
    <div className="border-t border-border p-4">
      <div className="flex items-end gap-2">
        <textarea
          value={input}
          onChange={(e) => setInput(e.target.value)}
          onKeyDown={handleKeyDown}
          placeholder="Type a message..."
          rows={1}
          className="flex-1 resize-none rounded-lg border border-input bg-background px-3 py-2 text-sm text-foreground placeholder:text-muted-foreground focus:outline-none focus:ring-1 focus:ring-ring"
          disabled={isStreaming}
        />
        <button
          onClick={handleSend}
          disabled={!input.trim() || isStreaming}
          className="flex h-9 w-9 items-center justify-center rounded-lg bg-primary text-primary-foreground disabled:opacity-50"
        >
          <Send className="h-4 w-4" />
        </button>
      </div>
    </div>
  );
}
