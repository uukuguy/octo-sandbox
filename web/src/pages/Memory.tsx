import { useState, useEffect } from "react";
import { useAtomValue } from "jotai";
import { sessionIdAtom } from "@/atoms/session";

interface MemoryBlock {
  id: string;
  kind: string;
  label: string;
  value: string;
  priority: number;
  char_limit: number;
  is_readonly: boolean;
}

interface PersistentMemory {
  id: string;
  content: string;
  category: string;
  importance: number;
  created_at: string;
}

interface ChatMessage {
  role: "user" | "assistant" | "system";
  content: Array<{ type: string; text?: string }>;
}

type MemoryType = "working" | "session" | "persistent";

export default function Memory() {
  const [activeMemory, setActiveMemory] = useState<MemoryType>("working");
  const [workingMemory, setWorkingMemory] = useState<MemoryBlock[]>([]);
  const [persistentMemory, setPersistentMemory] = useState<PersistentMemory[]>([]);
  const [sessionMessages, setSessionMessages] = useState<ChatMessage[]>([]);
  const [loading, setLoading] = useState(false);
  const [searchQuery, setSearchQuery] = useState("");
  const sessionId = useAtomValue(sessionIdAtom);

  useEffect(() => {
    fetchWorkingMemory();
    fetchPersistentMemory();
  }, []);

  useEffect(() => {
    if (activeMemory === "session" && sessionId) {
      fetchSessionMessages();
    }
  }, [activeMemory, sessionId]);

  const fetchSessionMessages = async () => {
    if (!sessionId) return;
    setLoading(true);
    try {
      const res = await fetch(`/api/v1/sessions/${sessionId}`);
      const data = await res.json();
      setSessionMessages(data.messages || []);
    } catch (error) {
      console.error("Failed to fetch session messages:", error);
    } finally {
      setLoading(false);
    }
  };

  const fetchWorkingMemory = async () => {
    setLoading(true);
    try {
      const res = await fetch("/api/v1/memories/working");
      const data = await res.json();
      setWorkingMemory(data.blocks || []);
    } catch (error) {
      console.error("Failed to fetch working memory:", error);
    } finally {
      setLoading(false);
    }
  };

  const fetchPersistentMemory = async () => {
    setLoading(true);
    try {
      const res = await fetch(`/api/v1/memories?limit=50`);
      const data = await res.json();
      setPersistentMemory(data.results || []);
    } catch (error) {
      console.error("Failed to fetch persistent memory:", error);
    } finally {
      setLoading(false);
    }
  };

  const filteredWorkingMemory = workingMemory.filter((block) =>
    (block.value + " " + block.label).toLowerCase().includes(searchQuery.toLowerCase())
  );

  const filteredPersistentMemory = persistentMemory.filter((mem) =>
    mem.content.toLowerCase().includes(searchQuery.toLowerCase())
  );

  const memoryTypes: { id: MemoryType; label: string }[] = [
    { id: "working", label: "Working Memory" },
    { id: "session", label: "Session Memory" },
    { id: "persistent", label: "Persistent Memory" },
  ];

  return (
    <div className="flex flex-1 flex-col overflow-hidden">
      {/* Header */}
      <div className="px-4 py-3 border-b border-border flex items-center justify-between">
        <div>
          <h2 className="text-lg font-semibold">Memory Explorer</h2>
          <p className="text-sm text-muted-foreground">
            View and manage AI memory across different layers
          </p>
        </div>
        <button
          onClick={() => {
            fetchWorkingMemory();
            fetchPersistentMemory();
            if (sessionId) fetchSessionMessages();
          }}
          className="text-xs px-2 py-1 rounded border border-border hover:bg-secondary/50"
        >
          Refresh
        </button>
      </div>

      {/* Memory Type Tabs */}
      <div className="flex gap-2 px-4 py-2 border-b border-border">
        {memoryTypes.map((type) => (
          <button
            key={type.id}
            onClick={() => setActiveMemory(type.id)}
            className={`px-3 py-1.5 text-sm rounded-md transition-colors ${
              activeMemory === type.id
                ? "bg-secondary text-foreground"
                : "text-muted-foreground hover:text-foreground hover:bg-secondary/50"
            }`}
          >
            {type.label}
          </button>
        ))}
      </div>

      {/* Search */}
      <div className="px-4 py-2 border-b border-border">
        <input
          type="text"
          placeholder="Search memories..."
          value={searchQuery}
          onChange={(e) => setSearchQuery(e.target.value)}
          className="w-full px-3 py-2 text-sm bg-secondary border border-border rounded-md focus:outline-none focus:ring-2 focus:ring-primary"
        />
      </div>

      {/* Memory Content */}
      <div className="flex-1 overflow-auto p-4">
        {loading ? (
          <div className="flex items-center justify-center h-full">
            <span className="text-muted-foreground">Loading...</span>
          </div>
        ) : activeMemory === "working" ? (
          <WorkingMemoryView blocks={filteredWorkingMemory} />
        ) : activeMemory === "session" ? (
          <SessionMemoryView messages={sessionMessages} />
        ) : (
          <PersistentMemoryView memories={filteredPersistentMemory} />
        )}
      </div>

      {/* Stats Footer */}
      <div className="px-4 py-2 border-t border-border bg-card text-xs text-muted-foreground">
        <span className="mr-4">
          Working: {workingMemory.length} blocks
        </span>
        <span className="mr-4">
          Session: {sessionMessages.length} messages
        </span>
        <span className="mr-4">
          Persistent: {persistentMemory.length} memories
        </span>
      </div>
    </div>
  );
}

