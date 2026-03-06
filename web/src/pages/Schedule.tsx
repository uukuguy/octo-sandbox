import { useState, useEffect, useCallback } from "react";

// Types matching backend API
interface AgentTaskConfig {
  system_prompt?: string;
  input: string;
  max_rounds?: number;
  timeout_secs?: number;
  model?: string;
}

interface ScheduledTask {
  id: string;
  user_id?: string;
  name: string;
  cron: string;
  agent_config: AgentTaskConfig;
  enabled: boolean;
  last_run?: string;
  next_run?: string;
  created_at: string;
  updated_at: string;
}

interface TaskExecution {
  id: string;
  task_id: string;
  started_at: string;
  finished_at?: string;
  status: string;
  result?: string;
  error?: string;
}

interface CreateTaskForm {
  name: string;
  cron: string;
  system_prompt: string;
  input: string;
  max_rounds: number;
  timeout_secs: number;
  model: string;
}

const DEFAULT_CRON = "0 * * * *";
const DEFAULT_MAX_ROUNDS = 50;
const DEFAULT_TIMEOUT_SECS = 300;
const DEFAULT_MODEL = "claude-3-5-sonnet-20241022";

const CRON_EXAMPLES = [
  { label: "Every hour", value: "0 * * * *" },
  { label: "Every day at 9am", value: "0 9 * * *" },
  { label: "Every Monday at 9am", value: "0 9 * * 1" },
  { label: "Every 30 minutes", value: "*/30 * * * *" },
];

interface ApiResponse<T> {
  tasks?: T;
}

// Basic cron validation: 5 fields (minute, hour, day, month, weekday)
function isValidCron(cron: string): boolean {
  const parts = cron.trim().split(/\s+/);
  if (parts.length !== 5) return false;
  // Allow digits, *, and simple ranges like 0-5, lists like 1,2,3
  const fieldPattern = /^(\*|[0-9]+(-[0-9]+)?(,[0-9]+(-[0-9]+)?)*)$/;
  return parts.every((p) => fieldPattern.test(p));
}

