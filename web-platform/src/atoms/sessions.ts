import { atom } from "jotai";
import { Session } from "../api/types";

// Sessions list
export const sessionsAtom = atom<Session[]>([]);

// Current session
export const currentSessionIdAtom = atom<string | null>(null);

// Derived: current session
export const currentSessionAtom = atom((get) => {
  const sessions = get(sessionsAtom);
  const currentId = get(currentSessionIdAtom);
  return sessions.find((s) => s.id === currentId) || null;
});
