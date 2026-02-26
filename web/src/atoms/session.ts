import { atom } from "jotai";

export interface ChatMsg {
  id: string;
  role: "user" | "assistant";
  content: string;
  thinking?: string;
  timestamp: number;
}

export interface ToolExecution {
  toolId: string;
  toolName: string;
  input: Record<string, unknown>;
  output?: string;
  success?: boolean;
  status: "running" | "complete";
}

export const sessionIdAtom = atom<string | null>(null);
export const messagesAtom = atom<ChatMsg[]>([]);
export const isStreamingAtom = atom(false);
export const streamingTextAtom = atom("");
export const streamingThinkingAtom = atom("");
export const toolExecutionsAtom = atom<ToolExecution[]>([]);
