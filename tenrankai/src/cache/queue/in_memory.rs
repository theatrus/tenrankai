//! In-memory queue implementation using tokio channels.

use super::{CacheCleanupRequest, CacheGenerationRequest, CacheQueue, QueueError, QueueMessage};
use async_trait::async_trait;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use tokio::sync::{Mutex, mpsc};

/// In-memory cache queue using tokio mpsc channel.
///
/// This is the default queue implementation, suitable for single-server
/// deployments. Messages are lost on restart.
pub struct InMemoryQueue {
    sender: mpsc::Sender<QueueMessage>,
    receiver: Mutex<mpsc::Receiver<QueueMessage>>,
    depth: AtomicUsize,
    closed: AtomicBool,
}

impl InMemoryQueue {
    /// Create a new in-memory queue with the specified buffer size.
    pub fn new(buffer_size: usize) -> Self {
        let (sender, receiver) = mpsc::channel(buffer_size);
        Self {
            sender,
            receiver: Mutex::new(receiver),
            depth: AtomicUsize::new(0),
            closed: AtomicBool::new(false),
        }
    }

    /// Create a new in-memory queue with default buffer size (1000).
    pub fn with_default_size() -> Self {
        Self::new(1000)
    }
}

#[async_trait]
impl CacheQueue for InMemoryQueue {
    async fn submit(&self, request: CacheGenerationRequest) -> Result<(), QueueError> {
        if self.closed.load(Ordering::SeqCst) {
            return Err(QueueError::QueueClosed);
        }

        match self.sender.try_send(QueueMessage::Generate(request)) {
            Ok(()) => {
                self.depth.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }
            Err(mpsc::error::TrySendError::Full(_)) => Err(QueueError::QueueFull),
            Err(mpsc::error::TrySendError::Closed(_)) => Err(QueueError::QueueClosed),
        }
    }

    async fn submit_cleanup(&self, request: CacheCleanupRequest) -> Result<(), QueueError> {
        if self.closed.load(Ordering::SeqCst) {
            return Err(QueueError::QueueClosed);
        }

        match self.sender.try_send(QueueMessage::Cleanup(request)) {
            Ok(()) => {
                self.depth.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }
            Err(mpsc::error::TrySendError::Full(_)) => Err(QueueError::QueueFull),
            Err(mpsc::error::TrySendError::Closed(_)) => Err(QueueError::QueueClosed),
        }
    }

    async fn receive(&self) -> Option<QueueMessage> {
        let mut receiver = self.receiver.lock().await;
        let msg = receiver.recv().await;
        if msg.is_some() {
            self.depth.fetch_sub(1, Ordering::SeqCst);
        }
        msg
    }

    fn queue_depth(&self) -> usize {
        self.depth.load(Ordering::SeqCst)
    }

    async fn close(&self) {
        self.closed.store(true, Ordering::SeqCst);
    }

    fn queue_type(&self) -> &'static str {
        "in-memory"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_submit_and_receive() {
        let queue = InMemoryQueue::new(10);

        // Submit a generation request
        queue
            .submit(CacheGenerationRequest::new(
                "test-site",
                "test-gallery",
                "path/to/image.jpg",
            ))
            .await
            .unwrap();

        assert_eq!(queue.queue_depth(), 1);

        // Receive it
        let msg = queue.receive().await.unwrap();
        match msg {
            QueueMessage::Generate(req) => {
                assert_eq!(req.site_name, "test-site");
                assert_eq!(req.gallery_name, "test-gallery");
                assert_eq!(req.image_path, "path/to/image.jpg");
                assert_eq!(req.queue_key(), "test-site:test-gallery");
            }
            _ => panic!("Expected Generate message"),
        }

        assert_eq!(queue.queue_depth(), 0);
    }

    #[tokio::test]
    async fn test_cleanup_request() {
        let queue = InMemoryQueue::new(10);

        queue
            .submit_cleanup(CacheCleanupRequest::new(
                "test-site",
                "test-gallery",
                "old/path.jpg",
            ))
            .await
            .unwrap();

        let msg = queue.receive().await.unwrap();
        match msg {
            QueueMessage::Cleanup(req) => {
                assert_eq!(req.site_name, "test-site");
                assert_eq!(req.gallery_name, "test-gallery");
                assert_eq!(req.old_path, "old/path.jpg");
                assert_eq!(req.queue_key(), "test-site:test-gallery");
            }
            _ => panic!("Expected Cleanup message"),
        }
    }

    #[tokio::test]
    async fn test_queue_full() {
        let queue = InMemoryQueue::new(1);

        // First should succeed
        queue
            .submit(CacheGenerationRequest::new("s", "g", "p1"))
            .await
            .unwrap();

        // Second should fail with QueueFull
        let result = queue
            .submit(CacheGenerationRequest::new("s", "g", "p2"))
            .await;
        assert!(matches!(result, Err(QueueError::QueueFull)));
    }

    #[tokio::test]
    async fn test_closed_queue() {
        let queue = InMemoryQueue::new(10);
        queue.close().await;

        let result = queue
            .submit(CacheGenerationRequest::new("s", "g", "p"))
            .await;
        assert!(matches!(result, Err(QueueError::QueueClosed)));
    }
}
