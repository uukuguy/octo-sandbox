import { useAtom } from "jotai";
import { executionRecordsAtom, selectedExecutionIdAtom } from "@/atoms/debug";
import { ExecutionDetail } from "./ExecutionDetail";

export function ExecutionList() {
  const [executions] = useAtom(executionRecordsAtom);
  const [selectedId, setSelectedId] = useAtom(selectedExecutionIdAtom);

  if (executions.length === 0) {
    return (
      <div className="flex flex-1 items-center justify-center text-muted-foreground text-sm">
        No tool executions yet. Start a conversation to see tool calls here.
      </div>
    );
  }

  return (
    <div className="flex flex-col overflow-auto">
      <table className="w-full text-sm">
        <thead className="sticky top-0 bg-card border-b border-border">
          <tr className="text-left text-muted-foreground">
            <th className="px-3 py-2 font-medium">Tool</th>
            <th className="px-3 py-2 font-medium">Source</th>
            <th className="px-3 py-2 font-medium">Status</th>
            <th className="px-3 py-2 font-medium">Duration</th>
            <th className="px-3 py-2 font-medium">Time</th>
          </tr>
        </thead>
        <tbody>
          {executions.map((exec) => (
            <tr
              key={exec.id}
              onClick={() =>
                setSelectedId(selectedId === exec.id ? null : exec.id)
              }
              className="border-b border-border/50 cursor-pointer hover:bg-secondary/30"
            >
              <td className="px-3 py-2 font-mono">{exec.tool_name}</td>
              <td className="px-3 py-2 text-muted-foreground">
                {typeof exec.source === "string" ? exec.source : "built_in"}
              </td>
              <td className="px-3 py-2">
                <StatusBadge status={exec.status} />
              </td>
              <td className="px-3 py-2 text-muted-foreground">
                {exec.duration_ms != null
                  ? `${(exec.duration_ms / 1000).toFixed(1)}s`
                  : "\u2014"}
              </td>
              <td className="px-3 py-2 text-muted-foreground">
                {new Date(exec.started_at).toLocaleTimeString()}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
      {selectedId && (
        <ExecutionDetail
          execution={executions.find((e) => e.id === selectedId) ?? null}
          onClose={() => setSelectedId(null)}
        />
      )}
    </div>
  );
}

function StatusBadge({ status }: { status: string }) {
  const styles: Record<string, string> = {
    running: "text-yellow-500",
    success: "text-green-500",
    failed: "text-red-500",
    timeout: "text-orange-500",
  };
  const icons: Record<string, string> = {
    running: "...",
    success: "ok",
    failed: "err",
    timeout: "t/o",
  };
  return (
    <span className={`font-mono text-xs ${styles[status] ?? ""}`}>
      {icons[status] ?? status}
    </span>
  );
}
