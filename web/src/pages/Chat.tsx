import { useEffect } from "react";
import { useStore, useAtomValue } from "jotai";
import { MessageList } from "@/components/chat/MessageList";
import { ChatInput } from "@/components/chat/ChatInput";
import { StreamingDisplay } from "@/components/chat/StreamingDisplay";
import { wsManager } from "@/ws/manager";
import { handleWsEvent } from "@/ws/events";
import { sessionIdAtom } from "@/atoms/session";
import { executionRecordsAtom } from "@/atoms/debug";
import type { ToolExecutionRecord } from "@/atoms/debug";

export default function Chat() {
  return (
    <>
      <WsEventBridge />
      <div className="flex flex-1 flex-col overflow-hidden">
        <MessageList />
        <StreamingDisplay />
        <ChatInput />
      </div>
    </>
  );
}

function WsEventBridge() {
  const store = useStore();
  const sessionId = useAtomValue(sessionIdAtom);

  // When session is established, load execution history from API
  useEffect(() => {
    if (!sessionId) return;
    fetch(`/api/v1/sessions/${sessionId}/executions?limit=100`)
      .then((res) => res.ok ? res.json() : [])
      .then((data: ToolExecutionRecord[]) => {
        if (!Array.isArray(data) || data.length === 0) return;
        store.set(executionRecordsAtom, (prev) => {
          // Merge: keep any newer real-time records, backfill with DB history
          const existingIds = new Set(prev.map((e) => e.id));
          const newRecords = data.filter((e) => !existingIds.has(e.id));
          // Prepend history (older), append real-time (newer)
          return [...newRecords, ...prev];
        });
      })
      .catch(() => {/* ignore */});
  }, [sessionId, store]);

  useEffect(() => {
    wsManager.connect();
    wsManager.onMessage((msg) => {
      handleWsEvent(msg, store.set);
    });
    return () => wsManager.disconnect();
  }, [store]);

  return null;
}
