//! Synchronous adapter for async storage operations.
//!
//! Provides sync wrappers for use in `spawn_blocking` contexts, with support
//! for speculative prefetching and chunk-based caching to reduce latency.

use super::{DynStorage, StorageError};
use bytes::Bytes;
use std::collections::HashMap;
use std::io::{self, Read, Seek, SeekFrom};
use std::sync::{Arc, Mutex};
use tokio::runtime::Handle;
use tokio::sync::oneshot;

/// Default chunk size for storage reads (256 KB).
const DEFAULT_CHUNK_SIZE: u64 = 256 * 1024;

/// Synchronous wrapper for storage write (for use in spawn_blocking).
///
/// Blocks the current thread until the write completes.
pub fn storage_write_sync(
    storage: &DynStorage,
    path: &str,
    data: Bytes,
    handle: &Handle,
) -> Result<(), StorageError> {
    handle.block_on(async { storage.write(path, data).await })
}

/// Synchronous wrapper for storage read (for use in spawn_blocking).
///
/// Blocks the current thread until the read completes.
pub fn storage_read_sync(
    storage: &DynStorage,
    path: &str,
    handle: &Handle,
) -> Result<Bytes, StorageError> {
    handle.block_on(async { storage.read(path).await })
}

/// Synchronous wrapper for storage exists check (for use in spawn_blocking).
///
/// Blocks the current thread until the check completes.
pub fn storage_exists_sync(storage: &DynStorage, path: &str, handle: &Handle) -> bool {
    handle.block_on(async { storage.exists(path).await.unwrap_or(false) })
}

/// Synchronous wrapper for storage range read (for use in spawn_blocking).
///
/// Blocks the current thread until the read completes.
pub fn storage_read_range_sync(
    storage: &DynStorage,
    path: &str,
    offset: u64,
    length: u64,
    handle: &Handle,
) -> Result<Bytes, StorageError> {
    handle.block_on(async { storage.read_range(path, offset, length).await })
}

/// A pending prefetch operation - waits on receiver for result.
type PrefetchReceiver = oneshot::Receiver<Result<Bytes, StorageError>>;

/// A synchronous reader for storage data that implements `Read` and `Seek`.
///
/// This reader uses chunk-based caching with speculative prefetching:
/// - Chunks are fetched on demand via range reads
/// - Read-ahead prefetches upcoming chunks
/// - Cached chunks avoid re-fetching
///
/// # Example
/// ```ignore
/// let reader = SyncStorageReader::open(&storage, "image.jpg", &handle)?;
/// let image = image::io::Reader::new(reader)
///     .with_guessed_format()?
///     .decode()?;
/// ```
pub struct SyncStorageReader {
    storage: DynStorage,
    path: String,
    handle: Handle,
    size: u64,
    position: u64,
    chunk_size: u64,
    cache: HashMap<u64, Bytes>,
    pending_prefetch: Option<(u64, PrefetchReceiver)>,
}

impl SyncStorageReader {
    /// Open a storage file for synchronous reading.
    ///
    /// This fetches metadata to determine file size, then creates a reader
    /// that fetches chunks on demand.
    pub fn open(storage: &DynStorage, path: &str, handle: &Handle) -> Result<Self, StorageError> {
        // Get file size from metadata
        let meta = handle.block_on(async { storage.metadata(path).await })?;

        Ok(Self {
            storage: storage.clone(),
            path: path.to_string(),
            handle: handle.clone(),
            size: meta.size,
            position: 0,
            chunk_size: DEFAULT_CHUNK_SIZE,
            cache: HashMap::new(),
            pending_prefetch: None,
        })
    }

    /// Open with a custom chunk size.
    pub fn open_with_chunk_size(
        storage: &DynStorage,
        path: &str,
        handle: &Handle,
        chunk_size: u64,
    ) -> Result<Self, StorageError> {
        let mut reader = Self::open(storage, path, handle)?;
        reader.chunk_size = chunk_size;
        Ok(reader)
    }

    /// Get the total length of the file.
    pub fn len(&self) -> u64 {
        self.size
    }

