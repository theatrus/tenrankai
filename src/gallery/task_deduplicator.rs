use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{Notify, RwLock};

/// A helper struct for deduplicating concurrent tasks
/// Ensures only one instance of a task with a given key runs at a time
#[derive(Clone)]
pub struct TaskDeduplicator {
    pending_tasks: Arc<RwLock<HashMap<String, Arc<Notify>>>>,
}

impl TaskDeduplicator {
    /// Create a new TaskDeduplicator
    pub fn new() -> Self {
        Self {
            pending_tasks: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Check if a task should execute or wait
    /// Returns a TaskHandle that must be used to complete or wait for the task
    pub async fn should_execute(&self, key: String) -> TaskHandle {
        let mut tasks = self.pending_tasks.write().await;

        if let Some(notify) = tasks.get(&key) {
            // Another task is already running for this key
            TaskHandle {
                key: key.clone(),
                deduplicator: Arc::new(self.clone()),
                is_executor: false,
                notify: Some(notify.clone()),
            }
        } else {
            // We're the first/only task for this key
            let notify = Arc::new(Notify::new());
            tasks.insert(key.clone(), notify.clone());

            TaskHandle {
                key,
                deduplicator: Arc::new(self.clone()),
                is_executor: true,
                notify: Some(notify),
            }
        }
    }

    /// Get current number of pending tasks (for monitoring/testing)
    #[allow(dead_code)]
    pub async fn pending_count(&self) -> usize {
        self.pending_tasks.read().await.len()
    }
}

impl Default for TaskDeduplicator {
    fn default() -> Self {
        Self::new()
    }
}

/// Handle for a deduplicated task
pub struct TaskHandle {
    key: String,
    deduplicator: Arc<TaskDeduplicator>,
    is_executor: bool,
    notify: Option<Arc<Notify>>,
}

impl TaskHandle {
    /// Check if this handle should execute the task
    pub fn is_executor(&self) -> bool {
        self.is_executor
    }

    /// Wait for the task to complete (for non-executors)
    pub async fn wait(&self) {
        if !self.is_executor
            && let Some(notify) = &self.notify
        {
            notify.notified().await;
        }
    }

    /// Mark the task as complete and notify waiters (for executors)
    /// This consumes the handle to ensure it can only be called once
    pub async fn complete(self) {
        if self.is_executor {
            // Remove the task from pending and notify all waiters
            let mut tasks = self.deduplicator.pending_tasks.write().await;
            if let Some(notify) = tasks.remove(&self.key) {
                // Notify all tasks waiting on this key
                notify.notify_waiters();
            }
        }
    }
}

/// RAII guard that ensures cleanup even if the task panics
impl Drop for TaskHandle {
    fn drop(&mut self) {
        if self.is_executor {
            // If we're dropping an executor handle without calling complete(),
            // we need to clean up to prevent memory leaks
            let deduplicator = self.deduplicator.clone();
            let key = self.key.clone();

            // Spawn a task to clean up asynchronously
            tokio::spawn(async move {
                let mut tasks = deduplicator.pending_tasks.write().await;
                if let Some(notify) = tasks.remove(&key) {
                    // Notify waiters that the task failed
                    notify.notify_waiters();
                }
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tokio::time::sleep;

    #[tokio::test]
    async fn test_single_task_execution() {
        let deduplicator = TaskDeduplicator::new();

        let handle = deduplicator.should_execute("test_key".to_string()).await;
        assert!(handle.is_executor());

        // Should have one pending task
        assert_eq!(deduplicator.pending_count().await, 1);

        handle.complete().await;

        // Should be cleaned up
        assert_eq!(deduplicator.pending_count().await, 0);
    }

    #[tokio::test]
    async fn test_concurrent_task_deduplication() {
        let deduplicator = Arc::new(TaskDeduplicator::new());
        let dedup_clone = deduplicator.clone();

        // First task starts
        let handle1 = deduplicator.should_execute("test_key".to_string()).await;
        assert!(handle1.is_executor());

        // Second task should wait
        let handle2_future = tokio::spawn(async move {
            let handle2 = dedup_clone.should_execute("test_key".to_string()).await;
            assert!(!handle2.is_executor());
            handle2.wait().await;
            true
        });

        // Give the second task time to start waiting
        sleep(Duration::from_millis(50)).await;

        // Complete the first task
        handle1.complete().await;

        // Second task should complete
        let result = tokio::time::timeout(Duration::from_secs(1), handle2_future)
            .await
            .expect("Timeout waiting for task")
            .expect("Task panicked");

        assert!(result);
    }

    #[tokio::test]
    async fn test_different_keys_execute_concurrently() {
        let deduplicator = TaskDeduplicator::new();

        let handle1 = deduplicator.should_execute("key1".to_string()).await;
        let handle2 = deduplicator.should_execute("key2".to_string()).await;

        assert!(handle1.is_executor());
        assert!(handle2.is_executor());

        assert_eq!(deduplicator.pending_count().await, 2);

        handle1.complete().await;
        handle2.complete().await;

        assert_eq!(deduplicator.pending_count().await, 0);
    }

    #[tokio::test]
    async fn test_cleanup_on_drop() {
        let deduplicator = TaskDeduplicator::new();

        {
            let handle = deduplicator.should_execute("test_key".to_string()).await;
            assert!(handle.is_executor());
            // Handle dropped without complete()
        }

        // Give the cleanup task time to run
        sleep(Duration::from_millis(100)).await;

        // Should be cleaned up
        assert_eq!(deduplicator.pending_count().await, 0);
    }

    #[tokio::test]
    async fn test_multiple_waiters() {
        let deduplicator = Arc::new(TaskDeduplicator::new());

        // Start the executor
        let executor_handle = deduplicator.should_execute("test_key".to_string()).await;
        assert!(executor_handle.is_executor());

        // Give a small delay to ensure the executor is registered
        sleep(Duration::from_millis(10)).await;

        // Start multiple waiters
        let mut waiter_handles = Vec::new();
        for _ in 0..3 {
            let dedup_clone = deduplicator.clone();
            let handle = tokio::spawn(async move {
                let handle = dedup_clone.should_execute("test_key".to_string()).await;
                assert!(!handle.is_executor());
                handle.wait().await;
            });
            waiter_handles.push(handle);
        }

        // Give waiters time to start waiting
        sleep(Duration::from_millis(50)).await;

        // Complete the executor
        executor_handle.complete().await;

        // All waiters should complete
        for handle in waiter_handles {
            tokio::time::timeout(Duration::from_secs(1), handle)
                .await
                .expect("Timeout waiting for waiter")
                .expect("Waiter panicked");
        }

        assert_eq!(deduplicator.pending_count().await, 0);
    }
}