export default function Schedule() {
  const [tasks, setTasks] = useState<ScheduledTask[]>([]);
  const [loading, setLoading] = useState(false);
  const [showAddModal, setShowAddModal] = useState(false);
  const [selectedTask, setSelectedTask] = useState<ScheduledTask | null>(null);
  const [executions, setExecutions] = useState<TaskExecution[]>([]);
  const [executionsLoading, setExecutionsLoading] = useState(false);
  const [runningTaskId, setRunningTaskId] = useState<string | null>(null);
  const [deletingTaskId, setDeletingTaskId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const fetchTasks = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const res = await fetch("/api/scheduler/tasks");
      if (!res.ok) {
        throw new Error(`HTTP ${res.status}: ${res.statusText}`);
      }
      const data: unknown = await res.json();
      // Validate response structure
      if (data && typeof data === "object" && "tasks" in data) {
        const response = data as ApiResponse<ScheduledTask[]>;
        setTasks(Array.isArray(response.tasks) ? response.tasks : []);
      } else {
        console.warn("Invalid response format:", data);
        setTasks([]);
      }
    } catch (err) {
      const msg = err instanceof Error ? err.message : "Failed to fetch tasks";
      console.error("Failed to fetch tasks:", msg);
      setError(msg);
    } finally {
      setLoading(false);
    }
  }, []);

  const fetchExecutions = useCallback(async (taskId: string) => {
    setExecutionsLoading(true);
    try {
      const res = await fetch(`/api/scheduler/tasks/${taskId}/executions?limit=20`);
      if (!res.ok) {
        throw new Error(`HTTP ${res.status}: ${res.statusText}`);
      }
      const data: unknown = await res.json();
      // Validate response is an array
      if (Array.isArray(data)) {
        setExecutions(data as TaskExecution[]);
      } else {
        console.warn("Invalid executions response:", data);
        setExecutions([]);
      }
    } catch (err) {
      const msg = err instanceof Error ? err.message : "Failed to fetch executions";
      console.error("Failed to fetch executions:", msg);
      setError(msg);
    } finally {
      setExecutionsLoading(false);
    }
  }, []);

  const handleSelectTask = useCallback(async (task: ScheduledTask) => {
    setSelectedTask(task);
    await fetchExecutions(task.id);
  }, [fetchExecutions]);

  const deleteTask = async (id: string) => {
    if (!confirm("Are you sure you want to delete this scheduled task?")) return;
    setDeletingTaskId(id);
    try {
      const res = await fetch(`/api/scheduler/tasks/${id}`, { method: "DELETE" });
      if (!res.ok) {
        throw new Error(`HTTP ${res.status}: ${res.statusText}`);
      }
      await fetchTasks();
      if (selectedTask?.id === id) {
        setSelectedTask(null);
        setExecutions([]);
      }
    } catch (err) {
      const msg = err instanceof Error ? err.message : "Failed to delete task";
      console.error("Failed to delete task:", msg);
      setError(msg);
    } finally {
      setDeletingTaskId(null);
    }
  };

  const runTaskNow = async (id: string) => {
    setRunningTaskId(id);
    setError(null);
    try {
      const res = await fetch(`/api/scheduler/tasks/${id}/run`, { method: "POST" });
      if (!res.ok) {
        throw new Error(`HTTP ${res.status}: ${res.statusText}`);
      }
      const data: unknown = await res.json();
      // Validate execution response
      if (data && typeof data === "object" && "id" in data) {
        const execution = data as TaskExecution;
        setExecutions((prev) => [execution, ...prev]);
      }
      await fetchTasks();
      if (selectedTask?.id === id) {
        await fetchExecutions(id);
      }
    } catch (err) {
      const msg = err instanceof Error ? err.message : "Failed to run task";
      console.error("Failed to run task:", msg);
      setError(msg);
    } finally {
      setRunningTaskId(null);
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
          <h2 className="text-lg font-semibold">Scheduled Tasks</h2>
          <p className="text-sm text-muted-foreground">
            Manage cron-based AI agent schedules
          </p>
        </div>
        <div className="flex items-center gap-2">
          <button
            onClick={fetchTasks}
            disabled={loading}
            className="text-xs px-2 py-1 rounded border border-border hover:bg-secondary/50 disabled:opacity-40"
          >
            Refresh
          </button>
          <button
            onClick={() => setShowAddModal(true)}
            className="text-xs px-3 py-1 rounded bg-primary text-primary-foreground hover:bg-primary/90"
          >
            + Add Task
          </button>
        </div>
      </div>

      {/* Error Display */}
      {error && (
        <div className="px-4 py-2 bg-destructive/10 border-b border-destructive/20">
          <div className="flex items-center justify-between">
            <span className="text-xs text-destructive">{error}</span>
            <button
              onClick={() => setError(null)}
              className="text-xs text-destructive hover:underline"
            >
              Dismiss
            </button>
          </div>
        </div>
      )}

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
                <p>No scheduled tasks</p>
                <p className="text-sm mt-2">Click "Add Task" to create one</p>
              </div>
            </div>
          ) : (
            <div className="divide-y divide-border">
              {tasks.map((task) => (
                <div
                  key={task.id}
                  onClick={() => handleSelectTask(task)}
                  className={`p-3 cursor-pointer hover:bg-secondary/50 transition-colors ${
                    selectedTask?.id === task.id ? "bg-secondary" : ""
                  }`}
                >
                  <div className="flex items-center justify-between">
                    <div className="flex items-center gap-2">
                      <span className="font-medium text-sm">{task.name}</span>
                      <span className={`px-1.5 py-0.5 rounded text-xs ${
                        task.enabled
                          ? "bg-green-100 text-green-800"
                          : "bg-gray-100 text-gray-800"
                      }`}>
                        {task.enabled ? "Active" : "Disabled"}
                      </span>
                    </div>
                    <div className="flex items-center gap-1">
                      <button
                        onClick={(e) => {
                          e.stopPropagation();
                          runTaskNow(task.id);
                        }}
                        disabled={runningTaskId === task.id}
                        className="text-xs text-muted-foreground hover:text-primary px-1 disabled:opacity-40"
                        title="Run now"
                      >
                        {runningTaskId === task.id ? "..." : "Run"}
                      </button>
                      <button
                        onClick={(e) => {
                          e.stopPropagation();
                          deleteTask(task.id);
                        }}
                        disabled={deletingTaskId === task.id}
                        className="text-xs text-muted-foreground hover:text-destructive px-1 disabled:opacity-40"
                      >
                        {deletingTaskId === task.id ? "..." : "Delete"}
                      </button>
                    </div>
                  </div>
                  <div className="mt-1 text-xs text-muted-foreground">
                    <span className="font-mono">{task.cron}</span>
                    {task.next_run && (
                      <span className="ml-2">
                        Next: {new Date(task.next_run).toLocaleString()}
                      </span>
                    )}
                  </div>
                  {task.last_run && (
                    <div className="mt-1 text-xs text-muted-foreground">
                      Last: {new Date(task.last_run).toLocaleString()}
                    </div>
                  )}
                </div>
              ))}
            </div>
          )}
        </div>

        {/* Task Detail / Execution History */}
        <div className="w-1/2 overflow-auto">
          {selectedTask ? (
            <TaskDetailView
              task={selectedTask}
              executions={executions}
              executionsLoading={executionsLoading}
              onRunNow={() => runTaskNow(selectedTask.id)}
              running={runningTaskId === selectedTask.id}
            />
          ) : (
            <div className="flex items-center justify-center h-full">
              <span className="text-muted-foreground text-sm">
                Select a task to view details and execution history
              </span>
            </div>
          )}
        </div>
      </div>

      {/* Stats Footer */}
      <div className="px-4 py-2 border-t border-border bg-card text-xs text-muted-foreground">
        <span>{tasks.length} scheduled tasks</span>
        <span className="mx-2">|</span>
        <span>{tasks.filter((t) => t.enabled).length} active</span>
        <span className="mx-2">|</span>
        <span>{tasks.filter((t) => !t.enabled).length} disabled</span>
      </div>

      {/* Add Task Modal */}
      {showAddModal && (
        <AddTaskModal
          onClose={() => setShowAddModal(false)}
          onCreated={fetchTasks}
        />
      )}
    </div>
  );
}

