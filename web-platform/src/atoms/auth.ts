import { atom } from "jotai";
import { User } from "../api/types";

// User atom
export const userAtom = atom<User | null>(null);

// Token atoms
export const accessTokenAtom = atom<string | null>(null);
export const refreshTokenAtom = atom<string | null>(null);

// Derived: is authenticated
export const isAuthenticatedAtom = atom((get) => !!get(userAtom));
