import { useAtomValue } from "jotai";
import { isStreamingAtom, streamingTextAtom, streamingThinkingAtom, toolExecutionsAtom } from "@/atoms/session";
import { Loader2, Terminal, FileText, Brain } from "lucide-react";

export function StreamingDisplay() {
  const isStreaming = useAtomValue(isStreamingAtom);
  const streamingText = useAtomValue(streamingTextAtom);
  const streamingThinking = useAtomValue(streamingThinkingAtom);
  const toolExecs = useAtomValue(toolExecutionsAtom);

  if (!isStreaming) return null;

  return (
    <div className="border-t border-border px-4 py-3">
      {streamingThinking && (
        <details className="mb-2" open>
          <summary className="flex cursor-pointer items-center gap-1.5 text-xs text-muted-foreground">
            <Brain className="h-3 w-3" />
            <span>Thinking...</span>
          </summary>
          <div className="mt-1 max-h-40 overflow-y-auto rounded-md bg-muted/50 px-3 py-2 text-xs text-muted-foreground whitespace-pre-wrap font-mono">
            {streamingThinking}
            <span className="ml-0.5 inline-block h-3 w-1 animate-pulse bg-muted-foreground" />
          </div>
        </details>
      )}
      {toolExecs.length > 0 && (
        <div className="mb-2 space-y-1">
          {toolExecs.map((tool) => (
            <div
              key={tool.toolId}
              className="flex items-center gap-2 text-xs text-muted-foreground"
            >
              {tool.toolName === "bash" ? (
                <Terminal className="h-3 w-3" />
              ) : (
                <FileText className="h-3 w-3" />
              )}
              <span className="font-mono">
                {tool.toolName}
                {tool.status === "running" && (
                  <Loader2 className="ml-1 inline h-3 w-3 animate-spin" />
                )}
                {tool.status === "complete" && (
                  <span className={tool.success ? "text-green-500" : "text-red-500"}>
                    {" "}
                    {tool.success ? "done" : "failed"}
                  </span>
                )}
              </span>
            </div>
          ))}
        </div>
      )}
      {streamingText && (
        <div className="max-w-[80%] rounded-lg bg-secondary px-4 py-2 text-sm whitespace-pre-wrap text-foreground">
          {streamingText}
          <span className="ml-0.5 inline-block h-4 w-1.5 animate-pulse bg-foreground" />
        </div>
      )}
    </div>
  );
}
