//! Cache generation queue for background processing.
//!
//! This module provides a pluggable queue interface for cache generation
//! and cleanup tasks. The default implementation uses an in-memory channel,
//! but can be extended to use SQS or other backends.

mod in_memory;
mod worker;

pub use in_memory::InMemoryQueue;
pub use worker::CacheQueueWorker;

use async_trait::async_trait;
use std::sync::Arc;
use thiserror::Error;

/// A request to generate cache files for an image.
#[derive(Debug, Clone)]
pub struct CacheGenerationRequest {
    /// Gallery name (for routing to correct gallery)
    pub gallery_name: String,
    /// Relative path to the image
    pub image_path: String,
    /// Priority (higher = process sooner, default 5)
    pub priority: u8,
}

impl CacheGenerationRequest {
    pub fn new(gallery_name: impl Into<String>, image_path: impl Into<String>) -> Self {
        Self {
            gallery_name: gallery_name.into(),
            image_path: image_path.into(),
            priority: 5,
        }
    }

    pub fn with_priority(mut self, priority: u8) -> Self {
        self.priority = priority;
        self
    }
}

/// A request to delete cache files for an image that was moved/deleted.
#[derive(Debug, Clone)]
pub struct CacheCleanupRequest {
    /// Gallery name
    pub gallery_name: String,
    /// Relative path to the image that was moved/deleted
    pub old_path: String,
}

impl CacheCleanupRequest {
    pub fn new(gallery_name: impl Into<String>, old_path: impl Into<String>) -> Self {
        Self {
            gallery_name: gallery_name.into(),
            old_path: old_path.into(),
        }
    }
}

/// Message types for the queue.
#[derive(Debug, Clone)]
pub enum QueueMessage {
    /// Generate cache files for an image
    Generate(CacheGenerationRequest),
    /// Delete cache files for a moved/deleted image
    Cleanup(CacheCleanupRequest),
}

/// Error type for queue operations.
#[derive(Debug, Error)]
pub enum QueueError {
    #[error("Queue is full")]
    QueueFull,
    #[error("Queue is closed")]
    QueueClosed,
    #[error("Backend error: {0}")]
    BackendError(String),
}

/// Trait for cache generation queue implementations.
///
/// This trait defines the interface for queue backends. The default
/// implementation uses an in-memory channel, but this can be extended
/// to use SQS or other message queues.
#[async_trait]
pub trait CacheQueue: Send + Sync + 'static {
    /// Submit a cache generation request.
    async fn submit(&self, request: CacheGenerationRequest) -> Result<(), QueueError>;

    /// Submit a cache cleanup request.
    async fn submit_cleanup(&self, request: CacheCleanupRequest) -> Result<(), QueueError>;

    /// Receive the next message (blocks until available or queue closed).
    async fn receive(&self) -> Option<QueueMessage>;

    /// Get approximate queue depth (for monitoring).
    fn queue_depth(&self) -> usize;

    /// Check if the queue is empty.
    fn is_empty(&self) -> bool {
        self.queue_depth() == 0
    }

    /// Close the queue (no more messages accepted).
    async fn close(&self);

    /// Queue type name for logging.
    fn queue_type(&self) -> &'static str;
}

/// Type alias for dynamic dispatch of queue implementations.
pub type DynCacheQueue = Arc<dyn CacheQueue>;
