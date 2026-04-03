//! TaskTracker — in-memory task management for multi-agent coordination.
//!
//! Provides a DashMap-backed registry of tracked tasks that agents can
//! create, update, and query via LLM tools (task_create / task_update / task_list).

use std::fmt;
use std::sync::atomic::{AtomicU32, Ordering};

use chrono::Utc;
use dashmap::DashMap;
use serde::{Deserialize, Serialize};

/// Status of a tracked task.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Pending,
    InProgress,
    Completed,
    Blocked,
    Cancelled,
}

impl fmt::Display for TaskStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Pending => write!(f, "pending"),
            Self::InProgress => write!(f, "in_progress"),
            Self::Completed => write!(f, "completed"),
            Self::Blocked => write!(f, "blocked"),
            Self::Cancelled => write!(f, "cancelled"),
        }
    }
}

impl TaskStatus {
    pub fn from_str_opt(s: &str) -> Option<Self> {
        match s {
            "pending" => Some(Self::Pending),
            "in_progress" => Some(Self::InProgress),
            "completed" => Some(Self::Completed),
            "blocked" => Some(Self::Blocked),
            "cancelled" => Some(Self::Cancelled),
            _ => None,
        }
    }
}

/// A task tracked by the multi-agent system.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackedTask {
    pub id: String,
    pub subject: String,
    pub description: String,
    pub status: TaskStatus,
    /// Agent name or session_id that owns this task.
    pub owner: Option<String>,
    /// Team this task belongs to (if any).
    pub team: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// In-memory task tracker with atomic ID generation.
pub struct TaskTracker {
    tasks: DashMap<String, TrackedTask>,
    next_id: AtomicU32,
}

impl TaskTracker {
    pub fn new() -> Self {
        Self {
            tasks: DashMap::new(),
            next_id: AtomicU32::new(1),
        }
    }

    /// Create a new task and return it.
    pub fn create(
        &self,
        subject: &str,
        description: &str,
        team: Option<&str>,
    ) -> TrackedTask {
        let id_num = self.next_id.fetch_add(1, Ordering::Relaxed);
        let id = format!("task-{id_num}");
        let now = Utc::now().to_rfc3339();
        let task = TrackedTask {
            id: id.clone(),
            subject: subject.to_string(),
            description: description.to_string(),
            status: TaskStatus::Pending,
            owner: None,
            team: team.map(|s| s.to_string()),
            created_at: now.clone(),
            updated_at: now,
        };
        self.tasks.insert(id, task.clone());
        task
    }

    /// Update a task's status and/or owner. Returns the updated task.
    pub fn update(
        &self,
        id: &str,
        status: Option<TaskStatus>,
        owner: Option<&str>,
    ) -> Result<TrackedTask, String> {
        let mut entry = self
            .tasks
            .get_mut(id)
            .ok_or_else(|| format!("Task '{}' not found", id))?;
        if let Some(s) = status {
            entry.status = s;
        }
        if let Some(o) = owner {
            entry.owner = Some(o.to_string());
        }
        entry.updated_at = Utc::now().to_rfc3339();
        Ok(entry.clone())
    }

    /// List tasks, optionally filtered by team.
    pub fn list(&self, team: Option<&str>) -> Vec<TrackedTask> {
        self.tasks
            .iter()
            .filter(|entry| {
                if let Some(t) = team {
                    entry.team.as_deref() == Some(t)
                } else {
                    true
                }
            })
            .map(|entry| entry.value().clone())
            .collect()
    }

    /// Get a single task by ID.
    pub fn get(&self, id: &str) -> Option<TrackedTask> {
        self.tasks.get(id).map(|e| e.value().clone())
    }

    /// Cancel a task. Only pending/in_progress/blocked tasks can be cancelled.
    pub fn cancel(&self, id: &str) -> Result<TrackedTask, String> {
        let mut entry = self
            .tasks
            .get_mut(id)
            .ok_or_else(|| format!("Task '{}' not found", id))?;
        match entry.status {
            TaskStatus::Completed => {
                return Err(format!("Task '{}' is already completed", id));
            }
            TaskStatus::Cancelled => {
                return Err(format!("Task '{}' is already cancelled", id));
            }
            _ => {}
        }
        entry.status = TaskStatus::Cancelled;
        entry.updated_at = Utc::now().to_rfc3339();
        Ok(entry.clone())
    }

    /// Count tasks by status.
    pub fn count_by_status(&self) -> (usize, usize, usize, usize) {
        let mut pending = 0;
        let mut in_progress = 0;
        let mut completed = 0;
        let mut blocked = 0;
        for entry in self.tasks.iter() {
            match entry.status {
                TaskStatus::Pending => pending += 1,
                TaskStatus::InProgress => in_progress += 1,
                TaskStatus::Completed => completed += 1,
                TaskStatus::Blocked => blocked += 1,
                TaskStatus::Cancelled => {} // not counted in 4-tuple
            }
        }
        (pending, in_progress, completed, blocked)
    }
}

impl Default for TaskTracker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_task() {
        let tracker = TaskTracker::new();
        let task = tracker.create("Fix bug", "Fix the login bug", None);
        assert_eq!(task.id, "task-1");
        assert_eq!(task.status, TaskStatus::Pending);
        assert!(task.owner.is_none());
    }

    #[test]
    fn test_auto_increment_ids() {
        let tracker = TaskTracker::new();
        let t1 = tracker.create("Task 1", "Desc 1", None);
        let t2 = tracker.create("Task 2", "Desc 2", None);
        assert_eq!(t1.id, "task-1");
        assert_eq!(t2.id, "task-2");
    }

    #[test]
    fn test_update_status_and_owner() {
        let tracker = TaskTracker::new();
        let task = tracker.create("Test", "Test desc", None);
        let updated = tracker
            .update(&task.id, Some(TaskStatus::InProgress), Some("agent-1"))
            .unwrap();
        assert_eq!(updated.status, TaskStatus::InProgress);
        assert_eq!(updated.owner.as_deref(), Some("agent-1"));
    }

    #[test]
    fn test_update_not_found() {
        let tracker = TaskTracker::new();
        assert!(tracker.update("task-999", None, None).is_err());
    }

    #[test]
    fn test_list_all() {
        let tracker = TaskTracker::new();
        tracker.create("A", "a", None);
        tracker.create("B", "b", Some("team-x"));
        assert_eq!(tracker.list(None).len(), 2);
    }

    #[test]
    fn test_list_by_team() {
        let tracker = TaskTracker::new();
        tracker.create("A", "a", None);
        tracker.create("B", "b", Some("team-x"));
        tracker.create("C", "c", Some("team-x"));
        assert_eq!(tracker.list(Some("team-x")).len(), 2);
        assert_eq!(tracker.list(Some("team-y")).len(), 0);
    }

    #[test]
    fn test_get_by_id() {
        let tracker = TaskTracker::new();
        let task = tracker.create("X", "x", None);
        assert!(tracker.get(&task.id).is_some());
        assert!(tracker.get("task-999").is_none());
    }

    #[test]
    fn test_count_by_status() {
        let tracker = TaskTracker::new();
        tracker.create("A", "a", None);
        tracker.create("B", "b", None);
        tracker.update("task-1", Some(TaskStatus::InProgress), None).unwrap();
        tracker.update("task-2", Some(TaskStatus::Completed), None).unwrap();
        tracker.create("C", "c", None); // pending
        let (p, ip, c, b) = tracker.count_by_status();
        assert_eq!(p, 1);
        assert_eq!(ip, 1);
        assert_eq!(c, 1);
        assert_eq!(b, 0);
    }
}
