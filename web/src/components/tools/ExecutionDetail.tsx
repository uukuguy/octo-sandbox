import type { ToolExecutionRecord } from "@/atoms/debug";

interface Props {
  execution: ToolExecutionRecord | null;
  onClose: () => void;
}

export function ExecutionDetail({ execution, onClose }: Props) {
  if (!execution) return null;

  return (
    <div className="border-t border-border bg-card/50 p-4">
      <div className="flex items-center justify-between mb-3">
        <h3 className="font-mono text-sm font-medium">
          {execution.tool_name}
        </h3>
        <button
          onClick={onClose}
          className="text-muted-foreground hover:text-foreground text-xs"
        >
          close
        </button>
      </div>

      <div className="space-y-3">
        <details open>
          <summary className="text-xs text-muted-foreground cursor-pointer">
            Input
          </summary>
          <pre className="mt-1 rounded bg-secondary/50 p-2 text-xs overflow-auto max-h-40">
            {JSON.stringify(execution.input, null, 2)}
          </pre>
        </details>

        {execution.output != null && (
          <details open>
            <summary className="text-xs text-muted-foreground cursor-pointer">
              Output
            </summary>
            <pre className="mt-1 rounded bg-secondary/50 p-2 text-xs overflow-auto max-h-40">
              {typeof execution.output === "string"
                ? execution.output
                : JSON.stringify(execution.output, null, 2)}
            </pre>
          </details>
        )}

        {execution.error && (
          <div className="rounded bg-red-500/10 p-2 text-xs text-red-400">
            {execution.error}
          </div>
        )}
      </div>
    </div>
  );
}
