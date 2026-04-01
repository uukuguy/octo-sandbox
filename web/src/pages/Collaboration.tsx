import { useAtom, useSetAtom } from "jotai";
import { useEffect, useCallback } from "react";
import {
  collaborationStatusAtom,
  collaborationAgentsAtom,
  collaborationProposalsAtom,
  collaborationEventsAtom,
  collaborationSharedStateAtom,
  collaborationLoadingAtom,
} from "@/atoms/collaboration";
import { AgentList } from "@/components/collaboration/AgentList";
import { EventLog } from "@/components/collaboration/EventLog";
import { ProposalList } from "@/components/collaboration/ProposalList";
import { SharedState } from "@/components/collaboration/SharedState";

export default function Collaboration() {
  const [status] = useAtom(collaborationStatusAtom);
  const [loading] = useAtom(collaborationLoadingAtom);
  const setStatus = useSetAtom(collaborationStatusAtom);
  const setAgents = useSetAtom(collaborationAgentsAtom);
  const setProposals = useSetAtom(collaborationProposalsAtom);
  const setEvents = useSetAtom(collaborationEventsAtom);
  const setSharedState = useSetAtom(collaborationSharedStateAtom);
  const setLoading = useSetAtom(collaborationLoadingAtom);

  const refresh = useCallback(async () => {
    setLoading(true);
    try {
      const [statusRes, agentsRes, proposalsRes, eventsRes, stateRes] = await Promise.all([
        fetch("/api/v1/collaboration/status").then((r) => r.json()),
        fetch("/api/v1/collaboration/agents").then((r) => r.json()),
        fetch("/api/v1/collaboration/proposals").then((r) => r.json()),
        fetch("/api/v1/collaboration/events").then((r) => r.json()),
        fetch("/api/v1/collaboration/shared-state").then((r) => r.json()),
      ]);
      setStatus(statusRes);
      setAgents(agentsRes);
      setProposals(proposalsRes);
      setEvents(eventsRes.map((e: { event?: unknown }) => e.event ?? e));
      setSharedState(stateRes.entries ?? []);
    } catch {
      // Silently handle — endpoints may not be available yet
    } finally {
      setLoading(false);
    }
  }, [setStatus, setAgents, setProposals, setEvents, setSharedState, setLoading]);

  // Fetch on mount and poll every 5s
  useEffect(() => {
    refresh();
    const interval = setInterval(refresh, 5000);
    return () => clearInterval(interval);
  }, [refresh]);

  return (
    <div className="flex flex-1 flex-col overflow-auto">
      {/* Header */}
      <div className="flex items-center justify-between px-4 py-2 border-b border-border">
        <div>
          <h2 className="text-sm font-medium">Agent Collaboration</h2>
          <p className="text-xs text-muted-foreground mt-0.5">
            Multi-agent collaboration state, proposals, and shared context.
          </p>
        </div>
        <button
          onClick={refresh}
          disabled={loading}
          className="text-xs px-3 py-1 rounded bg-secondary text-muted-foreground hover:text-foreground disabled:opacity-50 transition-colors"
        >
          {loading ? "Loading..." : "Refresh"}
        </button>
      </div>

      {/* Status summary */}
      {status && (
        <div className="px-4 py-2 border-b border-border">
          <div className="grid grid-cols-4 gap-4 text-xs">
            <div>
              <span className="text-muted-foreground">Agents:</span>{" "}
              <span className="font-mono">{status.agent_count}</span>
            </div>
            <div>
              <span className="text-muted-foreground">Active:</span>{" "}
              <span className="font-mono">{status.active_agent ?? "none"}</span>
            </div>
            <div>
              <span className="text-muted-foreground">Pending:</span>{" "}
              <span className="font-mono">{status.pending_proposals}</span>
            </div>
            <div>
              <span className="text-muted-foreground">Events:</span>{" "}
              <span className="font-mono">{status.event_count}</span>
            </div>
          </div>
        </div>
      )}

      {/* Four panels in a 2x2 grid */}
      <div className="flex-1 grid grid-cols-2 gap-0 overflow-auto">
        {/* Agents */}
        <div className="border-r border-b border-border p-3 overflow-auto">
          <h3 className="text-xs font-medium text-muted-foreground uppercase tracking-wide mb-2">
            Agents
          </h3>
          <AgentList />
        </div>

        {/* Proposals */}
        <div className="border-b border-border p-3 overflow-auto">
          <h3 className="text-xs font-medium text-muted-foreground uppercase tracking-wide mb-2">
            Proposals
          </h3>
          <ProposalList />
        </div>

        {/* Event Log */}
        <div className="border-r border-border p-3 overflow-auto">
          <h3 className="text-xs font-medium text-muted-foreground uppercase tracking-wide mb-2">
            Event Timeline
          </h3>
          <EventLog />
        </div>

        {/* Shared State */}
        <div className="p-3 overflow-auto">
          <h3 className="text-xs font-medium text-muted-foreground uppercase tracking-wide mb-2">
            Shared State
          </h3>
          <SharedState />
        </div>
      </div>
    </div>
  );
}