function WorkingMemoryView({ blocks }: { blocks: MemoryBlock[] }) {
  if (blocks.length === 0) {
    return (
      <div className="text-center text-muted-foreground py-8">
        <p>No working memory blocks</p>
        <p className="text-sm mt-2">Working memory contains current context</p>
      </div>
    );
  }

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between">
        <h3 className="font-medium">Context Blocks</h3>
        <span className="text-sm text-muted-foreground">{blocks.length} blocks</span>
      </div>
      <div className="space-y-2">
        {blocks.map((block) => (
          <div
            key={block.id}
            className="p-3 bg-secondary rounded-lg border border-border"
          >
            <div className="flex items-center justify-between mb-2">
              <div className="flex items-center gap-2">
                <span className="text-xs font-medium px-2 py-0.5 bg-primary/10 rounded font-mono">
                  {block.kind}
                </span>
                <span className="text-xs text-muted-foreground">{block.label}</span>
              </div>
              <span className="text-xs text-muted-foreground">
                {block.char_limit} char limit
              </span>
            </div>
            {block.value ? (
              <p className="text-sm">{block.value}</p>
            ) : (
              <p className="text-sm text-muted-foreground italic">empty</p>
            )}
          </div>
        ))}
      </div>
    </div>
  );
}

function SessionMemoryView({ messages }: { messages: ChatMessage[] }) {
  if (messages.length === 0) {
    return (
      <div className="text-center text-muted-foreground py-8">
        <p>No session messages</p>
        <p className="text-sm mt-2">
          Start a conversation to see messages here
        </p>
      </div>
    );
  }

  // Extract text content from message
  const getTextContent = (msg: ChatMessage): string => {
    return msg.content
      .filter((c) => c.type === "text" && c.text)
      .map((c) => c.text)
      .join("");
  };

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between">
        <h3 className="font-medium">Conversation History</h3>
        <span className="text-sm text-muted-foreground">{messages.length} messages</span>
      </div>
      {messages.map((msg, idx) => (
        <div
          key={idx}
          className={`p-3 rounded-lg border ${
            msg.role === "user"
              ? "bg-primary/5 border-border"
              : msg.role === "assistant"
              ? "bg-secondary border-border"
              : "bg-muted border-border"
          }`}
        >
          <div className="flex items-center gap-2 mb-2">
            <span className="text-xs font-medium px-2 py-0.5 bg-primary/10 rounded">
              {msg.role}
            </span>
          </div>
          <p className="text-sm whitespace-pre-wrap">{getTextContent(msg)}</p>
        </div>
      ))}
    </div>
  );
}

function PersistentMemoryView({ memories }: { memories: PersistentMemory[] }) {
  if (memories.length === 0) {
    return (
      <div className="text-center text-muted-foreground py-8">
        <p>No persistent memories</p>
        <p className="text-sm mt-2">
          Use memory_store tool to save important information
        </p>
      </div>
    );
  }

  const categories = [...new Set(memories.map((m) => m.category))];

  return (
    <div className="space-y-4">
      {/* Category Filter */}
      <div className="flex gap-2 flex-wrap">
        {categories.map((cat) => (
          <span
            key={cat}
            className="text-xs px-2 py-1 bg-secondary rounded-full text-muted-foreground"
          >
            {cat}
          </span>
        ))}
      </div>

      {/* Memory List */}
      <div className="space-y-2">
        {memories.map((mem) => (
          <div
            key={mem.id}
            className="p-3 bg-secondary rounded-lg border border-border hover:border-primary/50 transition-colors"
          >
            <div className="flex items-center justify-between mb-2">
              <div className="flex items-center gap-2">
                <span className="text-xs font-medium px-2 py-0.5 bg-primary/10 rounded">
                  {mem.category}
                </span>
                <span className="text-xs text-muted-foreground">
                  {mem.importance.toFixed(1)}
                </span>
              </div>
              <span className="text-xs text-muted-foreground">
                {mem.created_at}
              </span>
            </div>
            <p className="text-sm line-clamp-3">{mem.content}</p>
          </div>
        ))}
      </div>
    </div>
  );
}
