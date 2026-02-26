import { cn } from "@/lib/utils";
import type { ChatMsg } from "@/atoms/session";
import { Brain } from "lucide-react";

interface Props {
  message: ChatMsg;
}

export function MessageBubble({ message }: Props) {
  const isUser = message.role === "user";

  return (
    <div className={cn("flex w-full", isUser ? "justify-end" : "justify-start")}>
      <div
        className={cn(
          "max-w-[80%] rounded-lg px-4 py-2 text-sm whitespace-pre-wrap",
          isUser
            ? "bg-primary text-primary-foreground"
            : "bg-secondary text-foreground",
        )}
      >
        {message.thinking && (
          <details className="mb-2">
            <summary className="flex cursor-pointer items-center gap-1.5 text-xs text-muted-foreground">
              <Brain className="h-3 w-3" />
              <span>Thinking</span>
            </summary>
            <div className="mt-1 max-h-60 overflow-y-auto rounded-md bg-muted/50 px-3 py-2 text-xs text-muted-foreground whitespace-pre-wrap font-mono">
              {message.thinking}
            </div>
          </details>
        )}
        {message.content}
      </div>
    </div>
  );
}