    /// Check if the file is empty.
    pub fn is_empty(&self) -> bool {
        self.size == 0
    }

    /// Get the current position in the file.
    pub fn position(&self) -> u64 {
        self.position
    }

    /// Get the chunk index for a given byte offset.
    fn chunk_index(&self, offset: u64) -> u64 {
        offset / self.chunk_size
    }

    /// Get the byte offset for a given chunk index.
    fn chunk_offset(&self, chunk_idx: u64) -> u64 {
        chunk_idx * self.chunk_size
    }

    /// Fetch a chunk, using cache or pending prefetch if available.
    fn fetch_chunk(&mut self, chunk_idx: u64) -> Result<Bytes, StorageError> {
        // Check cache first
        if let Some(data) = self.cache.get(&chunk_idx) {
            return Ok(data.clone());
        }

        // Check if this chunk is being prefetched
        if let Some((prefetch_idx, rx)) = self.pending_prefetch.take() {
            if prefetch_idx == chunk_idx {
                // Wait for prefetch to complete
                match self.handle.block_on(rx) {
                    Ok(result) => {
                        let data = result?;
                        self.cache.insert(chunk_idx, data.clone());
                        // Start prefetch for next chunk
                        self.start_prefetch(chunk_idx + 1);
                        return Ok(data);
                    }
                    Err(_) => {
                        // Channel closed, fall through to direct fetch
                    }
                }
            } else {
                // Different chunk, put the prefetch back
                self.pending_prefetch = Some((prefetch_idx, rx));
            }
        }

        // Fetch chunk directly
        let offset = self.chunk_offset(chunk_idx);
        let length = std::cmp::min(self.chunk_size, self.size.saturating_sub(offset));

        if length == 0 {
            return Ok(Bytes::new());
        }

        let data = self
            .handle
            .block_on(async { self.storage.read_range(&self.path, offset, length).await })?;

        self.cache.insert(chunk_idx, data.clone());

        // Start prefetch for next chunk
        self.start_prefetch(chunk_idx + 1);

        Ok(data)
    }

    /// Start prefetching the next chunk asynchronously.
    fn start_prefetch(&mut self, chunk_idx: u64) {
        // Don't prefetch if already prefetching or past end of file
        if self.pending_prefetch.is_some() {
            return;
        }

        let offset = self.chunk_offset(chunk_idx);
        if offset >= self.size {
            return;
        }

        // Don't prefetch if already cached
        if self.cache.contains_key(&chunk_idx) {
            return;
        }

        let length = std::cmp::min(self.chunk_size, self.size.saturating_sub(offset));
        if length == 0 {
            return;
        }

        let storage = self.storage.clone();
        let path = self.path.clone();
        let (tx, rx) = oneshot::channel();

        // Spawn async prefetch task
        tokio::spawn(async move {
            let result = storage.read_range(&path, offset, length).await;
            let _ = tx.send(result);
        });

        self.pending_prefetch = Some((chunk_idx, rx));
    }

    /// Clear the chunk cache to free memory.
    pub fn clear_cache(&mut self) {
        self.cache.clear();
    }

    /// Prefetch a range of chunks.
    ///
    /// This is useful when you know you'll need specific data soon.
    pub fn prefetch_range(&mut self, start_offset: u64, length: u64) {
        let start_chunk = self.chunk_index(start_offset);
        let end_chunk = self.chunk_index(start_offset + length);

        for chunk_idx in start_chunk..=end_chunk {
            if !self.cache.contains_key(&chunk_idx) {
                // Start prefetch for first uncached chunk
                self.start_prefetch(chunk_idx);
                break;
            }
        }
    }
}