interface TaskDetailViewProps {
  task: ScheduledTask;
  executions: TaskExecution[];
  executionsLoading: boolean;
  onRunNow: () => void;
  running: boolean;
}

function TaskDetailView({
  task,
  executions,
  executionsLoading,
  onRunNow,
  running,
}: TaskDetailViewProps) {
  return (
    <div className="p-4 space-y-4">
      {/* Task Info */}
      <div className="space-y-2">
        <div className="flex items-center justify-between">
          <h3 className="font-medium">Task Details</h3>
          <div className="flex items-center gap-2">
            <button
              onClick={onRunNow}
              disabled={running}
              className="text-xs px-2 py-1 rounded border border-primary text-primary hover:bg-primary/10 disabled:opacity-40"
            >
              {running ? "Running..." : "Run Now"}
            </button>
          </div>
        </div>
        <div className="text-sm space-y-1">
          <p>
            <span className="text-muted-foreground">Name:</span>{" "}
            {task.name}
          </p>
          <p>
            <span className="text-muted-foreground">Cron:</span>{" "}
            <span className="font-mono">{task.cron}</span>
          </p>
          <p>
            <span className="text-muted-foreground">Status:</span>{" "}
            <span className={`px-1.5 py-0.5 rounded text-xs ${
              task.enabled
                ? "bg-green-100 text-green-800"
                : "bg-gray-100 text-gray-800"
            }`}>
              {task.enabled ? "Active" : "Disabled"}
            </span>
          </p>
          {task.next_run && (
            <p>
              <span className="text-muted-foreground">Next Run:</span>{" "}
              {new Date(task.next_run).toLocaleString()}
            </p>
          )}
          {task.last_run && (
            <p>
              <span className="text-muted-foreground">Last Run:</span>{" "}
              {new Date(task.last_run).toLocaleString()}
            </p>
          )}
        </div>
      </div>

      {/* Agent Config */}
      <div className="space-y-2">
        <h4 className="text-sm font-medium">Agent Configuration</h4>
        <div className="text-xs bg-secondary p-3 rounded-lg space-y-1">
          {task.agent_config.model && (
            <p>
              <span className="text-muted-foreground">Model:</span>{" "}
              {task.agent_config.model}
            </p>
          )}
          {task.agent_config.max_rounds && (
            <p>
              <span className="text-muted-foreground">Max Rounds:</span>{" "}
              {task.agent_config.max_rounds}
            </p>
          )}
          {task.agent_config.timeout_secs && (
            <p>
              <span className="text-muted-foreground">Timeout:</span>{" "}
              {task.agent_config.timeout_secs}s
            </p>
          )}
          {task.agent_config.system_prompt && (
            <p>
              <span className="text-muted-foreground">System Prompt:</span>{" "}
              <span className="block mt-1 whitespace-pre-wrap">
                {task.agent_config.system_prompt.slice(0, 200)}
                {task.agent_config.system_prompt.length > 200 && "..."}
              </span>
            </p>
          )}
          {task.agent_config.input && (
            <p>
              <span className="text-muted-foreground">Input Prompt:</span>{" "}
              <span className="block mt-1 whitespace-pre-wrap">
                {task.agent_config.input.slice(0, 200)}
                {task.agent_config.input.length > 200 && "..."}
              </span>
            </p>
          )}
        </div>
      </div>

      {/* Execution History */}
      <div className="space-y-2">
        <h4 className="text-sm font-medium">Execution History</h4>
        {executionsLoading ? (
          <div className="flex items-center justify-center py-4">
            <span className="text-muted-foreground text-xs">Loading...</span>
          </div>
        ) : executions.length === 0 ? (
          <div className="text-center py-4">
            <span className="text-muted-foreground text-xs">No executions yet</span>
          </div>
        ) : (
          <div className="space-y-2">
            {executions.map((exec) => (
              <ExecutionItem key={exec.id} execution={exec} />
            ))}
          </div>
        )}
      </div>
    </div>
  );
}

