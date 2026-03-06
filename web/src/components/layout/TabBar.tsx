import { useAtom } from "jotai";
import { cn } from "@/lib/utils";
import { activeTabAtom, type TabId } from "@/atoms/ui";
import { Server, ListTodo, Calendar } from "lucide-react";

const tabs: { id: TabId; label: string; icon?: React.ComponentType<{ className?: string }> }[] = [
  { id: "chat", label: "Chat" },
  { id: "tasks", label: "Tasks", icon: ListTodo },
  { id: "schedule", label: "Schedule", icon: Calendar },
  { id: "tools", label: "Tools" },
  { id: "memory", label: "Memory" },
  { id: "debug", label: "Debug" },
  { id: "mcp", label: "MCP", icon: Server },
];

export function TabBar() {
  const [activeTab, setActiveTab] = useAtom(activeTabAtom);

  return (
    <div className="flex h-10 items-center gap-1 border-b border-border bg-card px-4">
      {tabs.map((tab) => (
        <button
          key={tab.id}
          onClick={() => setActiveTab(tab.id)}
          className={cn(
            "rounded-md px-3 py-1 text-sm font-medium transition-colors",
            activeTab === tab.id
              ? "bg-secondary text-foreground"
              : "text-muted-foreground hover:text-foreground hover:bg-secondary/50",
          )}
        >
          <span className="flex items-center gap-1.5">
            {tab.icon && <tab.icon className="h-4 w-4" />}
            {tab.label}
          </span>
        </button>
      ))}
    </div>
  );
}
