//! Integration tests for S3 storage backend.
//!
//! These tests require MinIO or LocalStack running locally.
//! Tests automatically skip if MinIO is not available.
//!
//! To run with MinIO:
//!
//!   # Start MinIO
//!   podman run -d --name minio-test -p 9000:9000 -p 9001:9001 \
//!     -e MINIO_ROOT_USER=minioadmin -e MINIO_ROOT_PASSWORD=minioadmin \
//!     minio/minio server /data --console-address ":9001"
//!
//!   # Run tests
//!   AWS_ACCESS_KEY_ID=minioadmin AWS_SECRET_ACCESS_KEY=minioadmin \
//!     cargo test --test s3_storage_integration

use bytes::Bytes;
use std::time::Duration;
use tenrankai::storage::{S3Storage, Storage, StorageError};

const MINIO_ENDPOINT: &str = "http://localhost:9000";
const TEST_BUCKET: &str = "test-bucket";
const TEST_REGION: &str = "us-east-1";

/// Check if MinIO is available
async fn minio_available() -> bool {
    let client = reqwest::Client::new();
    client
        .get(format!("{}/minio/health/live", MINIO_ENDPOINT))
        .timeout(Duration::from_secs(2))
        .send()
        .await
        .is_ok()
}

/// Create the test bucket if it doesn't exist
async fn ensure_bucket_exists() -> Result<(), Box<dyn std::error::Error>> {
    use aws_config::BehaviorVersion;
    use aws_sdk_s3::config::Region;

    let config = aws_config::defaults(BehaviorVersion::latest())
        .region(Region::new(TEST_REGION))
        .load()
        .await;

    let s3_config = aws_sdk_s3::config::Builder::from(&config)
        .endpoint_url(MINIO_ENDPOINT)
        .force_path_style(true)
        .build();

    let client = aws_sdk_s3::Client::from_conf(s3_config);

    // Try to create bucket, ignore if it already exists
    match client.create_bucket().bucket(TEST_BUCKET).send().await {
        Ok(_) => println!("Created test bucket: {}", TEST_BUCKET),
        Err(e) => {
            let service_error = e.into_service_error();
            // BucketAlreadyOwnedByYou or BucketAlreadyExists is fine
            if !service_error.is_bucket_already_owned_by_you()
                && !service_error.is_bucket_already_exists()
            {
                return Err(format!("Failed to create bucket: {}", service_error).into());
            }
        }
    }

    Ok(())
}

/// Create an S3Storage instance for testing
async fn create_test_storage(prefix: &str) -> S3Storage {
    S3Storage::new(
        TEST_BUCKET.to_string(),
        prefix.to_string(),
        Some(TEST_REGION.to_string()),
        Some(MINIO_ENDPOINT.to_string()),
    )
    .await
    .expect("Failed to create S3Storage")
}

#[tokio::test]
async fn test_s3_basic_operations() {
    if !minio_available().await {
        eprintln!(
            "Skipping S3 test: MinIO not available at {}",
            MINIO_ENDPOINT
        );
        return;
    }

    ensure_bucket_exists()
        .await
        .expect("Failed to ensure bucket exists");

    let storage = create_test_storage("test-basic").await;

    // Write
    storage
        .write("hello.txt", Bytes::from("Hello, S3!"))
        .await
        .expect("write failed");

    // Exists
    assert!(storage.exists("hello.txt").await.expect("exists failed"));
    assert!(
        !storage
            .exists("nonexistent.txt")
            .await
            .expect("exists failed")
    );

    // Read
    let data = storage.read("hello.txt").await.expect("read failed");
    assert_eq!(&data[..], b"Hello, S3!");

    // Metadata
    let meta = storage
        .metadata("hello.txt")
        .await
        .expect("metadata failed");
    assert_eq!(meta.size, 10);
    assert!(meta.last_modified.is_some());
    assert!(meta.etag.is_some());

    // Delete
    storage.delete("hello.txt").await.expect("delete failed");
    assert!(!storage.exists("hello.txt").await.expect("exists failed"));
}

#[tokio::test]
async fn test_s3_nested_paths() {
    if !minio_available().await {
        eprintln!("Skipping S3 test: MinIO not available");
        return;
    }

    ensure_bucket_exists()
        .await
        .expect("Failed to ensure bucket exists");

    let storage = create_test_storage("test-nested").await;

    // Write to nested path
    storage
        .write("a/b/c/deep.txt", Bytes::from("deep content"))
        .await
        .expect("write failed");

    // Read back
    let data = storage.read("a/b/c/deep.txt").await.expect("read failed");
    assert_eq!(&data[..], b"deep content");

    // Clean up
    storage
        .delete("a/b/c/deep.txt")
        .await
        .expect("delete failed");
}

