//! Generic persistent cache with dirty tracking
//!
//! Provides a thread-safe HashMap-backed cache that persists to JSON storage.

use bytes::Bytes;
use serde::{Serialize, de::DeserializeOwned};
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::SystemTime;
use tokio::sync::RwLock;

use crate::storage::{DynStorage, StorageError};

/// Error type for cache operations
#[derive(Debug, thiserror::Error)]
pub enum CacheError {
    #[error("Storage error: {0}")]
    Storage(#[from] StorageError),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}

/// Generic JSON-backed cache with dirty tracking and staleness detection.
///
/// This cache provides:
/// - Thread-safe read/write access via RwLock
/// - Automatic dirty flag tracking on mutations
/// - JSON serialization to/from storage
/// - Lazy persistence (only saves when dirty)
/// - Staleness detection to reload from storage if newer
pub struct PersistentCache<V> {
    data: Arc<RwLock<HashMap<String, V>>>,
    dirty: Arc<AtomicBool>,
    filename: String,
    /// Timestamp of when cache was last loaded or saved
    last_sync: Arc<RwLock<Option<SystemTime>>>,
}

impl<V> PersistentCache<V>
where
    V: Clone + Serialize + DeserializeOwned + Send + Sync,
{
    /// Create a new cache that will persist to the given filename.
    pub fn new(filename: impl Into<String>) -> Self {
        Self {
            data: Arc::new(RwLock::new(HashMap::new())),
            dirty: Arc::new(AtomicBool::new(false)),
            filename: filename.into(),
            last_sync: Arc::new(RwLock::new(None)),
        }
    }

    /// Load cache contents from storage.
    ///
    /// Returns Ok(()) if the file doesn't exist (starts with empty cache).
    pub async fn load(&self, storage: &DynStorage) -> Result<(), CacheError> {
        match storage.read_to_string(&self.filename).await {
            Ok(json) => {
                let data: HashMap<String, V> = serde_json::from_str(&json)?;
                *self.data.write().await = data;
                self.dirty.store(false, Ordering::Relaxed);
                // Track when we loaded (use storage mtime if available, else now)
                let mtime = storage
                    .metadata(&self.filename)
                    .await
                    .ok()
                    .and_then(|m| m.last_modified)
                    .unwrap_or_else(SystemTime::now);
                *self.last_sync.write().await = Some(mtime);
                Ok(())
            }
            Err(StorageError::NotFound(_)) => Ok(()), // Empty cache is fine
            Err(e) => Err(e.into()),
        }
    }

    /// Save cache to storage if dirty.
    ///
    /// Returns Ok(true) if cache was saved, Ok(false) if not dirty.
    pub async fn save_if_dirty(&self, storage: &DynStorage) -> Result<bool, CacheError> {
        if !self.dirty.swap(false, Ordering::Relaxed) {
            return Ok(false);
        }

        let cache = self.data.read().await;
        let json = serde_json::to_string_pretty(&*cache)?;
        storage.write(&self.filename, Bytes::from(json)).await?;
        *self.last_sync.write().await = Some(SystemTime::now());
        Ok(true)
    }

    /// Force save cache to storage regardless of dirty state.
    pub async fn save(&self, storage: &DynStorage) -> Result<(), CacheError> {
        let cache = self.data.read().await;
        let json = serde_json::to_string_pretty(&*cache)?;
        storage.write(&self.filename, Bytes::from(json)).await?;
        self.dirty.store(false, Ordering::Relaxed);
        *self.last_sync.write().await = Some(SystemTime::now());
        Ok(())
    }

    /// Check if storage has a newer version of the cache file.
    ///
    /// Returns true if storage file is newer than our last sync, false otherwise.
    /// Returns false if we've never synced or if storage file doesn't exist.
    pub async fn is_storage_newer(&self, storage: &DynStorage) -> bool {
        let last_sync = *self.last_sync.read().await;
        let Some(last_sync) = last_sync else {
            return false; // Never synced, can't compare
        };

        match storage.metadata(&self.filename).await {
            Ok(meta) => {
                if let Some(storage_mtime) = meta.last_modified {
                    storage_mtime > last_sync
                } else {
                    false // No mtime available, assume not newer
                }
            }
            Err(_) => false, // File doesn't exist or error
        }
    }

    /// Load from storage if the storage file is newer than our last sync.
    ///
    /// This is useful before expensive refresh operations - if another process
    /// has already refreshed and saved the cache, we can just reload it.
    ///
    /// Returns Ok(true) if cache was reloaded, Ok(false) if storage wasn't newer.
    pub async fn load_if_newer(&self, storage: &DynStorage) -> Result<bool, CacheError> {
        if self.is_storage_newer(storage).await {
            tracing::info!(
                "Storage cache '{}' is newer than in-memory, reloading",
                self.filename
            );
            self.load(storage).await?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Get the last sync timestamp (when cache was last loaded or saved).
    pub async fn last_sync_time(&self) -> Option<SystemTime> {
        *self.last_sync.read().await
    }

    /// Get an item from the cache.
    pub async fn get(&self, key: &str) -> Option<V> {
        self.data.read().await.get(key).cloned()
    }

    /// Check if a key exists in the cache.
    pub async fn contains(&self, key: &str) -> bool {
        self.data.read().await.contains_key(key)
    }

    /// Insert an item, marking the cache as dirty.
    pub async fn insert(&self, key: String, value: V) {
        self.data.write().await.insert(key, value);
        self.dirty.store(true, Ordering::Relaxed);
    }

    /// Remove an item from the cache, marking it as dirty if the key existed.
    pub async fn remove(&self, key: &str) -> Option<V> {
        let removed = self.data.write().await.remove(key);
        if removed.is_some() {
            self.dirty.store(true, Ordering::Relaxed);
        }
        removed
    }

    /// Replace entire cache contents, marking as dirty.
    pub async fn replace_all(&self, data: HashMap<String, V>) {
        *self.data.write().await = data;
        self.dirty.store(true, Ordering::Relaxed);
    }

    /// Get read-only access to all data.
    pub async fn read_all(&self) -> tokio::sync::RwLockReadGuard<'_, HashMap<String, V>> {
        self.data.read().await
    }

    /// Get mutable access to all data (use sparingly).
    /// Caller must manually call `mark_dirty()` if modifications are made.
    pub async fn write_all(&self) -> tokio::sync::RwLockWriteGuard<'_, HashMap<String, V>> {
        self.data.write().await
    }

    /// Clear the cache, marking as dirty.
    pub async fn clear(&self) {
        self.data.write().await.clear();
        self.dirty.store(true, Ordering::Relaxed);
    }

    /// Explicitly mark the cache as dirty (has unsaved changes).
    pub fn mark_dirty(&self) {
        self.dirty.store(true, Ordering::Relaxed);
    }

    /// Check if the cache has unsaved changes.
    pub fn is_dirty(&self) -> bool {
        self.dirty.load(Ordering::Relaxed)
    }

    /// Get the number of items in the cache.
    pub async fn len(&self) -> usize {
        self.data.read().await.len()
    }

    /// Check if the cache is empty.
    pub async fn is_empty(&self) -> bool {
        self.data.read().await.is_empty()
    }

    /// Get the filename this cache persists to.
    pub fn filename(&self) -> &str {
        &self.filename
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Debug, PartialEq, Serialize, serde::Deserialize)]
    struct TestData {
        value: String,
    }

    #[tokio::test]
    async fn test_insert_and_get() {
        let cache: PersistentCache<TestData> = PersistentCache::new("test.json");

        assert!(!cache.is_dirty());
        assert!(cache.get("key1").await.is_none());

        cache
            .insert(
                "key1".to_string(),
                TestData {
                    value: "hello".to_string(),
                },
            )
            .await;

        assert!(cache.is_dirty());
        assert_eq!(
            cache.get("key1").await,
            Some(TestData {
                value: "hello".to_string()
            })
        );
    }

    #[tokio::test]
    async fn test_replace_all() {
        let cache: PersistentCache<TestData> = PersistentCache::new("test.json");

        let mut data = HashMap::new();
        data.insert(
            "a".to_string(),
            TestData {
                value: "1".to_string(),
            },
        );
        data.insert(
            "b".to_string(),
            TestData {
                value: "2".to_string(),
            },
        );

        cache.replace_all(data).await;

        assert!(cache.is_dirty());
        assert_eq!(cache.len().await, 2);
        assert!(cache.contains("a").await);
        assert!(cache.contains("b").await);
    }

    #[tokio::test]
    async fn test_remove() {
        let cache: PersistentCache<TestData> = PersistentCache::new("test.json");

        cache
            .insert(
                "key1".to_string(),
                TestData {
                    value: "test".to_string(),
                },
            )
            .await;

        // Reset dirty flag to test remove
        cache.dirty.store(false, Ordering::Relaxed);

        let removed = cache.remove("key1").await;
        assert!(removed.is_some());
        assert!(cache.is_dirty());
        assert!(cache.get("key1").await.is_none());

        // Removing non-existent key doesn't mark dirty
        cache.dirty.store(false, Ordering::Relaxed);
        let removed = cache.remove("nonexistent").await;
        assert!(removed.is_none());
        assert!(!cache.is_dirty());
    }

    #[tokio::test]
    async fn test_clear() {
        let cache: PersistentCache<TestData> = PersistentCache::new("test.json");

        cache
            .insert(
                "key1".to_string(),
                TestData {
                    value: "test".to_string(),
                },
            )
            .await;
        cache.dirty.store(false, Ordering::Relaxed);

        cache.clear().await;

        assert!(cache.is_dirty());
        assert!(cache.is_empty().await);
    }
}
