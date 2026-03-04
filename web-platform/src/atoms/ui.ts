import { atom } from "jotai";

// Sidebar collapsed
export const sidebarCollapsedAtom = atom<boolean>(false);

// Loading states
export const isLoadingAtom = atom<boolean>(false);

// Error message
export const errorAtom = atom<string | null>(null);
