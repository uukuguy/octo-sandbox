import type { ReactNode } from "react";
import { NavRail } from "./NavRail";
import { TabBar } from "./TabBar";

export function AppLayout({ children }: { children: ReactNode }) {
  return (
    <div className="flex h-screen w-screen overflow-hidden bg-background text-foreground">
      <NavRail />
      <div className="flex flex-1 flex-col">
        <TabBar />
        <main className="flex flex-1 flex-col overflow-hidden">{children}</main>
      </div>
    </div>
  );
}
