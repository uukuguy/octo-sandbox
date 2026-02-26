import { useAtom } from "jotai";
import { tokenBudgetAtom } from "@/atoms/debug";

const LEVEL_LABELS = ["L0 None", "L1 Soft Trim", "L2 Hard Clear", "L3 Compact"];

function usageColor(pct: number): string {
  if (pct < 60) return "bg-green-500";
  if (pct < 80) return "bg-yellow-500";
  if (pct < 90) return "bg-orange-500";
  return "bg-red-500";
}

function formatTokens(n: number): string {
  if (n >= 1000) return `${(n / 1000).toFixed(1)}K`;
  return `${n}`;
}

export function TokenBudgetBar() {
  const [budget] = useAtom(tokenBudgetAtom);

  if (!budget) {
    return (
      <div className="flex flex-1 items-center justify-center text-muted-foreground text-sm">
        No token budget data yet. Start a conversation to see context usage.
      </div>
    );
  }

  const total = budget.total || 1;
  const sysPct = (budget.system_prompt / total) * 100;
  const dynPct = (budget.dynamic_context / total) * 100;
  const histPct = (budget.history / total) * 100;

  return (
    <div className="p-4 space-y-4">
      <div className="flex items-center justify-between">
        <h3 className="text-sm font-medium">
          Context Window Usage ({budget.usage_percent.toFixed(0)}%)
        </h3>
        <span className="text-xs font-mono text-muted-foreground">
          {LEVEL_LABELS[budget.degradation_level] ?? `L${budget.degradation_level}`}
        </span>
      </div>

      <div className="h-6 flex rounded overflow-hidden bg-secondary/30">
        {sysPct > 0 && (
          <div
            className="bg-blue-500 flex items-center justify-center text-[10px] text-white"
            style={{ width: `${sysPct}%` }}
            title={`System: ${formatTokens(budget.system_prompt)}`}
          >
            {sysPct > 5 && "Sys"}
          </div>
        )}
        {dynPct > 0 && (
          <div
            className="bg-purple-500 flex items-center justify-center text-[10px] text-white"
            style={{ width: `${dynPct}%` }}
            title={`Dynamic: ${formatTokens(budget.dynamic_context)}`}
          >
            {dynPct > 5 && "Dyn"}
          </div>
        )}
        {histPct > 0 && (
          <div
            className={`${usageColor(budget.usage_percent)} flex items-center justify-center text-[10px] text-white`}
            style={{ width: `${histPct}%` }}
            title={`History: ${formatTokens(budget.history)}`}
          >
            {histPct > 5 && "Hist"}
          </div>
        )}
      </div>

      <div className="grid grid-cols-2 gap-2 text-xs text-muted-foreground">
        <div>System Prompt: {formatTokens(budget.system_prompt)} tokens</div>
        <div>Dynamic Context: {formatTokens(budget.dynamic_context)} tokens</div>
        <div>Conversation: {formatTokens(budget.history)} tokens</div>
        <div>Free: {formatTokens(budget.free)} tokens</div>
      </div>
    </div>
  );
}