function ExecutionItem({ execution }: { execution: TaskExecution }) {
  const statusStyles: Record<string, string> = {
    Success: "bg-green-100 text-green-800",
    Running: "bg-blue-100 text-blue-800",
    Failed: "bg-red-100 text-red-800",
  };
  const statusStyle = statusStyles[execution.status] || "bg-gray-100 text-gray-800";

  return (
    <div className="p-3 bg-secondary rounded-lg text-xs">
      <div className="flex items-center justify-between mb-1">
        <span className="font-mono text-muted-foreground">
          {execution.id.slice(0, 8)}...
        </span>
        <span className={`px-1.5 py-0.5 rounded ${statusStyle}`}>
          {execution.status}
        </span>
      </div>
      <p className="text-muted-foreground">
        {new Date(execution.started_at).toLocaleString()}
        {execution.finished_at &&
          ` - ${new Date(execution.finished_at).toLocaleString()}`}
      </p>
      {execution.result && (
        <pre className="mt-2 text-xs whitespace-pre-wrap bg-card p-2 rounded">
          {execution.result.slice(0, 500)}
          {execution.result.length > 500 && "..."}
        </pre>
      )}
      {execution.error && (
        <pre className="mt-2 text-xs whitespace-pre-wrap text-destructive bg-destructive/10 p-2 rounded">
          {execution.error.slice(0, 500)}
          {execution.error.length > 500 && "..."}
        </pre>
      )}
    </div>
  );
}

