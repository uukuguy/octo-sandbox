import { useState, useEffect } from "react";

interface Tool {
  name: string;
  description?: string;
  input_schema: object;
}

interface Server {
  id: string;
  name: string;
  tools: Tool[];
}

const mockServers: Server[] = [
  {
    id: "1",
    name: "filesystem",
    tools: [
      { name: "read_file", description: "Read a file", input_schema: { path: "string" } },
      { name: "write_file", description: "Write a file", input_schema: { path: "string", content: "string" } },
      { name: "list_directory", description: "List directory", input_schema: { path: "string" } },
    ],
  },
  {
    id: "2",
    name: "memory",
    tools: [
      { name: "memory_search", description: "Search memories", input_schema: { query: "string" } },
      { name: "memory_read", description: "Read memory", input_schema: { id: "string" } },
    ],
  },
];

export function ToolInvoker() {
  const [servers, setServers] = useState<Server[]>([]);
  const [selectedServer, setSelectedServer] = useState<string>("");
  const [selectedTool, setSelectedTool] = useState<string>("");
  const [serverTools, setServerTools] = useState<Tool[]>([]);
  const [toolsLoading, setToolsLoading] = useState(false);
  const [params, setParams] = useState<string>("{\n  \n}");
  const [result, setResult] = useState<string>("");
  const [loading, setLoading] = useState(false);

  useEffect(() => {
    fetch("/api/v1/mcp/servers")
      .then((res) => res.json())
      .then((data) => {
        if (Array.isArray(data) && data.length > 0) {
          setServers(data);
        } else {
          setServers(mockServers);
        }
      })
      .catch(() => {
        setServers(mockServers);
      });
  }, []);

  // Fetch tools when server changes
  useEffect(() => {
    if (!selectedServer) {
      setServerTools([]);
      return;
    }

    setToolsLoading(true);
    fetch(`/api/v1/mcp/servers/${selectedServer}/tools`)
      .then((res) => res.json())
      .then((data) => {
        if (Array.isArray(data)) {
          setServerTools(data);
        } else {
          // Fallback to mock
          const mock = mockServers.find((s) => s.id === selectedServer);
          setServerTools(mock?.tools || []);
        }
      })
      .catch(() => {
        const mock = mockServers.find((s) => s.id === selectedServer);
        setServerTools(mock?.tools || []);
      })
      .finally(() => setToolsLoading(false));
  }, [selectedServer]);

  const tool = serverTools.find((t) => t.name === selectedTool);

  const handleServerChange = (serverId: string) => {
    setSelectedServer(serverId);
    setSelectedTool("");
    setResult("");
  };

  const handleExecute = async () => {
    if (!selectedServer || !selectedTool) return;

    setLoading(true);
    try {
      const response = await fetch(`/api/v1/mcp/servers/${selectedServer}/call`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          tool_name: selectedTool,
          arguments: JSON.parse(params),
        }),
      });
      const data = await response.json();
      setResult(JSON.stringify(data, null, 2));
    } catch (err) {
      setResult(JSON.stringify({ error: "Failed to call tool", details: String(err) }, null, 2));
    } finally {
      setLoading(false);
    }
  };

  return (
    <div className="h-full flex flex-col">
      {/* Server & Tool Selection */}
      <div className="flex gap-4 mb-4">
        <div className="flex-1">
          <label className="block text-sm text-gray-400 mb-1">Server</label>
          <select
            className="w-full bg-gray-800 border border-gray-700 rounded px-3 py-2"
            value={selectedServer}
            onChange={(e) => handleServerChange(e.target.value)}
          >
            <option value="">选择 Server...</option>
            {servers.map((s) => (
              <option key={s.id} value={s.id}>
                {s.name}
              </option>
            ))}
          </select>
        </div>
        <div className="flex-1">
          <label className="block text-sm text-gray-400 mb-1">Tool</label>
          <select
            className="w-full bg-gray-800 border border-gray-700 rounded px-3 py-2"
            value={selectedTool}
            onChange={(e) => setSelectedTool(e.target.value)}
            disabled={!selectedServer || toolsLoading}
          >
            <option value="">{toolsLoading ? "加载中..." : "选择 Tool..."}</option>
            {serverTools.map((t) => (
              <option key={t.name} value={t.name}>
                {t.name}
              </option>
            ))}
          </select>
        </div>
      </div>

      {/* Parameters */}
      {tool && (
        <div className="mb-4">
          <label className="block text-sm text-gray-400 mb-1">
            Parameters (JSON)
          </label>
          <textarea
            className="w-full h-32 bg-gray-800 border border-gray-700 rounded px-3 py-2 font-mono text-sm"
            value={params}
            onChange={(e) => setParams(e.target.value)}
          />
        </div>
      )}

      {/* Execute Button */}
      <div className="mb-4">
        <button
          className="px-4 py-2 bg-blue-600 hover:bg-blue-500 rounded font-medium disabled:opacity-50"
          onClick={handleExecute}
          disabled={!selectedServer || !selectedTool || loading}
        >
          {loading ? "执行中..." : "执行"}
        </button>
      </div>

      {/* Result */}
      {result && (
        <div className="flex-1">
          <label className="block text-sm text-gray-400 mb-1">Result</label>
          <pre className="bg-gray-900 border border-gray-700 rounded p-4 text-sm overflow-auto h-64">
            {result}
          </pre>
        </div>
      )}
    </div>
  );
}