#[tokio::test]
async fn test_s3_list() {
    if !minio_available().await {
        eprintln!("Skipping S3 test: MinIO not available");
        return;
    }

    ensure_bucket_exists()
        .await
        .expect("Failed to ensure bucket exists");

    let storage = create_test_storage("test-list").await;

    // Create test files
    storage
        .write("file1.txt", Bytes::from("1"))
        .await
        .expect("write failed");
    storage
        .write("file2.txt", Bytes::from("2"))
        .await
        .expect("write failed");
    storage
        .write("subdir/file3.txt", Bytes::from("3"))
        .await
        .expect("write failed");

    // List root
    let entries = storage.list("").await.expect("list failed");

    let names: Vec<_> = entries.iter().map(|e| e.path.as_str()).collect();
    assert!(
        names.contains(&"file1.txt"),
        "Expected file1.txt in {:?}",
        names
    );
    assert!(
        names.contains(&"file2.txt"),
        "Expected file2.txt in {:?}",
        names
    );
    assert!(names.contains(&"subdir"), "Expected subdir in {:?}", names);

    // Check is_dir flags
    let subdir = entries.iter().find(|e| e.path == "subdir");
    assert!(subdir.is_some(), "subdir entry not found");
    assert!(
        subdir.unwrap().is_dir,
        "subdir should be marked as directory"
    );

    // Clean up
    storage.delete("file1.txt").await.ok();
    storage.delete("file2.txt").await.ok();
    storage.delete("subdir/file3.txt").await.ok();
}

#[tokio::test]
async fn test_s3_list_recursive() {
    if !minio_available().await {
        eprintln!("Skipping S3 test: MinIO not available");
        return;
    }

    ensure_bucket_exists()
        .await
        .expect("Failed to ensure bucket exists");

    let storage = create_test_storage("test-list-recursive").await;

    // Create nested structure
    storage
        .write("a/file1.txt", Bytes::from("1"))
        .await
        .expect("write failed");
    storage
        .write("a/b/file2.txt", Bytes::from("2"))
        .await
        .expect("write failed");
    storage
        .write("a/b/c/file3.txt", Bytes::from("3"))
        .await
        .expect("write failed");

    // List recursively
    let entries = storage
        .list_recursive("a")
        .await
        .expect("list_recursive failed");

    let paths: Vec<_> = entries.iter().map(|e| e.path.as_str()).collect();
    assert!(
        paths.contains(&"file1.txt"),
        "Expected file1.txt in {:?}",
        paths
    );
    assert!(paths.contains(&"b"), "Expected b in {:?}", paths);
    assert!(
        paths.contains(&"b/file2.txt"),
        "Expected b/file2.txt in {:?}",
        paths
    );
    assert!(paths.contains(&"b/c"), "Expected b/c in {:?}", paths);
    assert!(
        paths.contains(&"b/c/file3.txt"),
        "Expected b/c/file3.txt in {:?}",
        paths
    );

    // Clean up
    storage.delete("a/file1.txt").await.ok();
    storage.delete("a/b/file2.txt").await.ok();
    storage.delete("a/b/c/file3.txt").await.ok();
}

#[tokio::test]
async fn test_s3_not_found() {
    if !minio_available().await {
        eprintln!("Skipping S3 test: MinIO not available");
        return;
    }

    ensure_bucket_exists()
        .await
        .expect("Failed to ensure bucket exists");

    let storage = create_test_storage("test-notfound").await;

    let result = storage.read("nonexistent.txt").await;
    assert!(matches!(result, Err(StorageError::NotFound(_))));

    let result = storage.metadata("nonexistent.txt").await;
    assert!(matches!(result, Err(StorageError::NotFound(_))));
}

#[tokio::test]
async fn test_s3_signed_url() {
    if !minio_available().await {
        eprintln!("Skipping S3 test: MinIO not available");
        return;
    }

    ensure_bucket_exists()
        .await
        .expect("Failed to ensure bucket exists");

    let storage = create_test_storage("test-signed").await;

    // Write a file
    storage
        .write("signed-test.txt", Bytes::from("signed content"))
        .await
        .expect("write failed");

    // Get signed URL
    let url = storage
        .signed_url("signed-test.txt", Duration::from_secs(3600))
        .await;

    assert!(url.is_some(), "Expected signed URL");
    let url = url.unwrap();
    assert!(
        url.contains("X-Amz-Signature"),
        "URL should contain signature"
    );
    assert!(
        url.contains("test-bucket"),
        "URL should contain bucket name"
    );

    // Fetch the URL and verify content
    let client = reqwest::Client::new();
    let response = client
        .get(&url)
        .send()
        .await
        .expect("Failed to fetch signed URL");
    assert!(
        response.status().is_success(),
        "Signed URL should return success"
    );

    let body = response.text().await.expect("Failed to read body");
    assert_eq!(body, "signed content");

    // Clean up
    storage.delete("signed-test.txt").await.ok();
}

#[tokio::test]
async fn test_s3_read_to_string() {
    if !minio_available().await {
        eprintln!("Skipping S3 test: MinIO not available");
        return;
    }

    ensure_bucket_exists()
        .await
        .expect("Failed to ensure bucket exists");

    let storage = create_test_storage("test-string").await;

    // Write text file
    storage
        .write("text.txt", Bytes::from("Hello, UTF-8 World! 🌍"))
        .await
        .expect("write failed");

    // Read as string
    let content = storage
        .read_to_string("text.txt")
        .await
        .expect("read_to_string failed");
    assert_eq!(content, "Hello, UTF-8 World! 🌍");

    // Clean up
    storage.delete("text.txt").await.ok();
}

#[tokio::test]
async fn test_s3_supports_redirect() {
    if !minio_available().await {
        eprintln!("Skipping S3 test: MinIO not available");
        return;
    }

    ensure_bucket_exists()
        .await
        .expect("Failed to ensure bucket exists");

    let storage = create_test_storage("test-redirect").await;

    assert!(storage.supports_redirect(), "S3 should support redirect");
    assert_eq!(storage.storage_type(), "s3");
}
