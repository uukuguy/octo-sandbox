import { atom } from "jotai";

export interface ToolExecutionRecord {
  id: string;
  session_id: string;
  tool_name: string;
  source: string;
  input: unknown;
  output: unknown | null;
  status: "running" | "success" | "failed" | "timeout";
  started_at: number;
  duration_ms: number | null;
  error: string | null;
}

export interface TokenBudget {
  total: number;
  system_prompt: number;
  dynamic_context: number;
  history: number;
  free: number;
  usage_percent: number;
  degradation_level: number;
}

export const executionRecordsAtom = atom<ToolExecutionRecord[]>([]);
export const tokenBudgetAtom = atom<TokenBudget | null>(null);
export const selectedExecutionIdAtom = atom<string | null>(null);
