// Client → Server
export type ClientMessage =
  | { type: "send_message"; session_id?: string; content: string }
  | { type: "cancel"; session_id: string };

// Server → Client
export type ServerMessage =
  | { type: "session_created"; session_id: string }
  | { type: "text_delta"; session_id: string; text: string }
  | { type: "text_complete"; session_id: string; text: string }
  | { type: "thinking_delta"; session_id: string; text: string }
  | { type: "thinking_complete"; session_id: string; text: string }
  | {
      type: "tool_start";
      session_id: string;
      tool_id: string;
      tool_name: string;
      input: Record<string, unknown>;
    }
  | {
      type: "tool_result";
      session_id: string;
      tool_id: string;
      output: string;
      success: boolean;
    }
  | { type: "error"; session_id: string; message: string }
  | { type: "done"; session_id: string };
