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

    // Check gallery source directories
    if let Some(galleries) = &config.galleries {
        for gallery_config in galleries {
            let gallery_dir = Path::new(&gallery_config.source_directory);
            if !gallery_dir.exists() {
                error!(
                    "Gallery '{}' source directory does not exist: {:?}",
                    gallery_config.name, gallery_dir
                );
                errors.push(StartupCheckError::GallerySourceDirectoryMissing(
                    gallery_config.name.clone(),
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
                            gallery_config.name.clone(),
                        ));
                    }
                }
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