impl Read for SyncStorageReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.position >= self.size {
            return Ok(0);
        }

        let mut bytes_read = 0;
        let mut remaining = buf.len();

        while remaining > 0 && self.position < self.size {
            let chunk_idx = self.chunk_index(self.position);
            let chunk_data = self
                .fetch_chunk(chunk_idx)
                .map_err(|e| io::Error::other(format!("Storage error: {}", e)))?;

            let chunk_start = self.chunk_offset(chunk_idx);
            let offset_in_chunk = (self.position - chunk_start) as usize;
            let available = chunk_data.len().saturating_sub(offset_in_chunk);
            let to_copy = std::cmp::min(remaining, available);

            if to_copy == 0 {
                break;
            }

            buf[bytes_read..bytes_read + to_copy]
                .copy_from_slice(&chunk_data[offset_in_chunk..offset_in_chunk + to_copy]);

            bytes_read += to_copy;
            remaining -= to_copy;
            self.position += to_copy as u64;
        }

        Ok(bytes_read)
    }
}

impl Seek for SyncStorageReader {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        let new_pos = match pos {
            SeekFrom::Start(offset) => offset as i64,
            SeekFrom::End(offset) => self.size as i64 + offset,
            SeekFrom::Current(offset) => self.position as i64 + offset,
        };

        if new_pos < 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Seek before start of file",
            ));
        }

        self.position = new_pos as u64;
        Ok(self.position)
    }
}

/// A synchronous reader that pre-loads the entire file into memory.
///
/// This is more efficient than `SyncStorageReader` when you know you'll
/// need the entire file (e.g., for image processing), especially for S3
/// where a single GET request is cheaper than multiple range requests.
pub struct PreloadedReader {
    data: Bytes,
    position: usize,
}

impl PreloadedReader {
    /// Create a new preloaded reader from a storage path.
    ///
    /// This blocks until the entire file is loaded.
    pub fn open(storage: &DynStorage, path: &str, handle: &Handle) -> Result<Self, StorageError> {
        let data = handle.block_on(async { storage.read(path).await })?;
        Ok(Self { data, position: 0 })
    }

    /// Create a preloaded reader from existing bytes.
    pub fn from_bytes(data: Bytes) -> Self {
        Self { data, position: 0 }
    }

    /// Get the total length of the file.
    pub fn len(&self) -> usize {
        self.data.len()
    }

    /// Check if the file is empty.
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    /// Get the underlying bytes.
    pub fn into_bytes(self) -> Bytes {
        self.data
    }

    /// Get a reference to the underlying bytes.
    pub fn as_bytes(&self) -> &Bytes {
        &self.data
    }
}

impl Read for PreloadedReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let remaining = self.data.len().saturating_sub(self.position);
        let to_copy = std::cmp::min(remaining, buf.len());

        if to_copy == 0 {
            return Ok(0);
        }

        buf[..to_copy].copy_from_slice(&self.data[self.position..self.position + to_copy]);
        self.position += to_copy;
        Ok(to_copy)
    }
}

impl Seek for PreloadedReader {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        let new_pos = match pos {
            SeekFrom::Start(offset) => offset as i64,
            SeekFrom::End(offset) => self.data.len() as i64 + offset,
            SeekFrom::Current(offset) => self.position as i64 + offset,
        };

        if new_pos < 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Seek before start of file",
            ));
        }

        self.position = new_pos as usize;
        Ok(self.position as u64)
    }
}

/// Open a storage file for synchronous reading with the specified strategy.
///
/// Returns either a streaming reader (for partial reads) or a preloaded reader
/// (for full file access), both implementing `Read + Seek`.
pub fn storage_open_with_strategy(
    storage: &DynStorage,
    path: &str,
    strategy: super::ReadStrategy,
    handle: &Handle,
) -> Result<Box<dyn ReadSeek>, StorageError> {
    match strategy {
        super::ReadStrategy::Streaming => {
            let reader = SyncStorageReader::open(storage, path, handle)?;
            Ok(Box::new(reader))
        }
        super::ReadStrategy::FullFetch => {
            let reader = PreloadedReader::open(storage, path, handle)?;
            Ok(Box::new(reader))
        }
        super::ReadStrategy::HeaderOnly { size, .. } => {
            // Read only the header portion into a PreloadedReader
            let data = storage_read_range_sync(storage, path, 0, size, handle)?;
            Ok(Box::new(PreloadedReader::from_bytes(data)))
        }
    }
}

