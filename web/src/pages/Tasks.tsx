import { useState, useEffect, useCallback } from "react";

interface Task {
  id: string;
  status: "pending" | "running" | "success" | "failed";
  result?: string;
  error?: string;
}

interface TaskDetail {
  task: Task;
  executions: TaskExecution[];
}

interface TaskExecution {
  id: string;
  task_id: string;
  started_at: string;
  finished_at?: string;
  status: "pending" | "running" | "success" | "failed";
  result?: string;
  error?: string;
}

export default function Tasks() {
  const [tasks, setTasks] = useState<Task[]>([]);
  const [loading, setLoading] = useState(false);
  const [detailLoading, setDetailLoading] = useState(false);
  const [selectedTask, setSelectedTask] = useState<TaskDetail | null>(null);
  const [prompt, setPrompt] = useState("");
  const [submitting, setSubmitting] = useState(false);

  const fetchTasks = useCallback(async () => {
    setLoading(true);
    try {
      const res = await fetch("/api/tasks");
      if (!res.ok) {
        throw new Error(`HTTP ${res.status}: ${res.statusText}`);
      }
      const data = await res.json();
      setTasks(data);
    } catch (error) {
      const msg = error instanceof Error ? error.message : "Failed to fetch tasks";
      console.error("Failed to fetch tasks:", msg);
      window.alert(`Error: ${msg}`);
    } finally {
      setLoading(false);
    }
  }, []);

  const fetchTaskDetail = useCallback(async (id: string) => {
    setDetailLoading(true);
    try {
      const res = await fetch(`/api/tasks/${id}`);
      if (!res.ok) {
        throw new Error(`HTTP ${res.status}: ${res.statusText}`);
      }
      const data: TaskDetail = await res.json();
      setSelectedTask(data);
    } catch (error) {
      const msg = error instanceof Error ? error.message : "Failed to fetch task detail";
      console.error("Failed to fetch task detail:", msg);
      window.alert(`Error: ${msg}`);
    } finally {
      setDetailLoading(false);
    }
  }, []);

  const submitTask = async () => {
    if (!prompt.trim()) return;
    setSubmitting(true);
    try {
      const res = await fetch("/api/tasks", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ prompt, max_rounds: 10, timeout_secs: 300 }),
      });
      if (!res.ok) {
        throw new Error(`HTTP ${res.status}: ${res.statusText}`);
      }
      setPrompt("");
      await fetchTasks();
    } catch (error) {
      const msg = error instanceof Error ? error.message : "Failed to submit task";
      console.error("Failed to submit task:", msg);
      window.alert(`Error: ${msg}`);
    } finally {
      setSubmitting(false);
    }
  };

  const deleteTask = async (id: string) => {
    if (!confirm("Are you sure you want to delete this task?")) return;
    try {
      const res = await fetch(`/api/tasks/${id}`, { method: "DELETE" });
      if (!res.ok) {
        throw new Error(`HTTP ${res.status}: ${res.statusText}`);
      }
      await fetchTasks();
      if (selectedTask?.task.id === id) {
        setSelectedTask(null);
      }
    } catch (error) {
      const msg = error instanceof Error ? error.message : "Failed to delete task";
      console.error("Failed to delete task:", msg);
      window.alert(`Error: ${msg}`);
    }
  };

  useEffect(() => {
    fetchTasks();
  }, [fetchTasks]);

  return (
    <div className="flex flex-1 flex-col overflow-hidden">
      {/* Header */}
      <div className="px-4 py-3 border-b border-border flex items-center justify-between">
        <div>
          <h2 className="text-lg font-semibold">Background Tasks</h2>
          <p className="text-sm text-muted-foreground">
            Submit and manage AI agent tasks
          </p>
        </div>
        <button
          onClick={fetchTasks}
          disabled={loading}
          className="text-xs px-2 py-1 rounded border border-border hover:bg-secondary/50 disabled:opacity-40"
        >
          Refresh
        </button>
      </div>

      {/* Submit Form */}
      <div className="px-4 py-3 border-b border-border">
        <div className="flex gap-2">
          <input
            type="text"
            value={prompt}
            onChange={(e) => setPrompt(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && !e.shiftKey && submitTask()}
            placeholder="Enter task prompt... (Press Enter to submit)"
            className="flex-1 px-3 py-2 text-sm bg-secondary border border-border rounded-md focus:outline-none focus:ring-2 focus:ring-primary"
          />
          <button
            onClick={submitTask}
            disabled={submitting || !prompt.trim()}
            className="px-4 py-2 text-sm bg-primary text-primary-foreground rounded-md hover:bg-primary/90 disabled:opacity-40 disabled:cursor-not-allowed"
          >
            {submitting ? "Submitting..." : "Submit"}
          </button>
        </div>
      </div>

      {/* Task List and Detail */}
      <div className="flex-1 overflow-hidden flex">
        {/* Task List */}
        <div className="w-1/2 border-r border-border overflow-auto">
          {loading ? (
            <div className="flex items-center justify-center h-full">
              <span className="text-muted-foreground">Loading...</span>
            </div>
          ) : tasks.length === 0 ? (
            <div className="flex items-center justify-center h-full">
              <div className="text-center text-muted-foreground">
                <p>No tasks</p>
                <p className="text-sm mt-2">Submit a task to get started</p>
              </div>
            </div>
          ) : (
            <div className="divide-y divide-border">
              {tasks.map((task) => (
                <div
                  key={task.id}
                  onClick={() => fetchTaskDetail(task.id)}
                  className={`p-3 cursor-pointer hover:bg-secondary/50 transition-colors ${
                    selectedTask?.task.id === task.id ? "bg-secondary" : ""
                  }`}
                >
                  <div className="flex items-center justify-between">
                    <span className="font-mono text-xs text-muted-foreground">
                      {task.id.slice(0, 8)}...
                    </span>
                    <div className="flex items-center gap-2">
                      <StatusBadge status={task.status} />
                      <button
                        onClick={(e) => {
                          e.stopPropagation();
                          deleteTask(task.id);
                        }}
                        className="text-xs text-muted-foreground hover:text-destructive px-1"
                      >
                        Delete
                      </button>
                    </div>
                  </div>
                  {task.status === "failed" && task.error && (
                    <p className="text-xs text-destructive mt-1 truncate">
                      {task.error}
                    </p>
                  )}
                </div>
              ))}
            </div>
          )}
        </div>

        {/* Task Detail */}
        <div className="w-1/2 overflow-auto">
          {detailLoading ? (
            <div className="flex items-center justify-center h-full">
              <span className="text-muted-foreground">Loading...</span>
            </div>
          ) : selectedTask ? (
            <TaskDetailView task={selectedTask} />
          ) : (
            <div className="flex items-center justify-center h-full">
              <span className="text-muted-foreground text-sm">
                Select a task to view details
              </span>
            </div>
          )}
        </div>
      </div>

      {/* Stats Footer */}
      <div className="px-4 py-2 border-t border-border bg-card text-xs text-muted-foreground">
        <span>{tasks.length} tasks</span>
        <span className="mx-2">|</span>
        <span>
          {tasks.filter((t) => t.status === "running").length} running
        </span>
        <span className="mx-2">|</span>
        <span>
          {tasks.filter((t) => t.status === "success").length} completed
        </span>
      </div>
    </div>
  );
}

