//! JSON file-based storage

use crate::pipeline::Pipeline;
use crate::queue::Queue;
use crate::task::{Task, TaskId, TaskState};
use crate::workspace::Workspace;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::time::Duration;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum StorageError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("not found: {kind}/{id}")]
    NotFound { kind: String, id: String },
}

/// JSON file-based storage
#[derive(Clone)]
pub struct JsonStore {
    base_path: PathBuf,
}

impl JsonStore {
    /// Open a store at the given path
    pub fn open(base_path: impl Into<PathBuf>) -> Result<Self, StorageError> {
        let base_path = base_path.into();
        fs::create_dir_all(&base_path)?;
        Ok(Self { base_path })
    }

    /// Open a temporary store for testing
    pub fn open_temp() -> Result<Self, StorageError> {
        let temp_dir = std::env::temp_dir().join(format!("oj-test-{}", uuid::Uuid::new_v4()));
        Self::open(temp_dir)
    }

    /// Save a value to storage
    pub fn save<T: Serialize>(&self, kind: &str, id: &str, data: &T) -> Result<(), StorageError> {
        let path = self.path_for(kind, id);
        fs::create_dir_all(path.parent().unwrap())?;
        let json = serde_json::to_string_pretty(data)?;
        fs::write(&path, json)?;
        Ok(())
    }

    /// Load a value from storage
    pub fn load<T: DeserializeOwned>(&self, kind: &str, id: &str) -> Result<T, StorageError> {
        let path = self.path_for(kind, id);
        if !path.exists() {
            return Err(StorageError::NotFound {
                kind: kind.to_string(),
                id: id.to_string(),
            });
        }
        let json = fs::read_to_string(&path)?;
        Ok(serde_json::from_str(&json)?)
    }

    /// Delete a value from storage
    pub fn delete(&self, kind: &str, id: &str) -> Result<(), StorageError> {
        let path = self.path_for(kind, id);
        if path.exists() {
            fs::remove_file(&path)?;
        }
        Ok(())
    }

    /// List all IDs of a given kind
    pub fn list(&self, kind: &str) -> Result<Vec<String>, StorageError> {
        let dir = self.base_path.join(kind);
        if !dir.exists() {
            return Ok(Vec::new());
        }

        let mut ids = Vec::new();
        for entry in fs::read_dir(&dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().map(|e| e == "json").unwrap_or(false) {
                if let Some(stem) = path.file_stem() {
                    ids.push(stem.to_string_lossy().to_string());
                }
            }
        }
        Ok(ids)
    }

    /// Check if a value exists
    pub fn exists(&self, kind: &str, id: &str) -> bool {
        self.path_for(kind, id).exists()
    }

    fn path_for(&self, kind: &str, id: &str) -> PathBuf {
        self.base_path.join(kind).join(format!("{}.json", id))
    }

    // Convenience methods for common types

    /// Save a pipeline
    pub fn save_pipeline(&self, pipeline: &Pipeline) -> Result<(), StorageError> {
        self.save("pipelines", &pipeline.id.0, pipeline)
    }

    /// Load a pipeline
    pub fn load_pipeline(&self, id: &str) -> Result<Pipeline, StorageError> {
        self.load("pipelines", id)
    }

    /// List all pipelines
    pub fn list_pipelines(&self) -> Result<Vec<String>, StorageError> {
        self.list("pipelines")
    }

    /// Save a workspace
    pub fn save_workspace(&self, workspace: &Workspace) -> Result<(), StorageError> {
        self.save("workspaces", &workspace.id.0, workspace)
    }

    /// Load a workspace
    pub fn load_workspace(&self, id: &str) -> Result<Workspace, StorageError> {
        self.load("workspaces", id)
    }

    /// List all workspaces
    pub fn list_workspaces(&self) -> Result<Vec<String>, StorageError> {
        self.list("workspaces")
    }

    /// Save a queue
    pub fn save_queue(&self, name: &str, queue: &Queue) -> Result<(), StorageError> {
        self.save("queues", name, queue)
    }

    /// Load a queue
    pub fn load_queue(&self, name: &str) -> Result<Queue, StorageError> {
        self.load("queues", name)
    }

    /// Save a task
    pub fn save_task(&self, task: &Task) -> Result<(), StorageError> {
        let storable = StorableTask::from_task(task);
        self.save("tasks", &task.id.0, &storable)
    }

    /// Load a task
    pub fn load_task(&self, id: &str) -> Result<Task, StorageError> {
        let storable: StorableTask = self.load("tasks", id)?;
        Ok(storable.to_task())
    }

    /// List all tasks
    pub fn list_tasks(&self) -> Result<Vec<String>, StorageError> {
        self.list("tasks")
    }
}