/// Trait combining Read and Seek for type erasure.
pub trait ReadSeek: Read + Seek + Send {}
impl<T: Read + Seek + Send> ReadSeek for T {}

/// A synchronous storage adapter with speculative prefetching support.
///
/// This adapter allows you to:
/// 1. Speculatively start async reads before entering sync code
/// 2. Retrieve the data synchronously (blocks if not yet ready)
/// 3. Cache results for repeated access
///
/// # Example
/// ```ignore
/// // Before entering spawn_blocking, start prefetches
/// let adapter = SyncStorageAdapter::new(storage.clone());
/// adapter.prefetch("image1.jpg");
/// adapter.prefetch("image2.jpg");
///
/// // Inside spawn_blocking
/// let handle = Handle::current();
/// let data = adapter.read_sync("image1.jpg", &handle)?;
/// ```
pub struct SyncStorageAdapter {
    storage: DynStorage,
    pending: Arc<Mutex<HashMap<String, PrefetchReceiver>>>,
}

impl SyncStorageAdapter {
    /// Create a new sync storage adapter.
    pub fn new(storage: DynStorage) -> Self {
        Self {
            storage,
            pending: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Start a speculative prefetch for a path.
    ///
    /// This spawns an async task to fetch the data. Call `read_sync` later
    /// to get the result (blocks if not yet ready).
    pub fn prefetch(&self, path: &str) {
        let path = path.to_string();
        let storage = self.storage.clone();
        let pending = self.pending.clone();

        // Check if already prefetching
        {
            let guard = pending.lock().unwrap();
            if guard.contains_key(&path) {
                return; // Already prefetching
            }
        }

        // Create channel for result
        let (tx, rx) = oneshot::channel();

        // Insert pending receiver
        {
            let mut guard = pending.lock().unwrap();
            guard.insert(path.clone(), rx);
        }

        // Spawn prefetch task
        let pending_for_task = pending.clone();
        let path_for_task = path.clone();
        tokio::spawn(async move {
            let result = storage.read(&path_for_task).await;

            // Try to send result through channel
            // If receiver is dropped (read_sync already retrieved via blocking read),
            // we just drop the result - it's already been handled
            let _ = tx.send(result);

            // Remove the pending entry (either it was consumed or we sent it)
            let mut guard = pending_for_task.lock().unwrap();
            guard.remove(&path_for_task);
        });
    }

    /// Read data synchronously, using prefetched data if available.
    ///
    /// If the data was prefetched, this returns immediately (or blocks until
    /// the prefetch completes). If not prefetched, this performs a blocking read.
    pub fn read_sync(&self, path: &str, handle: &Handle) -> Result<Bytes, StorageError> {
        // Check for pending prefetch
        let receiver = {
            let mut guard = self.pending.lock().unwrap();
            guard.remove(path)
        };

        match receiver {
            Some(rx) => {
                // Prefetch in progress, wait for it
                match handle.block_on(rx) {
                    Ok(result) => result,
                    Err(_) => {
                        // Channel closed unexpectedly, fall back to direct read
                        storage_read_sync(&self.storage, path, handle)
                    }
                }
            }
            None => {
                // Not prefetched, do blocking read
                storage_read_sync(&self.storage, path, handle)
            }
        }
    }

    /// Open a file for sync reading with chunked caching.
    ///
    /// Returns a `SyncStorageReader` that implements `Read` and `Seek`.
    pub fn open(&self, path: &str, handle: &Handle) -> Result<SyncStorageReader, StorageError> {
        SyncStorageReader::open(&self.storage, path, handle)
    }

    /// Write data synchronously.
    pub fn write_sync(&self, path: &str, data: Bytes, handle: &Handle) -> Result<(), StorageError> {
        storage_write_sync(&self.storage, path, data, handle)
    }

    /// Check if path exists synchronously.
    pub fn exists_sync(&self, path: &str, handle: &Handle) -> bool {
        storage_exists_sync(&self.storage, path, handle)
    }

    /// Get the underlying storage.
    pub fn storage(&self) -> &DynStorage {
        &self.storage
    }
}

impl Clone for SyncStorageAdapter {
    fn clone(&self) -> Self {
        Self {
            storage: self.storage.clone(),
            pending: self.pending.clone(),
        }
    }
}

/// Convenience function to open a storage file for sync reading.
///
/// This is equivalent to `SyncStorageReader::open()`.
pub fn storage_open_sync(
    storage: &DynStorage,
    path: &str,
    handle: &Handle,
) -> Result<SyncStorageReader, StorageError> {
    SyncStorageReader::open(storage, path, handle)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::FilesystemStorage;
    use std::sync::Arc;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_sync_write_and_read() {
        let temp_dir = TempDir::new().unwrap();
        let storage: DynStorage = Arc::new(FilesystemStorage::new(temp_dir.path().to_path_buf()));
        let handle = Handle::current();

        // Run sync operations from spawn_blocking (simulates real usage)
        let storage_clone = storage.clone();
        let result = tokio::task::spawn_blocking(move || {
            // Test write
            let data = Bytes::from("hello world");
            storage_write_sync(&storage_clone, "test.txt", data.clone(), &handle)?;

            // Test read
            let read_data = storage_read_sync(&storage_clone, "test.txt", &handle)?;
            assert_eq!(read_data, data);

            // Test exists
            assert!(storage_exists_sync(&storage_clone, "test.txt", &handle));
            assert!(!storage_exists_sync(
                &storage_clone,
                "nonexistent.txt",
                &handle
            ));

            Ok::<_, StorageError>(())
        })
        .await
        .unwrap();

        result.unwrap();
    }

    #[tokio::test]
    async fn test_sync_storage_reader() {
        let temp_dir = TempDir::new().unwrap();
        let storage: DynStorage = Arc::new(FilesystemStorage::new(temp_dir.path().to_path_buf()));

        // Write test file with known content
        let test_data = b"Hello, World! This is a test file with some content.";
        storage
            .write("test.txt", Bytes::from(&test_data[..]))
            .await
            .unwrap();

        let handle = Handle::current();

        // Test reading in spawn_blocking
        let storage_clone = storage.clone();
        let result = tokio::task::spawn_blocking(move || {
            let mut reader = SyncStorageReader::open(&storage_clone, "test.txt", &handle)?;

            // Check metadata
            assert_eq!(reader.len(), test_data.len() as u64);
            assert!(!reader.is_empty());
            assert_eq!(reader.position(), 0);

            // Read all content
            let mut buffer = Vec::new();
            reader.read_to_end(&mut buffer)?;
            assert_eq!(buffer, test_data);

            Ok::<_, Box<dyn std::error::Error + Send + Sync>>(())
        })
        .await
        .unwrap();

        result.unwrap();
    }

    #[tokio::test]
    async fn test_sync_storage_reader_seek() {
        let temp_dir = TempDir::new().unwrap();
        let storage: DynStorage = Arc::new(FilesystemStorage::new(temp_dir.path().to_path_buf()));

        // Write test file: "0123456789ABCDEF" (16 bytes)
        // Position: 0123456789...
        //           0         10
        let test_data = b"0123456789ABCDEF";
        storage
            .write("seek_test.txt", Bytes::from(&test_data[..]))
            .await
            .unwrap();

        let handle = Handle::current();
        let storage_clone = storage.clone();

        let result = tokio::task::spawn_blocking(move || {
            let mut reader = SyncStorageReader::open(&storage_clone, "seek_test.txt", &handle)?;

            // Seek to position 10 (where 'A' starts)
            reader.seek(SeekFrom::Start(10))?;
            assert_eq!(reader.position(), 10);

            // Read from position - should get "ABCD"
            let mut buf = [0u8; 4];
            reader.read_exact(&mut buf)?;
            assert_eq!(&buf, b"ABCD");

            // Seek relative back 4 bytes, should be at 'A' again
            reader.seek(SeekFrom::Current(-4))?;
            reader.read_exact(&mut buf)?;
            assert_eq!(&buf, b"ABCD");

            // Seek from end (-4 bytes from end = position 12 = 'CDEF')
            reader.seek(SeekFrom::End(-4))?;
            reader.read_exact(&mut buf)?;
            assert_eq!(&buf, b"CDEF");

            Ok::<_, Box<dyn std::error::Error + Send + Sync>>(())
        })
        .await
        .unwrap();

        result.unwrap();
    }

    #[tokio::test]
    async fn test_sync_storage_reader_chunked() {
        let temp_dir = TempDir::new().unwrap();
        let storage: DynStorage = Arc::new(FilesystemStorage::new(temp_dir.path().to_path_buf()));

        // Create larger test file (multiple chunks with small chunk size)
        let test_data: Vec<u8> = (0..1000).map(|i| (i % 256) as u8).collect();
        storage
            .write("chunked.bin", Bytes::from(test_data.clone()))
            .await
            .unwrap();

        let handle = Handle::current();
        let storage_clone = storage.clone();

        let result = tokio::task::spawn_blocking(move || {
            // Use small chunk size to test chunking
            let mut reader = SyncStorageReader::open_with_chunk_size(
                &storage_clone,
                "chunked.bin",
                &handle,
                64,
            )?;

            // Read entire file in small pieces
            let mut buffer = Vec::new();
            let mut small_buf = [0u8; 17]; // Odd size to cross chunk boundaries

            loop {
                let n = reader.read(&mut small_buf)?;
                if n == 0 {
                    break;
                }
                buffer.extend_from_slice(&small_buf[..n]);
            }

            assert_eq!(buffer, test_data);

            Ok::<_, Box<dyn std::error::Error + Send + Sync>>(())
        })
        .await
        .unwrap();

        result.unwrap();
    }

    #[tokio::test]
    async fn test_prefetch_adapter() {
        let temp_dir = TempDir::new().unwrap();
        let storage: DynStorage = Arc::new(FilesystemStorage::new(temp_dir.path().to_path_buf()));

        // Write test file
        storage
            .write("test.txt", Bytes::from("prefetched"))
            .await
            .unwrap();

        // Create adapter and prefetch
        let adapter = SyncStorageAdapter::new(storage);
        adapter.prefetch("test.txt");

        // Give prefetch time to complete
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        // Read from spawn_blocking
        let handle = Handle::current();
        let data = tokio::task::spawn_blocking(move || adapter.read_sync("test.txt", &handle))
            .await
            .unwrap()
            .unwrap();

        assert_eq!(data, Bytes::from("prefetched"));
    }

    #[tokio::test]
    async fn test_adapter_without_prefetch() {
        let temp_dir = TempDir::new().unwrap();
        let storage: DynStorage = Arc::new(FilesystemStorage::new(temp_dir.path().to_path_buf()));

        // Write test file
        storage
            .write("test.txt", Bytes::from("direct"))
            .await
            .unwrap();

        // Create adapter without prefetch
        let adapter = SyncStorageAdapter::new(storage);

        // Read from spawn_blocking (no prefetch)
        let handle = Handle::current();
        let data = tokio::task::spawn_blocking(move || adapter.read_sync("test.txt", &handle))
            .await
            .unwrap()
            .unwrap();

        assert_eq!(data, Bytes::from("direct"));
    }

    #[tokio::test]
    async fn test_range_read_sync() {
        let temp_dir = TempDir::new().unwrap();
        let storage: DynStorage = Arc::new(FilesystemStorage::new(temp_dir.path().to_path_buf()));

        // Write test file
        let test_data = b"0123456789ABCDEF";
        storage
            .write("range.txt", Bytes::from(&test_data[..]))
            .await
            .unwrap();

        let handle = Handle::current();
        let storage_clone = storage.clone();

        let result = tokio::task::spawn_blocking(move || {
            // Read middle portion
            let data = storage_read_range_sync(&storage_clone, "range.txt", 4, 8, &handle)?;
            assert_eq!(&data[..], b"456789AB");
            Ok::<_, StorageError>(())
        })
        .await
        .unwrap();

        result.unwrap();
    }
}
