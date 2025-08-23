use super::{Gallery, GalleryError, GalleryItem, ImageInfo};
use pulldown_cmark::{Parser, html};
use std::path::Path as StdPath;
use std::time::SystemTime;
use tracing::debug;
use walkdir::WalkDir;

impl Gallery {
    pub async fn scan_directory(
        &self,
        relative_path: &str,
    ) -> Result<Vec<GalleryItem>, GalleryError> {
        let full_path = self.config.source_directory.join(relative_path);

        debug!("Scanning directory: {:?}", full_path);

        if !full_path.starts_with(&self.config.source_directory) {
            return Err(GalleryError::InvalidPath);
        }

        let mut items = Vec::new();

        let entries = tokio::fs::read_dir(&full_path).await?;

        let mut entries = entries;
        while let Some(entry) = entries.next_entry().await? {
            let file_name = entry.file_name().to_string_lossy().to_string();

            if file_name.starts_with('.') || file_name.ends_with(".md") {
                continue;
            }

            let metadata = entry.metadata().await?;
            let is_directory = metadata.is_dir();

            let item_path = if relative_path.is_empty() {
                file_name.clone()
            } else {
                format!("{}/{}", relative_path, file_name)
            };

            if is_directory {
                let item_count = self.count_images_in_directory(&item_path).await;
                let preview_images = self.get_directory_preview_images(&item_path).await;
                let (display_name, description) = self.read_folder_metadata(&item_path).await;
                items.push(GalleryItem {
                    name: file_name,
                    display_name,
                    description,
                    path: item_path,
                    parent_path: Some(relative_path.to_string()),
                    is_directory: true,
                    thumbnail_url: None,
                    gallery_url: None,
                    preview_images: Some(preview_images),
                    item_count: Some(item_count),
                    dimensions: None,
                    capture_date: None,
                });
            } else if self.is_image(&file_name) {
                // Found image
                let encoded_path = urlencoding::encode(&item_path);
                let thumbnail_url = format!(
                    "/{}/image/{}?size=thumbnail",
                    self.config.path_prefix, encoded_path
                );
                let gallery_url = format!(
                    "/{}/image/{}?size=gallery",
                    self.config.path_prefix, encoded_path
                );

                // Get metadata from cache if available
                let (dimensions, capture_date) = {
                    let cache = self.metadata_cache.read().await;
                    if let Some(metadata) = cache.get(&item_path) {
                        (Some(metadata.dimensions), metadata.capture_date)
                    } else {
                        // If not in cache, try to extract it now
                        drop(cache);
                        match self.get_image_metadata_cached(&item_path).await {
                            Ok(metadata) => (Some(metadata.dimensions), metadata.capture_date),
                            Err(_) => (None, None),
                        }
                    }
                };

                items.push(GalleryItem {
                    name: file_name,
                    display_name: None,
                    description: None,
                    path: item_path,
                    parent_path: Some(relative_path.to_string()),
                    is_directory: false,
                    thumbnail_url: Some(thumbnail_url),
                    gallery_url: Some(gallery_url),
                    preview_images: None,
                    item_count: None,
                    dimensions,
                    capture_date,
                });
            }
        }

        items.sort_by(|a, b| match (a.is_directory, b.is_directory) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => {
                // For directories, sort by display_name if available, otherwise by name
                if a.is_directory && b.is_directory {
                    let a_sort_name = a.display_name.as_ref().unwrap_or(&a.name);
                    let b_sort_name = b.display_name.as_ref().unwrap_or(&b.name);
                    a_sort_name.cmp(b_sort_name)
                } else {
                    // For files, sort by capture date first, then by name
                    match (&a.capture_date, &b.capture_date) {
                        (Some(a_date), Some(b_date)) => a_date.cmp(b_date),
                        (Some(_), None) => std::cmp::Ordering::Less,
                        (None, Some(_)) => std::cmp::Ordering::Greater,
                        (None, None) => a.name.cmp(&b.name),
                    }
                }
            }
        });

        debug!(
            "Found {} items total ({} directories, {} images)",
            items.len(),
            items.iter().filter(|i| i.is_directory).count(),
            items.iter().filter(|i| !i.is_directory).count()
        );

