import type { ReactNode } from "react";
import { useAtomValue } from "jotai";
import { NavRail } from "./NavRail";
import { TabBar } from "./TabBar";
import { SessionBar } from "@/components/SessionBar";
import { activeTabAtom } from "@/atoms/ui";

export function AppLayout({ children }: { children: ReactNode }) {
  const activeTab = useAtomValue(activeTabAtom);

  return (
    <div className="flex h-screen w-screen overflow-hidden bg-background text-foreground">
      <NavRail />
      <div className="flex flex-1 flex-col">
        <TabBar />
        {activeTab === "chat" && <SessionBar />}
        <main className="flex flex-1 flex-col overflow-hidden">{children}</main>
      </div>
    </div>
  );
}
