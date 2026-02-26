import { useAtom } from "jotai";
import { cn } from "@/lib/utils";
import { activeTabAtom, type TabId } from "@/atoms/ui";

const tabs: { id: TabId; label: string }[] = [
  { id: "chat", label: "Chat" },
  { id: "tools", label: "Tools" },
  { id: "debug", label: "Debug" },
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
          {tab.label}
        </button>
      ))}
    </div>
  );
}
