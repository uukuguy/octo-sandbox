import { cn } from "@/lib/utils";

export function TabBar() {
  return (
    <div className="flex h-10 items-center border-b border-border bg-card px-4">
      <button
        className={cn(
          "rounded-md px-3 py-1 text-sm font-medium",
          "bg-secondary text-foreground",
        )}
      >
        Chat
      </button>
    </div>
  );
}
