import { useState, useEffect } from "react";

interface MemoryBlock {
  id: string;
  content: string;
  role: string;
  token_count: number;
}

interface PersistentMemory {
  id: string;
  content: string;
  category: string;
  importance: number;
  created_at: string;
}

type MemoryType = "working" | "session" | "persistent";

export default function Memory() {
  const [activeMemory, setActiveMemory] = useState<MemoryType>("working");
  const [workingMemory, setWorkingMemory] = useState<MemoryBlock[]>([]);
  const [persistentMemory, setPersistentMemory] = useState<PersistentMemory[]>([]);
  const [loading, setLoading] = useState(false);
  const [searchQuery, setSearchQuery] = useState("");

  useEffect(() => {
    fetchWorkingMemory();
    fetchPersistentMemory();
  }, []);

  const fetchWorkingMemory = async () => {
    setLoading(true);
    try {
      const res = await fetch("/api/memories/working");
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
      const res = await fetch(`/api/memories?limit=50`);
      const data = await res.json();
      setPersistentMemory(data.results || []);
    } catch (error) {
      console.error("Failed to fetch persistent memory:", error);
    } finally {
      setLoading(false);
    }
  };

  const filteredWorkingMemory = workingMemory.filter((block) =>
    block.content.toLowerCase().includes(searchQuery.toLowerCase())
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
      <div className="px-4 py-3 border-b border-border">
        <h2 className="text-lg font-semibold">Memory Explorer</h2>
        <p className="text-sm text-muted-foreground">
          View and manage AI memory across different layers
        </p>
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
          <SessionMemoryView />
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
          Persistent: {persistentMemory.length} memories
        </span>
      </div>
    </div>
  );
}

function WorkingMemoryView({ blocks }: { blocks: MemoryBlock[] }) {
  const totalTokens = blocks.reduce((sum, b) => sum + b.token_count, 0);

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
        <span className="text-sm text-muted-foreground">
          {totalTokens} tokens
        </span>
      </div>
      <div className="space-y-2">
        {blocks.map((block, index) => (
          <div
            key={block.id || index}
            className="p-3 bg-secondary rounded-lg border border-border"
          >
            <div className="flex items-center justify-between mb-2">
              <span className="text-xs font-medium px-2 py-0.5 bg-primary/10 rounded">
                {block.role}
              </span>
              <span className="text-xs text-muted-foreground">
                {block.token_count} tokens
              </span>
            </div>
            <p className="text-sm line-clamp-3">{block.content}</p>
          </div>
        ))}
      </div>
    </div>
  );
}

function SessionMemoryView() {
  const [sessionMemories] = useState<PersistentMemory[]>([]);

  if (sessionMemories.length === 0) {
    return (
      <div className="text-center text-muted-foreground py-8">
        <p>No session memories</p>
        <p className="text-sm mt-2">
          Session memories are accumulated during this conversation
        </p>
      </div>
    );
  }

  return (
    <div className="space-y-4">
      {sessionMemories.map((mem) => (
        <div
          key={mem.id}
          className="p-3 bg-secondary rounded-lg border border-border"
        >
          <div className="flex items-center justify-between mb-2">
            <span className="text-xs font-medium px-2 py-0.5 bg-primary/10 rounded">
              {mem.category}
            </span>
            <span className="text-xs text-muted-foreground">
              {mem.created_at}
            </span>
          </div>
          <p className="text-sm">{mem.content}</p>
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
