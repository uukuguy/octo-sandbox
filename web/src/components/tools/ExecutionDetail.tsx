import type { ToolExecutionRecord } from "@/atoms/debug";
import { TimelineView } from "./TimelineView";
import { JsonViewer } from "./JsonViewer";

interface Props {
  execution: ToolExecutionRecord | null;
  onClose: () => void;
}

export function ExecutionDetail({ execution, onClose }: Props) {
  if (!execution) return null;

  // Build timeline events from execution data
  const timelineEvents = [
    {
      id: `${execution.id}-start`,
      timestamp: execution.started_at,
      type: 'start' as const,
    },
    ...(execution.status === 'success' || execution.status === 'failed' ? [{
      id: `${execution.id}-end`,
      timestamp: execution.started_at + (execution.duration_ms || 0),
      type: 'end' as const,
      duration: execution.duration_ms || undefined,
    }] : []),
    ...(execution.error ? [{
      id: `${execution.id}-error`,
      timestamp: execution.started_at,
      type: 'error' as const,
    }] : []),
  ];

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
        {/* Timeline */}
        <TimelineView events={timelineEvents} />

        {/* Input */}
        <details open>
          <summary className="text-xs text-muted-foreground cursor-pointer">
            Input
          </summary>
          <div className="mt-1">
            <JsonViewer data={execution.input} />
          </div>
        </details>

        {/* Output */}
        {execution.output != null && (
          <details open>
            <summary className="text-xs text-muted-foreground cursor-pointer">
              Output
            </summary>
            <div className="mt-1">
              <JsonViewer data={execution.output} />
            </div>
          </details>
        )}

        {/* Error */}
        {execution.error && (
          <div className="rounded bg-red-500/10 p-2 text-xs text-red-400">
            {execution.error}
          </div>
        )}
      </div>
    </div>
  );
}