/// Serializable version of Task (Instant fields are stored as elapsed microseconds from a reference point)
#[derive(Debug, Clone, Serialize, Deserialize)]
struct StorableTask {
    id: String,
    pipeline_id: String,
    phase: String,
    state: StorableTaskState,
    session_id: Option<String>,
    heartbeat_interval_secs: u64,
    stuck_threshold_secs: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
enum StorableTaskState {
    Pending,
    Running,
    Stuck { nudge_count: u32 },
    Done { output: Option<String> },
    Failed { reason: String },
}

impl StorableTask {
    fn from_task(task: &Task) -> Self {
        let state = match &task.state {
            TaskState::Pending => StorableTaskState::Pending,
            TaskState::Running => StorableTaskState::Running,
            TaskState::Stuck { nudge_count, .. } => StorableTaskState::Stuck {
                nudge_count: *nudge_count,
            },
            TaskState::Done { output } => StorableTaskState::Done {
                output: output.clone(),
            },
            TaskState::Failed { reason } => StorableTaskState::Failed {
                reason: reason.clone(),
            },
        };

        StorableTask {
            id: task.id.0.clone(),
            pipeline_id: task.pipeline_id.0.clone(),
            phase: task.phase.clone(),
            state,
            session_id: task.session_id.as_ref().map(|s| s.0.clone()),
            heartbeat_interval_secs: task.heartbeat_interval.as_secs(),
            stuck_threshold_secs: task.stuck_threshold.as_secs(),
        }
    }

    fn to_task(&self) -> Task {
        use crate::clock::{Clock, SystemClock};
        use crate::pipeline::PipelineId;
        use crate::session::SessionId;

        let clock = SystemClock;
        let now = clock.now();

        let state = match &self.state {
            StorableTaskState::Pending => TaskState::Pending,
            StorableTaskState::Running => TaskState::Running,
            StorableTaskState::Stuck { nudge_count } => TaskState::Stuck {
                since: now,
                nudge_count: *nudge_count,
            },
            StorableTaskState::Done { output } => TaskState::Done {
                output: output.clone(),
            },
            StorableTaskState::Failed { reason } => TaskState::Failed {
                reason: reason.clone(),
            },
        };

        Task {
            id: TaskId(self.id.clone()),
            pipeline_id: PipelineId(self.pipeline_id.clone()),
            phase: self.phase.clone(),
            state,
            session_id: self.session_id.as_ref().map(|s| SessionId(s.clone())),
            heartbeat_interval: Duration::from_secs(self.heartbeat_interval_secs),
            stuck_threshold: Duration::from_secs(self.stuck_threshold_secs),
            last_heartbeat: None,
            created_at: now,
            started_at: None,
            completed_at: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn store_save_and_load() {
        let store = JsonStore::open_temp().unwrap();

        #[derive(Debug, Clone, PartialEq, Eq, Serialize, serde::Deserialize)]
        struct TestData {
            name: String,
            value: i32,
        }

        let data = TestData {
            name: "test".to_string(),
            value: 42,
        };

        store.save("test_kind", "test_id", &data).unwrap();
        let loaded: TestData = store.load("test_kind", "test_id").unwrap();

        assert_eq!(data, loaded);
    }

    #[test]
    fn store_load_not_found() {
        let store = JsonStore::open_temp().unwrap();
        let result: Result<String, _> = store.load("nonexistent", "id");
        assert!(matches!(result, Err(StorageError::NotFound { .. })));
    }

    #[test]
    fn store_list_returns_ids() {
        let store = JsonStore::open_temp().unwrap();

        store.save("items", "a", &"data-a").unwrap();
        store.save("items", "b", &"data-b").unwrap();
        store.save("items", "c", &"data-c").unwrap();

        let mut ids = store.list("items").unwrap();
        ids.sort();

        assert_eq!(ids, vec!["a", "b", "c"]);
    }

    #[test]
    fn store_delete_removes_file() {
        let store = JsonStore::open_temp().unwrap();

        store.save("items", "to_delete", &"data").unwrap();
        assert!(store.exists("items", "to_delete"));

        store.delete("items", "to_delete").unwrap();
        assert!(!store.exists("items", "to_delete"));
    }

    #[test]
    fn store_pipeline_convenience_methods() {
        let store = JsonStore::open_temp().unwrap();

        let pipeline = Pipeline::new_build("p-1", "test", "Test prompt");
        store.save_pipeline(&pipeline).unwrap();

        let loaded = store.load_pipeline("p-1").unwrap();
        assert_eq!(loaded.name, "test");

        let ids = store.list_pipelines().unwrap();
        assert_eq!(ids, vec!["p-1"]);
    }

    #[test]
    fn store_workspace_convenience_methods() {
        let store = JsonStore::open_temp().unwrap();

        let workspace = Workspace::new("ws-1", "test", PathBuf::from("/tmp/test"), "feature-x");
        store.save_workspace(&workspace).unwrap();

        let loaded = store.load_workspace("ws-1").unwrap();
        assert_eq!(loaded.name, "test");
    }

    #[test]
    fn store_queue_convenience_methods() {
        let store = JsonStore::open_temp().unwrap();

        let queue = Queue::new("test-queue");
        store.save_queue("test-queue", &queue).unwrap();

        let loaded = store.load_queue("test-queue").unwrap();
        assert_eq!(loaded.name, "test-queue");
    }
}
