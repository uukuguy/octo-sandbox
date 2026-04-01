import type { Getter, Setter } from "jotai";
import type { ServerMessage } from "./types";
import {
  sessionIdAtom,
  messagesAtom,
  isStreamingAtom,
  streamingTextAtom,
  streamingThinkingAtom,
  toolExecutionsAtom,
  sessionsAtom,
  migrateFallbackMessagesAtom,
} from "../atoms/session";
import {
  executionRecordsAtom,
  tokenBudgetAtom,
  pushLiveEventAtom,
  contextStatusAtom,
} from "../atoms/debug";
import { addToastAtom } from "../atoms/ui";

let streamBuffer = "";
let thinkingBuffer = "";

export function handleWsEvent(msg: ServerMessage, set: Setter, get?: Getter) {
  switch (msg.type) {
    case "session_created": {
      set(sessionIdAtom, msg.session_id);
      // Register the session in the multi-session list if not already present
      if (get) {
        const existing = get(sessionsAtom);
        if (!existing.some((s) => s.id === msg.session_id)) {
          set(sessionsAtom, [
            ...existing,
            { id: msg.session_id, createdAt: new Date().toISOString() },
          ]);
        }
      }
      // Migrate any pre-session messages into this session's slot
      set(migrateFallbackMessagesAtom);
      break;
    }

    case "text_delta":
      streamBuffer += msg.text;
      set(streamingTextAtom, streamBuffer);
      break;

    case "text_complete": {
      // Some thinking models start the reply mid-sentence (e.g. ",以下是...")
      // Trim leading punctuation for cleaner display
      const text = msg.text.replace(/^[，,、；;：:。.！!？?\s]+/, "");
      set(messagesAtom, (prev) => [
        ...prev,
        {
          id: crypto.randomUUID(),
          role: "assistant" as const,
          content: text,
          thinking: thinkingBuffer || undefined,
          timestamp: Date.now(),
        },
      ]);
      streamBuffer = "";
      thinkingBuffer = "";
      set(streamingTextAtom, "");
      set(streamingThinkingAtom, "");
      break;
    }

    case "thinking_delta":
      thinkingBuffer += msg.text;
      set(streamingThinkingAtom, thinkingBuffer);
      break;

    case "thinking_complete":
      // Keep thinkingBuffer — it will be saved into the message on text_complete
      break;

    case "tool_start":
      set(toolExecutionsAtom, (prev) => [
        ...prev,
        {
          toolId: msg.tool_id,
          toolName: msg.tool_name,
          input: msg.input,
          status: "running" as const,
        },
      ]);
      set(pushLiveEventAtom, {
        id: crypto.randomUUID(),
        timestamp: Date.now(),
        type: "tool_start",
        summary: `Tool started: ${msg.tool_name}`,
        data: { tool_id: msg.tool_id, input: msg.input },
      });
      break;

    case "tool_result":
      set(toolExecutionsAtom, (prev) =>
        prev.map((t) =>
          t.toolId === msg.tool_id
            ? { ...t, output: msg.output, success: msg.success, status: "complete" as const }
            : t,
        ),
      );
      set(pushLiveEventAtom, {
        id: crypto.randomUUID(),
        timestamp: Date.now(),
        type: msg.success ? "tool_result" : "tool_error",
        summary: `Tool ${msg.success ? "completed" : "failed"}: ${msg.tool_id}`,
        data: { tool_id: msg.tool_id, success: msg.success },
      });
      break;

    case "error":
      set(messagesAtom, (prev) => [
        ...prev,
        {
          id: crypto.randomUUID(),
          role: "assistant" as const,
          content: `Error: ${msg.message}`,
          timestamp: Date.now(),
        },
      ]);
      streamBuffer = "";
      thinkingBuffer = "";
      set(streamingTextAtom, "");
      set(streamingThinkingAtom, "");
      set(isStreamingAtom, false);
      set(pushLiveEventAtom, {
        id: crypto.randomUUID(),
        timestamp: Date.now(),
        type: "error",
        summary: `Error: ${msg.message}`,
      });
      set(addToastAtom, {
        type: "error",
        title: "Server Error",
        message: msg.message,
      });
      break;

    case "done":
      streamBuffer = "";
      thinkingBuffer = "";
      set(isStreamingAtom, false);
      set(streamingThinkingAtom, "");
      // Do NOT clear executionRecordsAtom here — Tools tab shows accumulated history
      set(toolExecutionsAtom, []);
      break;

    case "tool_execution":
      set(executionRecordsAtom, (prev) => {
        const idx = prev.findIndex((e) => e.id === msg.execution.id);
        if (idx >= 0) {
          const next = [...prev];
          next[idx] = msg.execution;
          return next;
        }
        return [...prev, msg.execution];
      });
      break;

    case "token_budget_update":
      set(tokenBudgetAtom, msg.budget);
      set(pushLiveEventAtom, {
        id: crypto.randomUUID(),
        timestamp: Date.now(),
        type: "token_budget_update",
        summary: `Budget ${msg.budget.usage_percent.toFixed(0)}% used (L${msg.budget.degradation_level})`,
        data: msg.budget,
      });
      break;

    case "context_degraded":
      set(contextStatusAtom, { level: msg.level, usage_pct: msg.usage_pct });
      set(pushLiveEventAtom, {
        id: crypto.randomUUID(),
        timestamp: Date.now(),
        type: "context_degraded",
        summary: `Context degraded to ${msg.level} (${msg.usage_pct.toFixed(0)}%)`,
      });
      break;

    case "memory_flushed":
      set(pushLiveEventAtom, {
        id: crypto.randomUUID(),
        timestamp: Date.now(),
        type: "memory_flushed",
        summary: `Memory flushed: ${msg.facts_count} facts extracted`,
      });
      break;

    case "approval_required":
      set(pushLiveEventAtom, {
        id: crypto.randomUUID(),
        timestamp: Date.now(),
        type: "approval_required",
        summary: `Approval required: ${msg.tool_name} (${msg.risk_level})`,
        data: { tool_id: msg.tool_id, tool_name: msg.tool_name, risk_level: msg.risk_level },
      });
      break;

    case "security_blocked":
      set(pushLiveEventAtom, {
        id: crypto.randomUUID(),
        timestamp: Date.now(),
        type: "security_blocked",
        summary: `Security blocked: ${msg.reason}`,
      });
      break;

    case "typing":
      // Typing indicator — no live event needed, just state tracking
      break;
  }
}
