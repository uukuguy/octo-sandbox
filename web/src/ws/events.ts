import type { Setter } from "jotai";
import type { ServerMessage } from "./types";
import {
  sessionIdAtom,
  messagesAtom,
  isStreamingAtom,
  streamingTextAtom,
  streamingThinkingAtom,
  toolExecutionsAtom,
} from "../atoms/session";
import {
  executionRecordsAtom,
  tokenBudgetAtom,
} from "../atoms/debug";

let streamBuffer = "";
let thinkingBuffer = "";

export function handleWsEvent(msg: ServerMessage, set: Setter) {
  switch (msg.type) {
    case "session_created":
      set(sessionIdAtom, msg.session_id);
      break;

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
      break;

    case "tool_result":
      set(toolExecutionsAtom, (prev) =>
        prev.map((t) =>
          t.toolId === msg.tool_id
            ? { ...t, output: msg.output, success: msg.success, status: "complete" as const }
            : t,
        ),
      );
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
      break;

    case "done":
      streamBuffer = "";
      thinkingBuffer = "";
      set(isStreamingAtom, false);
      set(streamingThinkingAtom, "");
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
      break;
  }
}
