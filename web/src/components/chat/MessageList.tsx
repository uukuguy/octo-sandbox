import { useAtomValue } from "jotai";
import { useEffect, useRef } from "react";
import { messagesAtom } from "@/atoms/session";
import { MessageBubble } from "./MessageBubble";

export function MessageList() {
  const messages = useAtomValue(messagesAtom);
  const bottomRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    bottomRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [messages]);

  return (
    <div className="flex flex-1 flex-col gap-3 overflow-y-auto p-4">
      {messages.length === 0 && (
        <div className="flex flex-1 items-center justify-center">
          <p className="text-muted-foreground">Start a conversation...</p>
        </div>
      )}
      {messages.map((msg) => (
        <MessageBubble key={msg.id} message={msg} />
      ))}
      <div ref={bottomRef} />
    </div>
  );
}
