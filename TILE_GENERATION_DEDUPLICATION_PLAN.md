# Image Generation Task Deduplication Plan

## Problem Statement

The current implementation has race conditions where multiple concurrent requests for the same image processing task result in duplicate work:

- Frontend requests tiles 0,0 and 1,0 and 0,1 almost simultaneously
- Each request triggers `process_all_tiles_for_image()`
- The same image gets loaded and all tiles generated multiple times in parallel
- This wastes CPU and I/O resources

This issue affects:
- Regular image resizing (thumbnail, gallery, medium, large)
- Tile generation
- Composite image generation
- Any other image processing operations

## Proposed Solution: TaskDeduplicator Helper Struct

Create a reusable helper struct that encapsulates the complexity of task deduplication.

### Core Design

```rust
// In a new file: src/gallery/task_deduplicator.rs
pub struct TaskDeduplicator {
    pending_tasks: Arc<RwLock<HashMap<String, Arc<tokio::sync::Notify>>>>
}

impl TaskDeduplicator {
    pub fn new() -> Self { 
        Self {
            pending_tasks: Arc::new(RwLock::new(HashMap::new()))
        }
    }
    
    // Returns a handle that indicates whether to execute or wait
    pub async fn should_execute(&self, key: String) -> TaskHandle {
        let mut tasks = self.pending_tasks.write().await;
        
        if let Some(notify) = tasks.get(&key) {
            // Another task is running, return wait handle
            TaskHandle {
                key: key.clone(),
                deduplicator: self.clone(),
                is_executor: false,
                notify: Some(notify.clone()),
            }
        } else {
            // We should execute
            let notify = Arc::new(tokio::sync::Notify::new());
            tasks.insert(key.clone(), notify);
            TaskHandle {
                key,
                deduplicator: self.clone(), 
                is_executor: true,
                notify: None,
            }
        }
    }
}

pub struct TaskHandle {
    key: String,
    deduplicator: Arc<TaskDeduplicator>,
    is_executor: bool,
    notify: Option<Arc<tokio::sync::Notify>>,
}

impl TaskHandle {
    pub fn is_executor(&self) -> bool {
        self.is_executor
    }
    
    // Wait for the task to complete (if not executor)
    pub async fn wait(&self) {
        if let Some(notify) = &self.notify {
            notify.notified().await;
        }
    }
    
    // Mark task as complete and notify waiters (if executor)
    pub fn complete(self) {
        if self.is_executor {
            let tasks = self.deduplicator.pending_tasks.blocking_write();
            if let Some(notify) = tasks.remove(&self.key) {
                notify.notify_waiters();
            }
        }
    }
}

// Ensure cleanup on drop
impl Drop for TaskHandle {
    fn drop(&mut self) {
        if self.is_executor {
            // Ensure we clean up if complete() wasn't called
            // This prevents memory leaks on error paths
        }
    }
}
```

### Usage Pattern

```rust
// In Gallery struct:
pub struct Gallery {
    // ... existing fields ...
    image_generation_deduplicator: Arc<TaskDeduplicator>,
}

// In Gallery::new():
image_generation_deduplicator: Arc::new(TaskDeduplicator::new()),

// Example: In get_image_tile_with_size()
pub async fn get_image_tile_with_size(&self, ...) -> Result<PathBuf, GalleryError> {
    // Check cache first
    if self.is_cache_valid(&cache_path, original_path).await? {
        return Ok(cache_path);
    }
    
    // Create deduplication key
    let dedup_key = format!("tiles_{}_size{}", relative_path, tile_size);
    
    // Check if we should execute or wait
    let task_handle = self.image_generation_deduplicator
        .should_execute(dedup_key)
        .await;
    
    if task_handle.is_executor() {
        // We're responsible for generation
        let result = tokio::task::spawn_blocking(move || {
            process_all_tiles_for_image(...)
        }).await?;
        
        // Mark as complete and notify waiters
        task_handle.complete();
        
        result?;
    } else {
        // Wait for the other task
        task_handle.wait().await;
        
        // Check cache again - it should exist now
        if !self.is_cache_valid(&cache_path, original_path).await? {
            return Err(GalleryError::CacheGenerationFailed);
        }
    }
    
    Ok(cache_path)
}

// Example: In get_resized_image()
pub async fn get_resized_image(&self, ...) -> Result<PathBuf, GalleryError> {
    // Similar pattern with key like:
    let dedup_key = format!("resize_{}_{}_{}_{}", 
        relative_path, size, output_format.extension(), apply_watermark);
    // ... same deduplication logic
}
```

## Implementation Locations

1. **Tile Generation**: In `get_image_tile_with_size()`
   - Key format: `"tiles_{path}_size{size}"`

2. **Regular Resizing**: In `get_resized_image()` 
   - Key format: `"resize_{path}_{size}_{format}_{watermark}"`

3. **Composite Generation**: In `store_and_serve_composite()`
   - Key format: `"composite_{cache_key}"`

4. **Metadata Extraction**: In `extract_metadata()` if needed
   - Key format: `"metadata_{path}"`

## Benefits

1. **Encapsulates Complexity**: The complex type `Arc<RwLock<HashMap<String, Arc<tokio::sync::Notify>>>>` is hidden inside a clean API

2. **Reusable**: Can be used for any type of task deduplication

3. **Self-Cleaning**: RAII pattern ensures cleanup even on error paths

4. **Type Safe**: Can't forget to notify waiters or clean up

5. **Testable**: The deduplicator logic can be unit tested independently

6. **Performance**: Reduces redundant I/O and CPU usage significantly

## Additional Considerations

### Metrics and Logging
```rust
impl TaskDeduplicator {
    pub async fn stats(&self) -> DeduplicatorStats {
        let tasks = self.pending_tasks.read().await;
        DeduplicatorStats {
            active_tasks: tasks.len(),
            // Could track historical stats too
        }
    }
}
```

### Timeout Support
```rust
pub async fn wait_with_timeout(&self, duration: Duration) -> Result<(), Timeout> {
    tokio::time::timeout(duration, self.wait()).await
}
```

### Error Handling
- Consider what happens if a task executor panics
- May want to add error propagation between executor and waiters

### Testing Strategy
```rust
#[cfg(test)]
mod tests {
    #[tokio::test]
    async fn test_single_execution() {
        // Verify only one task executes
    }
    
    #[tokio::test] 
    async fn test_multiple_waiters() {
        // Verify all waiters get notified
    }
    
    #[tokio::test]
    async fn test_cleanup_on_error() {
        // Verify no memory leaks on panic
    }
}
```

## Migration Steps

1. Create `task_deduplicator.rs` module
2. Add to Gallery struct 
3. Update `get_image_tile_with_size()` first (highest impact)
4. Update `get_resized_image()` 
5. Update `store_and_serve_composite()`
6. Add metrics/logging
7. Load test to verify improvement

## Expected Results

- Tile generation: ~4x reduction in redundant work (4 tiles → 1 generation)
- Regular resizing: 2-3x reduction for concurrent page loads
- Significant I/O and CPU savings during high load
- Better user experience with faster image loading