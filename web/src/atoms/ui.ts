import { atom } from "jotai";

export const activeTabAtom = atom<"chat" | "debug">("chat");
export const sidebarOpenAtom = atom(false);