function TaskDetailView({ task: taskDetail }: { task: TaskDetail }) {
  const { task, executions } = taskDetail;

  return (
    <div className="p-4 space-y-4">
      {/* Task Info */}
      <div className="space-y-2">
        <div className="flex items-center justify-between">
          <h3 className="font-medium">Task Details</h3>
          <StatusBadge status={task.status} />
        </div>
        <div className="text-sm text-muted-foreground">
          <p>
            <span className="text-foreground">ID:</span>{" "}
            <span className="font-mono">{task.id}</span>
          </p>
        </div>
      </div>

      {/* Result or Error */}
      {(task.result || task.error) && (
        <div className="space-y-2">
          <h4 className="text-sm font-medium">
            {task.status === "failed" ? "Error" : "Result"}
          </h4>
          <pre
            className={`p-3 rounded-lg text-xs overflow-auto max-h-64 ${
              task.status === "failed"
                ? "bg-destructive/10 text-destructive"
                : "bg-secondary"
            }`}
          >
            {task.error || task.result || "No output"}
          </pre>
        </div>
      )}

      {/* Execution History */}
      {executions.length > 0 && (
        <div className="space-y-2">
          <h4 className="text-sm font-medium">Execution History</h4>
          <div className="space-y-2">
            {executions.map((exec) => (
              <div key={exec.id} className="p-3 bg-secondary rounded-lg text-xs">
                <div className="flex items-center justify-between mb-1">
                  <span className="font-mono text-muted-foreground">
                    {exec.id.slice(0, 8)}...
                  </span>
                  <span
                    className={`px-1.5 py-0.5 rounded ${
                      exec.status === "success"
                        ? "bg-green-100 text-green-800"
                        : exec.status === "running"
                        ? "bg-blue-100 text-blue-800"
                        : "bg-red-100 text-red-800"
                    }`}
                  >
                    {exec.status}
                  </span>
                </div>
                <p className="text-muted-foreground">
                  {new Date(exec.started_at).toLocaleString()}
                  {exec.finished_at &&
                    ` - ${new Date(exec.finished_at).toLocaleString()}`}
                </p>
                {exec.result && (
                  <pre className="mt-2 text-xs whitespace-pre-wrap">
                    {exec.result.slice(0, 500)}
                    {exec.result.length > 500 && "..."}
                  </pre>
                )}
                {exec.error && (
                  <pre className="mt-2 text-xs text-destructive whitespace-pre-wrap">
                    {exec.error.slice(0, 500)}
                    {exec.error.length > 500 && "..."}
                  </pre>
                )}
              </div>
            ))}
          </div>
        </div>
      )}
    </div>
  );
}

function StatusBadge({ status }: { status: "pending" | "running" | "success" | "failed" }) {
  const styles = {
    pending: "bg-yellow-100 text-yellow-800",
    running: "bg-blue-100 text-blue-800",
    success: "bg-green-100 text-green-800",
    failed: "bg-red-100 text-red-800",
  };
  const style = styles[status as keyof typeof styles] || "";

  return (
    <span className={`px-2 py-0.5 rounded text-xs font-medium ${style}`}>
      {status}
    </span>
  );
}
