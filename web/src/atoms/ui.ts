import { atom } from "jotai";

export type TabId = "chat" | "tasks" | "schedule" | "tools" | "debug" | "memory" | "mcp";
export const activeTabAtom = atom<TabId>("chat");
export const sidebarOpenAtom = atom(false);
