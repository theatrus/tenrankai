use crate::Config;
use std::path::Path;
use thiserror::Error;
use tracing::{error, info, warn};

#[derive(Debug, Error)]
pub enum StartupCheckError {
    #[error("Failed to create cache directory: {0}")]
    CacheDirectoryCreationFailed(#[from] std::io::Error),

    #[error("Static files directory does not exist")]
    StaticDirectoryMissing,

    #[error("Gallery source directory does not exist")]
    GallerySourceDirectoryMissing,

    #[error("Required file missing: {0}")]
    RequiredFileMissing(String),
}

pub async fn perform_startup_checks(config: &Config) -> Result<(), Vec<StartupCheckError>> {
    let mut errors = Vec::new();

    info!("Performing startup checks...");

    // Check cache directory
    let cache_dir = Path::new(&config.gallery.cache_directory);
    if !cache_dir.exists() {
        info!("Cache directory does not exist, creating: {:?}", cache_dir);
        if let Err(e) = tokio::fs::create_dir_all(cache_dir).await {
            error!("Failed to create cache directory: {}", e);
            errors.push(StartupCheckError::CacheDirectoryCreationFailed(e));
        } else {
            info!("Cache directory created successfully");
        }
    } else {
        info!("Cache directory exists: {:?}", cache_dir);
    }

    // Check static files directory
    let static_dir = Path::new(&config.static_files.directory);
    if !static_dir.exists() {
        warn!("Static files directory does not exist: {:?}", static_dir);
        errors.push(StartupCheckError::StaticDirectoryMissing);
    } else {
        info!("Static files directory exists: {:?}", static_dir);

        // Check for required files in static directory
        let required_files = vec!["DejaVuSans.ttf"];
        for file in required_files {
            let file_path = static_dir.join(file);
            if !file_path.exists() {
                warn!("Required file missing: {:?}", file_path);
                errors.push(StartupCheckError::RequiredFileMissing(file.to_string()));
            } else {
                info!("Required file found: {:?}", file_path);
            }
        }
    }

    // Check gallery source directory
    let gallery_dir = Path::new(&config.gallery.source_directory);
    if !gallery_dir.exists() {
        error!("Gallery source directory does not exist: {:?}", gallery_dir);
        errors.push(StartupCheckError::GallerySourceDirectoryMissing);
    } else {
        info!("Gallery source directory exists: {:?}", gallery_dir);

        // Check if directory is readable
        match tokio::fs::read_dir(gallery_dir).await {
            Ok(_) => info!("Gallery source directory is accessible"),
            Err(e) => {
                error!("Gallery source directory is not accessible: {}", e);
                errors.push(StartupCheckError::GallerySourceDirectoryMissing);
            }
        }
    }

    // Check templates directory
    let templates_dir = Path::new(&config.templates.directory);
    if !templates_dir.exists() {
        warn!("Templates directory does not exist: {:?}", templates_dir);
        warn!("This may cause issues with page rendering");
    } else {
        info!("Templates directory exists: {:?}", templates_dir);
    }

    if errors.is_empty() {
        info!("All startup checks passed");
        Ok(())
    } else {
        error!("Startup checks failed with {} errors", errors.len());
        Err(errors)
    }
}
