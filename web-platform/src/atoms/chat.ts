import { atom } from "jotai";
import { ChatMessage } from "../api/types";

// Messages for current session
export const messagesAtom = atom<ChatMessage[]>([]);

// Input text
export const inputAtom = atom<string>("");

// Streaming state
export const isStreamingAtom = atom<boolean>(false);

// Connection state
export const isConnectedAtom = atom<boolean>(false);