interface AddTaskModalProps {
  onClose: () => void;
  onCreated: () => void;
}

function AddTaskModal({ onClose, onCreated }: AddTaskModalProps) {
  const [form, setForm] = useState<CreateTaskForm>({
    name: "",
    cron: DEFAULT_CRON,
    system_prompt: "",
    input: "",
    max_rounds: DEFAULT_MAX_ROUNDS,
    timeout_secs: DEFAULT_TIMEOUT_SECS,
    model: DEFAULT_MODEL,
  });
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [cronError, setCronError] = useState<string | null>(null);

  const handleCronChange = (value: string) => {
    setForm({ ...form, cron: value });
    // Validate on change
    if (value.trim()) {
      if (!isValidCron(value)) {
        setCronError("Invalid cron format. Use: minute hour day month weekday");
      } else {
        setCronError(null);
      }
    } else {
      setCronError(null);
    }
  };

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!form.name.trim()) {
      setError("Name is required");
      return;
    }
    if (!form.cron.trim()) {
      setError("Cron expression is required");
      return;
    }
    if (!isValidCron(form.cron)) {
      setCronError("Invalid cron format. Use: minute hour day month weekday");
      setError("Please fix cron expression errors");
      return;
    }
    if (!form.input.trim()) {
      setError("Input prompt is required");
      return;
    }

    setSubmitting(true);
    setError(null);
    try {
      const res = await fetch("/api/scheduler/tasks", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          name: form.name.trim(),
          cron: form.cron.trim(),
          agent_config: {
            system_prompt: form.system_prompt.trim() || undefined,
            input: form.input.trim(),
            max_rounds: form.max_rounds,
            timeout_secs: form.timeout_secs,
            model: form.model || undefined,
          },
          enabled: true,
        }),
      });
      if (!res.ok) {
        const errText = await res.text();
        throw new Error(`HTTP ${res.status}: ${errText}`);
      }
      onCreated();
      onClose();
    } catch (err) {
      const msg = err instanceof Error ? err.message : "Failed to create task";
      console.error("Failed to create task:", msg);
      setError(msg);
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <div className="fixed inset-0 bg-black/50 flex items-center justify-center z-50">
      <div className="bg-background rounded-lg shadow-xl w-full max-w-lg max-h-[90vh] overflow-auto">
        <div className="px-4 py-3 border-b border-border flex items-center justify-between">
          <h3 className="font-medium">Add Scheduled Task</h3>
          <button
            onClick={onClose}
            className="text-muted-foreground hover:text-foreground"
          >
            &times;
          </button>
        </div>
        <form onSubmit={handleSubmit} className="p-4 space-y-4">
          {/* Name */}
          <div>
            <label className="block text-sm font-medium mb-1">
              Task Name <span className="text-destructive">*</span>
            </label>
            <input
              type="text"
              value={form.name}
              onChange={(e) => setForm({ ...form, name: e.target.value })}
              placeholder="e.g., Daily Report"
              className="w-full px-3 py-2 text-sm bg-secondary border border-border rounded-md focus:outline-none focus:ring-2 focus:ring-primary"
            />
          </div>

          {/* Cron */}
          <div>
            <label className="block text-sm font-medium mb-1">
              Cron Expression <span className="text-destructive">*</span>
            </label>
            <input
              type="text"
              value={form.cron}
              onChange={(e) => handleCronChange(e.target.value)}
              placeholder="e.g., 0 * * * *"
              className={`w-full px-3 py-2 text-sm bg-secondary border rounded-md focus:outline-none focus:ring-2 focus:ring-primary font-mono ${
                cronError ? "border-destructive" : "border-border"
              }`}
            />
            {cronError && (
              <p className="mt-1 text-xs text-destructive">{cronError}</p>
            )}
            <div className="mt-2 flex flex-wrap gap-1">
              {CRON_EXAMPLES.map((ex) => (
                <button
                  key={ex.value}
                  type="button"
                  onClick={() => handleCronChange(ex.value)}
                  className="text-xs px-2 py-0.5 rounded bg-secondary hover:bg-secondary/80 text-muted-foreground"
                >
                  {ex.label}
                </button>
              ))}
            </div>
          </div>

          {/* Input Prompt */}
          <div>
            <label className="block text-sm font-medium mb-1">
              Input Prompt <span className="text-destructive">*</span>
            </label>
            <textarea
              value={form.input}
              onChange={(e) => setForm({ ...form, input: e.target.value })}
              placeholder="What should the agent do?"
              rows={3}
              className="w-full px-3 py-2 text-sm bg-secondary border border-border rounded-md focus:outline-none focus:ring-2 focus:ring-primary resize-none"
            />
          </div>

          {/* System Prompt (Optional) */}
          <div>
            <label className="block text-sm font-medium mb-1">
              System Prompt (Optional)
            </label>
            <textarea
              value={form.system_prompt}
              onChange={(e) => setForm({ ...form, system_prompt: e.target.value })}
              placeholder="Optional system instructions for the agent"
              rows={2}
              className="w-full px-3 py-2 text-sm bg-secondary border border-border rounded-md focus:outline-none focus:ring-2 focus:ring-primary resize-none"
            />
          </div>

          {/* Advanced Options */}
          <div className="grid grid-cols-3 gap-3">
            <div>
              <label className="block text-xs text-muted-foreground mb-1">
                Max Rounds
              </label>
              <input
                type="number"
                value={form.max_rounds}
                onChange={(e) => setForm({ ...form, max_rounds: parseInt(e.target.value) || DEFAULT_MAX_ROUNDS })}
                min={1}
                max={500}
                className="w-full px-2 py-1.5 text-sm bg-secondary border border-border rounded-md focus:outline-none focus:ring-2 focus:ring-primary"
              />
            </div>
            <div>
              <label className="block text-xs text-muted-foreground mb-1">
                Timeout (sec)
              </label>
              <input
                type="number"
                value={form.timeout_secs}
                onChange={(e) => setForm({ ...form, timeout_secs: parseInt(e.target.value) || DEFAULT_TIMEOUT_SECS })}
                min={10}
                max={3600}
                className="w-full px-2 py-1.5 text-sm bg-secondary border border-border rounded-md focus:outline-none focus:ring-2 focus:ring-primary"
              />
            </div>
            <div>
              <label className="block text-xs text-muted-foreground mb-1">
                Model
              </label>
              <select
                value={form.model}
                onChange={(e) => setForm({ ...form, model: e.target.value })}
                className="w-full px-2 py-1.5 text-sm bg-secondary border border-border rounded-md focus:outline-none focus:ring-2 focus:ring-primary"
              >
                <option value="claude-3-5-sonnet-20241022">Sonnet 3.5</option>
                <option value="claude-3-opus-20240229">Opus 3</option>
                <option value="claude-3-5-haiku-20241022">Haiku 3.5</option>
                <option value="claude-3-haiku-20240307">Haiku 3</option>
              </select>
            </div>
          </div>

          {/* Error */}
          {error && (
            <div className="text-xs text-destructive bg-destructive/10 p-2 rounded">
              {error}
            </div>
          )}

          {/* Actions */}
          <div className="flex justify-end gap-2 pt-2">
            <button
              type="button"
              onClick={onClose}
              className="px-3 py-2 text-sm border border-border rounded-md hover:bg-secondary"
            >
              Cancel
            </button>
            <button
              type="submit"
              disabled={submitting}
              className="px-3 py-2 text-sm bg-primary text-primary-foreground rounded-md hover:bg-primary/90 disabled:opacity-40"
            >
              {submitting ? "Creating..." : "Create Task"}
            </button>
          </div>
        </form>
      </div>
    </div>
  );
}
