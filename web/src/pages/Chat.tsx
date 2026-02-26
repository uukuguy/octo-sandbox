import { useEffect } from "react";
import { useStore } from "jotai";
import { MessageList } from "@/components/chat/MessageList";
import { ChatInput } from "@/components/chat/ChatInput";
import { StreamingDisplay } from "@/components/chat/StreamingDisplay";
import { wsManager } from "@/ws/manager";
import { handleWsEvent } from "@/ws/events";

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

  useEffect(() => {
    wsManager.connect();

    wsManager.onMessage((msg) => {
      handleWsEvent(msg, store.set);
    });

    return () => wsManager.disconnect();
  }, [store]);

  return null;
}
