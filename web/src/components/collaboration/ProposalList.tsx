import { useAtom } from "jotai";
import { collaborationProposalsAtom } from "@/atoms/collaboration";
import { cn } from "@/lib/utils";

const STATUS_COLORS: Record<string, string> = {
  Pending: "bg-yellow-500/10 text-yellow-400 border-yellow-500/20",
  Accepted: "bg-green-500/10 text-green-400 border-green-500/20",
  Rejected: "bg-red-500/10 text-red-400 border-red-500/20",
};

async function castVote(proposalId: string, agentId: string, approve: boolean) {
  try {
    await fetch(`/api/v1/collaboration/proposals/${proposalId}/vote`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ agent_id: agentId, approve, reason: null }),
    });
  } catch {
    // Silently fail — event stream will reflect the result
  }
}

export function ProposalList() {
  const [proposals] = useAtom(collaborationProposalsAtom);

  if (proposals.length === 0) {
    return (
      <p className="text-xs text-muted-foreground py-4 text-center">
        No proposals yet.
      </p>
    );
  }

  return (
    <div className="space-y-2">
      {proposals.map((p) => (
        <div key={p.id} className="rounded border border-border p-3 bg-card">
          <div className="flex items-center justify-between mb-1">
            <span className="text-sm font-medium">{p.action}</span>
            <span
              className={cn(
                "inline-flex items-center rounded px-1.5 py-0.5 text-[10px] font-medium border",
                STATUS_COLORS[p.status] ?? "bg-secondary text-muted-foreground",
              )}
            >
              {p.status}
            </span>
          </div>
          <p className="text-xs text-muted-foreground mb-1">{p.description}</p>
          <div className="text-xs text-muted-foreground mb-2">
            Proposed by: <span className="font-mono">{p.from_agent}</span>
          </div>

          {/* Votes */}
          {p.votes.length > 0 && (
            <div className="mb-2 space-y-0.5">
              {p.votes.map((v, i) => (
                <div key={i} className="flex items-center gap-2 text-xs">
                  <span className={v.approve ? "text-green-400" : "text-red-400"}>
                    {v.approve ? "+" : "-"}
                  </span>
                  <span className="font-mono text-muted-foreground">{v.agent_id}</span>
                  {v.reason && <span className="text-muted-foreground">({v.reason})</span>}
                </div>
              ))}
            </div>
          )}

          {/* Vote buttons (only for Pending) */}
          {p.status === "Pending" && (
            <div className="flex gap-2 mt-2">
              <button
                onClick={() => castVote(p.id, "user", true)}
                className="text-xs px-2 py-0.5 rounded bg-green-500/10 text-green-400 border border-green-500/20 hover:bg-green-500/20 transition-colors"
              >
                Approve
              </button>
              <button
                onClick={() => castVote(p.id, "user", false)}
                className="text-xs px-2 py-0.5 rounded bg-red-500/10 text-red-400 border border-red-500/20 hover:bg-red-500/20 transition-colors"
              >
                Reject
              </button>
            </div>
          )}
        </div>
      ))}
    </div>
  );
}
