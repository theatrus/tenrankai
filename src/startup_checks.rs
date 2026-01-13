use crate::{Config, storage::{StorageUrl, create_storage_from_url}};
use std::path::Path;
use thiserror::Error;
use tracing::{error, info, warn};

#[derive(Debug, Error)]
pub enum StartupCheckError {
    #[error("Failed to create cache directory: {0}")]
    CacheDirectoryCreationFailed(#[from] std::io::Error),

    #[error("Static files directory does not exist")]
    StaticDirectoryMissing,

    #[error("Gallery source directory does not exist: {0}")]
    GallerySourceDirectoryMissing(String),

    #[error("Required file missing: {0}")]
    RequiredFileMissing(String),
}

pub async fn perform_startup_checks(config: &Config) -> Result<(), Vec<StartupCheckError>> {
    let mut errors = Vec::new();

    info!("Performing startup checks...");

    // Check cache directories for all galleries
    if let Some(galleries) = &config.galleries {
        for gallery_config in galleries {
            // Parse the cache_directory as a storage URL
            match StorageUrl::parse(&gallery_config.cache_directory) {
                Ok(StorageUrl::Filesystem { path }) => {
                    // Filesystem cache - check/create directory
                    if !path.exists() {
                        info!(
                            "Cache directory for gallery '{}' does not exist, creating: {:?}",
                            gallery_config.name, path
                        );
                        if let Err(e) = tokio::fs::create_dir_all(&path).await {
                            error!(
                                "Failed to create cache directory for gallery '{}': {}",
                                gallery_config.name, e
                            );
                            errors.push(StartupCheckError::CacheDirectoryCreationFailed(e));
                        } else {
                            info!(
                                "Cache directory for gallery '{}' created successfully",
                                gallery_config.name
                            );
                        }
                    } else {
                        info!(
                            "Cache directory for gallery '{}' exists: {:?}",
                            gallery_config.name, path
                        );
                    }
                }
                Ok(StorageUrl::S3 { bucket, prefix, .. }) => {
                    // S3 cache - no directory creation needed
                    info!(
                        "Cache for gallery '{}' is S3 storage: s3://{}/{}",
                        gallery_config.name, bucket, prefix
                    );
                }
                Err(e) => {
                    warn!(
                        "Could not parse cache_directory for gallery '{}': {} (treating as filesystem path)",
                        gallery_config.name, e
                    );
                    // Fallback to treating as filesystem path
                    let cache_dir = Path::new(&gallery_config.cache_directory);
                    if !cache_dir.exists() {
                        info!(
                            "Cache directory for gallery '{}' does not exist, creating: {:?}",
                            gallery_config.name, cache_dir
                        );
                        if let Err(e) = tokio::fs::create_dir_all(cache_dir).await {
                            error!(
                                "Failed to create cache directory for gallery '{}': {}",
                                gallery_config.name, e
                            );
                            errors.push(StartupCheckError::CacheDirectoryCreationFailed(e));
                        } else {
                            info!(
                                "Cache directory for gallery '{}' created successfully",
                                gallery_config.name
                            );
                        }
                    } else {
                        info!(
                            "Cache directory for gallery '{}' exists: {:?}",
                            gallery_config.name, cache_dir
                        );
                    }
                }
            }
        }
    }

    // Check static files directories (filesystem paths only, not S3 URLs)
    for (index, static_url) in config.static_files.directories.iter().enumerate() {
        // Skip S3 URLs - they are checked at runtime
        if static_url.starts_with("s3://") {
            info!(
                "Static files directory {} is S3 storage: {}",
                index, static_url
            );
            continue;
        }

        let static_dir = std::path::Path::new(static_url);
        if !static_dir.exists() {
            warn!(
                "Static files directory {} does not exist: {:?}",
                index, static_dir
            );
            if index == 0 {
                // Only error if the first directory doesn't exist
                errors.push(StartupCheckError::StaticDirectoryMissing);
            }
        } else {
            info!("Static files directory {} exists: {:?}", index, static_dir);
        }
    }

    // Check for required files across all static directories (filesystem only)
    let required_files = vec!["DejaVuSans.ttf"];
    for file in required_files {
        let mut file_found = false;
        for static_url in &config.static_files.directories {
            // Skip S3 URLs
            if static_url.starts_with("s3://") {
                continue;
            }

            let static_dir = std::path::Path::new(static_url);
            let file_path = static_dir.join(file);
            if file_path.exists() {
                info!("Required file found: {:?}", file_path);
                file_found = true;
                break;
            }
        }
        if !file_found {
            warn!("Required file missing in all static directories: {}", file);
            errors.push(StartupCheckError::RequiredFileMissing(file.to_string()));
        }
    }

    // Check gallery source directories
    if let Some(galleries) = &config.galleries {
        for gallery_config in galleries {
            // Parse the source_directory as a storage URL
            match StorageUrl::parse(&gallery_config.source_directory) {
                Ok(StorageUrl::Filesystem { path }) => {
                    // Filesystem source - check directory exists
                    if !path.exists() {
                        error!(
                            "Gallery '{}' source directory does not exist: {:?}",
                            gallery_config.name, path
                        );
                        errors.push(StartupCheckError::GallerySourceDirectoryMissing(
                            gallery_config.source_directory.clone(),
                        ));
                    } else {
                        info!(
                            "Gallery '{}' source directory exists: {:?}",
                            gallery_config.name, path
                        );

                        // Check if directory is readable
                        match tokio::fs::read_dir(&path).await {
                            Ok(_) => info!(
                                "Gallery '{}' source directory is accessible",
                                gallery_config.name
                            ),
                            Err(e) => {
                                error!(
                                    "Gallery '{}' source directory is not accessible: {}",
                                    gallery_config.name, e
                                );
                                errors.push(StartupCheckError::GallerySourceDirectoryMissing(
                                    gallery_config.source_directory.clone(),
                                ));
                            }
                        }
                    }
                }
                Ok(StorageUrl::S3 { bucket, prefix, .. }) => {
                    // S3 source - verify connectivity using storage abstraction
                    info!(
                        "Gallery '{}' source is S3 storage: s3://{}/{}",
                        gallery_config.name, bucket, prefix
                    );

                    // Create storage and verify it's accessible
                    match create_storage_from_url(&gallery_config.source_directory).await {
                        Ok(storage) => {
                            // Try to list the root to verify connectivity
                            match storage.list("").await {
                                Ok(_) => info!(
                                    "Gallery '{}' S3 source directory is accessible",
                                    gallery_config.name
                                ),
                                Err(e) => {
                                    error!(
                                        "Gallery '{}' S3 source directory is not accessible: {}",
                                        gallery_config.name, e
                                    );
                                    errors.push(StartupCheckError::GallerySourceDirectoryMissing(
                                        gallery_config.source_directory.clone(),
                                    ));
                                }
                            }
                        }
                        Err(e) => {
                            error!(
                                "Failed to create storage for gallery '{}' source: {}",
                                gallery_config.name, e
                            );
                            errors.push(StartupCheckError::GallerySourceDirectoryMissing(
                                gallery_config.source_directory.clone(),
                            ));
                        }
                    }
                }
                Err(e) => {
                    warn!(
                        "Could not parse source_directory for gallery '{}': {} (treating as filesystem path)",
                        gallery_config.name, e
                    );
                    // Fallback to treating as filesystem path
                    let gallery_dir = Path::new(&gallery_config.source_directory);
                    if !gallery_dir.exists() {
                        error!(
                            "Gallery '{}' source directory does not exist: {:?}",
                            gallery_config.name, gallery_dir
                        );
                        errors.push(StartupCheckError::GallerySourceDirectoryMissing(
                            gallery_config.source_directory.clone(),
                        ));
                    } else {
                        info!(
                            "Gallery '{}' source directory exists: {:?}",
                            gallery_config.name, gallery_dir
                        );

                        // Check if directory is readable
                        match tokio::fs::read_dir(gallery_dir).await {
                            Ok(_) => info!(
                                "Gallery '{}' source directory is accessible",
                                gallery_config.name
                            ),
                            Err(e) => {
                                error!(
                                    "Gallery '{}' source directory is not accessible: {}",
                                    gallery_config.name, e
                                );
                                errors.push(StartupCheckError::GallerySourceDirectoryMissing(
                                    gallery_config.source_directory.clone(),
                                ));
                            }
                        }
                    }
                }
            }
        }
    }

    // Check templates directories (filesystem paths only, not S3 URLs)
    for (index, templates_url) in config.templates.directories.iter().enumerate() {
        // Skip S3 URLs - they are checked at runtime
        if templates_url.starts_with("s3://") {
            info!(
                "Templates directory {} is S3 storage: {}",
                index, templates_url
            );
            continue;
        }

        let templates_dir = std::path::Path::new(templates_url);
        if !templates_dir.exists() {
            warn!(
                "Templates directory {} does not exist: {:?}",
                index, templates_dir
            );
            if index == 0 {
                warn!("This may cause issues with page rendering");
            }
        } else {
            info!("Templates directory {} exists: {:?}", index, templates_dir);
        }
    }

    if errors.is_empty() {
        info!("All startup checks passed");
        Ok(())
    } else {
        error!("Startup checks failed with {} errors", errors.len());
        Err(errors)
    }
}
