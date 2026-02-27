import { atom } from "jotai";

export type TabId = "chat" | "tools" | "debug" | "memory";
export const activeTabAtom = atom<TabId>("chat");
export const sidebarOpenAtom = atom(false);