        Ok(items)
    }

    pub async fn list_directory(
        &self,
        path: &str,
        page: usize,
    ) -> Result<(Vec<GalleryItem>, Vec<GalleryItem>, usize), GalleryError> {
        let items = self.scan_directory(path).await?;

        // Separate directories and images
        let (directories, images): (Vec<_>, Vec<_>) =
            items.into_iter().partition(|item| item.is_directory);

        debug!(
            "list_directory: {} directories, {} images for path '{}'",
            directories.len(),
            images.len(),
            path
        );

        // Calculate pagination for images
        let total_images = images.len();
        let total_pages = total_images.div_ceil(self.config.images_per_page);
        let total_pages = total_pages.max(1); // At least 1 page

        let start = page * self.config.images_per_page;
        let end = ((page + 1) * self.config.images_per_page).min(total_images);

        let paginated_images = if start < total_images {
            images[start..end].to_vec()
        } else {
            Vec::new()
        };

        debug!(
            "Pagination: page={}, start={}, end={}, total_images={}, returning {} paginated images",
            page,
            start,
            end,
            total_images,
            paginated_images.len()
        );

        // Return all directories and paginated images
        Ok((directories, paginated_images, total_pages))
    }

    async fn count_images_in_directory(&self, relative_path: &str) -> usize {
        let full_path = self.config.source_directory.join(relative_path);
        let mut count = 0;

        for entry in WalkDir::new(full_path).min_depth(1).into_iter().flatten() {
            if entry.file_type().is_file()
                && let Some(name) = entry.file_name().to_str()
                && self.is_image(name)
                && !name.starts_with('.')
            {
                count += 1;
            }
        }

        count
    }

    async fn get_directory_preview_images(&self, relative_path: &str) -> Vec<String> {
        let full_path = self.config.source_directory.join(relative_path);
        let mut preview_images = Vec::new();

        // Get up to configured number of images for preview
        let max_preview_images = self.config.preview.max_images;

        for entry in WalkDir::new(&full_path)
            .min_depth(1)
            .max_depth(self.config.preview.max_depth)
            .into_iter()
            .flatten()
        {
            if preview_images.len() >= max_preview_images {
                break;
            }

            if entry.file_type().is_file()
                && let Some(name) = entry.file_name().to_str()
                && self.is_image(name)
                && !name.starts_with('.')
                && let Ok(relative_to_source) =
                    entry.path().strip_prefix(&self.config.source_directory)
            {
                let path_string = relative_to_source.to_string_lossy().to_string();
                let encoded_path = urlencoding::encode(&path_string);
                let thumbnail_url = format!(
                    "/{}/image/{}?size=thumbnail",
                    self.config.path_prefix, encoded_path
                );
                preview_images.push(thumbnail_url);
            }
        }

        preview_images
    }

    pub async fn get_image_info(&self, relative_path: &str) -> Result<ImageInfo, GalleryError> {
        let full_path = self.config.source_directory.join(relative_path);

        if !full_path.starts_with(&self.config.source_directory) {
            return Err(GalleryError::InvalidPath);
        }

        // Get cached metadata (includes dimensions and file size)
        let cached_metadata = self.get_image_metadata_cached(relative_path).await?;
        let file_size = cached_metadata.file_size;
        let dimensions = cached_metadata.dimensions;

        let description = self.read_sidecar_markdown(relative_path).await;

        let encoded_path = urlencoding::encode(relative_path);

        // Format capture date if available
        let capture_date = cached_metadata.capture_date.and_then(|date| {
            match date.duration_since(SystemTime::UNIX_EPOCH) {
                Ok(duration) => {
                    let datetime = chrono::DateTime::<chrono::Utc>::from_timestamp(
                        duration.as_secs() as i64,
                        0,
                    )?;
                    Some(datetime.format("%B %d, %Y at %H:%M:%S").to_string())
                }
                Err(_) => None,
            }
        });

        Ok(ImageInfo {
            name: StdPath::new(relative_path)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("unknown")
                .to_string(),
            path: relative_path.to_string(),
            url: format!("/{}/image/{}", self.config.path_prefix, encoded_path),
            thumbnail_url: format!(
                "/{}/image/{}?size=thumbnail",
                self.config.path_prefix, encoded_path
            ),
            gallery_url: format!(
                "/{}/image/{}?size=gallery",
                self.config.path_prefix, encoded_path
            ),
            medium_url: format!(
                "/{}/image/{}?size=medium",
                self.config.path_prefix, encoded_path
            ),
            description,
            camera_info: cached_metadata.camera_info,
            location_info: cached_metadata.location_info,
            file_size,
            dimensions,
            capture_date,
        })
    }

    pub(crate) async fn read_folder_metadata(
        &self,
        folder_path: &str,
    ) -> (Option<String>, Option<String>) {
        let folder_md_path = self
            .config
            .source_directory
            .join(folder_path)
            .join("_folder.md");

        match tokio::fs::read_to_string(&folder_md_path).await {
            Ok(content) => {
                let mut title: Option<String> = None;
                let mut description_content = String::new();
                let mut found_title = false;

                for line in content.lines() {
                    let trimmed = line.trim();

                    // Look for the first title (# heading)
                    if !found_title && trimmed.starts_with("# ") {
                        title = Some(trimmed.trim_start_matches("# ").to_string());
                        found_title = true;
                        continue;
                    }

                    // Skip empty lines immediately after title
                    if found_title && trimmed.is_empty() && description_content.is_empty() {
                        continue;
                    }

                    // Collect description content (everything after the title)
                    if found_title {
                        if !description_content.is_empty() {
                            description_content.push('\n');
                        }
                        description_content.push_str(line);
                    }
                }

                // Convert description markdown to HTML if there's content
                let description = if description_content.trim().is_empty() {
                    None
                } else {
                    let parser = Parser::new(&description_content);
                    let mut html_output = String::new();
                    html::push_html(&mut html_output, parser);
                    Some(html_output)
                };

                (title, description)
            }
            Err(_) => (None, None),
        }
    }

    async fn read_sidecar_markdown(&self, image_path: &str) -> Option<String> {
        let path = StdPath::new(image_path);
        let stem = path.file_stem()?;
        let parent = path.parent()?;

        let md_filename = format!("{}.md", stem.to_str()?);
        let md_path = self.config.source_directory.join(parent).join(md_filename);

        match tokio::fs::read_to_string(&md_path).await {
            Ok(content) => {
                let parser = Parser::new(&content);
                let mut html_output = String::new();
                html::push_html(&mut html_output, parser);
                Some(html_output)
            }
            Err(_) => None,
        }
    }

    pub(crate) async fn get_image_metadata_cached(
        &self,
        relative_path: &str,
    ) -> Result<ImageMetadataWithSize, GalleryError> {
        // Check if we have cached metadata
        {
            let cache = self.metadata_cache.read().await;
            if let Some(metadata) = cache.get(relative_path) {
                // We have metadata, just need to add file size
                let full_path = self.config.source_directory.join(relative_path);
                let file_metadata = tokio::fs::metadata(&full_path).await?;

                return Ok(ImageMetadataWithSize {
                    dimensions: metadata.dimensions,
                    capture_date: metadata.capture_date,
                    camera_info: metadata.camera_info.clone(),
                    location_info: metadata.location_info.clone(),
                    file_size: file_metadata.len(),
                });
            }
        }

        // No cached metadata, extract it
        let full_path = self.config.source_directory.join(relative_path);
        let file_metadata = tokio::fs::metadata(&full_path).await?;
        let file_size = file_metadata.len();

        // Extract metadata
        let metadata = self.extract_image_metadata(&full_path).await?;

        // Cache it with tracking
        self.insert_metadata_with_tracking(relative_path.to_string(), metadata.clone())
            .await;

        Ok(ImageMetadataWithSize {
            dimensions: metadata.dimensions,
            capture_date: metadata.capture_date,
            camera_info: metadata.camera_info,
            location_info: metadata.location_info,
            file_size,
        })
    }

    pub async fn get_gallery_preview(
        &self,
        max_items: usize,
    ) -> Result<Vec<GalleryItem>, GalleryError> {
        use rand::seq::SliceRandom;
        use rand::{Rng, thread_rng};

        let mut all_items = Vec::new();

        // Recursively collect images up to max_depth
        self.collect_preview_items(
            "",
            &mut all_items,
            0,
            self.config.preview.max_depth,
            self.config.preview.max_per_folder,
        )
        .await?;

        // If we have more items than requested, randomly select a subset
        if all_items.len() > max_items {
            let mut rng = thread_rng();
            // Add some extra randomness by shuffling multiple times
            for _ in 0..rng.gen_range(1..4) {
                all_items.shuffle(&mut rng);
            }
            all_items.truncate(max_items);

            // Keep the random order - don't sort by date to ensure different results each time
        }

        Ok(all_items)
    }

    fn collect_preview_items<'a>(
        &'a self,
        path: &'a str,
        items: &'a mut Vec<GalleryItem>,
        current_depth: usize,
        max_depth: usize,
        max_per_folder: usize,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), GalleryError>> + Send + 'a>>
    {
        Box::pin(async move {
            if current_depth > max_depth {
                return Ok(());
            }

            let full_path = if path.is_empty() {
                self.config.source_directory.clone()
            } else {
                self.config.source_directory.join(path)
            };

            let mut dir_entries = tokio::fs::read_dir(&full_path).await?;
            let mut folder_items = Vec::new();

            while let Some(entry) = dir_entries.next_entry().await? {
                let file_name = entry.file_name().to_string_lossy().to_string();

                if file_name.starts_with('.') || file_name.ends_with(".md") {
                    continue;
                }

                let metadata = entry.metadata().await?;
                let item_path = if path.is_empty() {
                    file_name.clone()
                } else {
                    format!("{}/{}", path, file_name)
                };

                if metadata.is_dir() {
                    // Recursively collect from subdirectories
                    self.collect_preview_items(
                        &item_path,
                        items,
                        current_depth + 1,
                        max_depth,
                        max_per_folder,
                    )
                    .await?;
                } else if self.is_image(&file_name) && folder_items.len() < max_per_folder {
                    // Get metadata from cache if available
                    let (dimensions, capture_date) = {
                        let cache = self.metadata_cache.read().await;
                        if let Some(metadata) = cache.get(&item_path) {
                            (Some(metadata.dimensions), metadata.capture_date)
                        } else {
                            // If not in cache, try to extract it now
                            drop(cache);
                            match self.get_image_metadata_cached(&item_path).await {
                                Ok(metadata) => (Some(metadata.dimensions), metadata.capture_date),
                                Err(_) => (None, None),
                            }
                        }
                    };

                    let encoded_path = urlencoding::encode(&item_path);
                    let thumbnail_url = format!(
                        "/{}/image/{}?size=thumbnail",
                        self.config.path_prefix, encoded_path
                    );
                    let gallery_url = format!(
                        "/{}/image/{}?size=gallery",
                        self.config.path_prefix, encoded_path
                    );

                    folder_items.push(GalleryItem {
                        name: file_name,
                        display_name: None,
                        description: None,
                        path: item_path.clone(),
                        parent_path: Some(path.to_string()),
                        is_directory: false,
                        thumbnail_url: Some(thumbnail_url),
                        gallery_url: Some(gallery_url),
                        preview_images: None,
                        item_count: None,
                        dimensions,
                        capture_date,
                    });
                }
            }

            items.extend(folder_items);
            Ok(())
        })
    }

    pub async fn build_breadcrumbs(&self, path: &str) -> Vec<BreadcrumbItem> {
        let mut breadcrumbs = vec![BreadcrumbItem {
            name: "Gallery".to_string(),
            display_name: "Gallery".to_string(),
            path: "".to_string(),
            is_current: path.is_empty(),
        }];

        if !path.is_empty() {
            let parts: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
            let mut current_path = String::new();

            for (i, part) in parts.iter().enumerate() {
                if i > 0 {
                    current_path.push('/');
                }
                current_path.push_str(part);

                // Check if this folder has a custom display name
                let (display_name, _) = self.read_folder_metadata(&current_path).await;
                let display_name = display_name.unwrap_or_else(|| part.to_string());

                breadcrumbs.push(BreadcrumbItem {
                    name: part.to_string(),
                    display_name,
                    path: current_path.clone(),
                    is_current: i == parts.len() - 1,
                });
            }
        }

        breadcrumbs
    }
}

// Helper struct that includes file size
pub(crate) struct ImageMetadataWithSize {
    pub dimensions: (u32, u32),
    pub capture_date: Option<SystemTime>,
    pub camera_info: Option<super::CameraInfo>,
    pub location_info: Option<super::LocationInfo>,
    pub file_size: u64,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct BreadcrumbItem {
    pub name: String,
    pub display_name: String,
    pub path: String,
    pub is_current: bool,
}
