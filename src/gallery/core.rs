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
                // Check if this directory is hidden
                let folder_metadata = self.read_folder_metadata_full(&item_path).await;
                let is_hidden = folder_metadata
                    .as_ref()
                    .map(|m| m.config.hidden)
                    .unwrap_or(false);

                // Skip hidden directories in listings
                if is_hidden {
                    continue;
                }

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
                    is_new: false,
                });
            } else if self.is_image(&file_name) {
                // Found image
                let encoded_path = urlencoding::encode(&item_path);
                let thumbnail_url = format!(
                    "/{}/image/{}?size=thumbnail",
                    self.config.url_prefix.trim_start_matches('/'),
                    encoded_path
                );
                let gallery_url = format!(
                    "/{}/image/{}?size=gallery",
                    self.config.url_prefix.trim_start_matches('/'),
                    encoded_path
                );

                // Get metadata from cache if available
                let (dimensions, capture_date, modification_date) = {
                    let cache = self.metadata_cache.read().await;
                    if let Some(metadata) = cache.get(&item_path) {
                        (
                            Some(metadata.dimensions),
                            metadata.capture_date,
                            metadata.modification_date,
                        )
                    } else {
                        // If not in cache, try to extract it now
                        drop(cache);
                        match self.get_image_metadata_cached(&item_path).await {
                            Ok(metadata) => (
                                Some(metadata.dimensions),
                                metadata.capture_date,
                                metadata.modification_date,
                            ),
                            Err(_) => (None, None, None),
                        }
                    }
                };

                let is_new = self.is_new(modification_date);

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
                    is_new,
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

        // Pre-load hidden folder paths for this directory tree
        let hidden_folders = self.collect_hidden_folders(relative_path).await;

        for entry in WalkDir::new(full_path).min_depth(1).into_iter().flatten() {
            if entry.file_type().is_dir() {
                // Check if this subdirectory is hidden
                if let Ok(subdir_relative) =
                    entry.path().strip_prefix(&self.config.source_directory)
                {
                    let subdir_path = subdir_relative.to_string_lossy().replace('\\', "/");
                    if hidden_folders.contains(&subdir_path) {
                        // Skip this entire directory tree
                        continue;
                    }
                }
            } else if entry.file_type().is_file()
                && let Some(name) = entry.file_name().to_str()
                && self.is_image(name)
                && !name.starts_with('.')
            {
                // Check if this file is in a hidden directory
                if let Ok(file_relative) = entry.path().strip_prefix(&self.config.source_directory)
                {
                    let file_path = file_relative.to_string_lossy().replace('\\', "/");
                    let is_in_hidden = hidden_folders.iter().any(|hidden| {
                        file_path.starts_with(hidden) && file_path[hidden.len()..].starts_with('/')
                    });
                    if !is_in_hidden {
                        count += 1;
                    }
                }
            }
        }

        count
    }

    async fn collect_hidden_folders(&self, base_path: &str) -> Vec<String> {
        let mut hidden_folders = Vec::new();
        let full_base_path = self.config.source_directory.join(base_path);

        for entry in WalkDir::new(&full_base_path).into_iter().flatten() {
            if entry.file_type().is_dir()
                && let Ok(relative) = entry.path().strip_prefix(&self.config.source_directory)
            {
                let relative_str = relative.to_string_lossy().replace('\\', "/");

                // Check if _folder.md exists with hidden flag
                let folder_md_path = entry.path().join("_folder.md");
                if folder_md_path.exists()
                    && let Ok(content) = std::fs::read_to_string(&folder_md_path)
                    && content.trim_start().starts_with("+++")
                {
                    let parts: Vec<&str> = content.splitn(3, "+++").collect();
                    if parts.len() >= 3
                        && let Ok(config) = toml::from_str::<super::FolderConfig>(parts[1])
                        && config.hidden
                    {
                        hidden_folders.push(relative_str);
                    }
                }
            }
        }

        hidden_folders
    }

    async fn get_directory_preview_images(&self, relative_path: &str) -> Vec<String> {
        let full_path = self.config.source_directory.join(relative_path);
        let mut preview_images = Vec::new();

        // Get up to configured number of images for preview
        let max_preview_images = self.config.preview.max_images;

        // Pre-load hidden folder paths
        let hidden_folders = self.collect_hidden_folders(relative_path).await;

        for entry in WalkDir::new(&full_path)
            .min_depth(1)
            .max_depth(self.config.preview.max_depth)
            .into_iter()
            .flatten()
        {
            if preview_images.len() >= max_preview_images {
                break;
            }

            // Skip if in hidden directory
            if let Ok(entry_relative) = entry.path().strip_prefix(&self.config.source_directory) {
                let entry_path = entry_relative.to_string_lossy().replace('\\', "/");
                let is_in_hidden = hidden_folders.iter().any(|hidden| {
                    entry_path.starts_with(hidden)
                        && (entry_path.len() == hidden.len()
                            || entry_path[hidden.len()..].starts_with('/'))
                });
                if is_in_hidden {
                    continue;
                }
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
                    self.config.url_prefix.trim_start_matches('/'),
                    encoded_path
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

        let is_new = self.is_new(cached_metadata.modification_date);

        Ok(ImageInfo {
            name: StdPath::new(relative_path)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("unknown")
                .to_string(),
            path: relative_path.to_string(),
            url: format!(
                "/{}/image/{}",
                self.config.url_prefix.trim_start_matches('/'),
                encoded_path
            ),
            thumbnail_url: format!(
                "/{}/image/{}?size=thumbnail",
                self.config.url_prefix.trim_start_matches('/'),
                encoded_path
            ),
            gallery_url: format!(
                "/{}/image/{}?size=gallery",
                self.config.url_prefix.trim_start_matches('/'),
                encoded_path
            ),
            medium_url: format!(
                "/{}/image/{}?size=medium",
                self.config.url_prefix.trim_start_matches('/'),
                encoded_path
            ),
            description,
            camera_info: cached_metadata.camera_info,
            location_info: cached_metadata.location_info,
            file_size,
            dimensions,
            capture_date,
            is_new,
            color_profile: cached_metadata.color_profile,
        })
    }

    pub(crate) async fn read_folder_metadata(
        &self,
        folder_path: &str,
    ) -> (Option<String>, Option<String>) {
        let metadata = self.read_folder_metadata_full(folder_path).await;
        match metadata {
            Some(meta) => {
                let has_config_title = meta.config.title.is_some();
                let title = meta.config.title.or_else(|| {
                    // Try to extract title from markdown if not in config
                    meta.description_markdown
                        .lines()
                        .find(|line| line.trim().starts_with("# "))
                        .map(|line| line.trim_start_matches("# ").trim().to_string())
                });

                // Convert description markdown to HTML
                let description = if meta.description_markdown.trim().is_empty() {
                    None
                } else {
                    // If we found a title in the markdown, remove it from the description
                    let desc_content = if title.is_some() && !has_config_title {
                        meta.description_markdown
                            .lines()
                            .skip_while(|line| !line.trim().starts_with("# "))
                            .skip(1)
                            .collect::<Vec<_>>()
                            .join("\n")
                            .trim()
                            .to_string()
                    } else {
                        meta.description_markdown
                    };

                    if desc_content.is_empty() {
                        None
                    } else {
                        let parser = Parser::new(&desc_content);
                        let mut html_output = String::new();
                        html::push_html(&mut html_output, parser);
                        Some(html_output)
                    }
                };

                (title, description)
            }
            None => (None, None),
        }
    }

    pub(crate) async fn read_folder_metadata_full(
        &self,
        folder_path: &str,
    ) -> Option<super::FolderMetadata> {
        let folder_md_path = self
            .config
            .source_directory
            .join(folder_path)
            .join("_folder.md");

        match tokio::fs::read_to_string(&folder_md_path).await {
            Ok(content) => {
                // Check if content starts with TOML front matter
                if content.trim_start().starts_with("+++") {
                    // Parse TOML front matter
                    let parts: Vec<&str> = content.splitn(3, "+++").collect();

                    if parts.len() >= 3 {
                        let toml_content = parts[1];
                        let markdown_content = parts[2].trim().to_string();

                        match toml::from_str::<super::FolderConfig>(toml_content) {
                            Ok(config) => {
                                return Some(super::FolderMetadata {
                                    config,
                                    description_markdown: markdown_content,
                                });
                            }
                            Err(e) => {
                                debug!("Failed to parse folder TOML config: {}", e);
                            }
                        }
                    }
                }

                // No TOML front matter, treat entire content as markdown
                Some(super::FolderMetadata {
                    config: super::FolderConfig {
                        hidden: false,
                        title: None,
                    },
                    description_markdown: content,
                })
            }
            Err(_) => None,
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
                    modification_date: metadata.modification_date,
                    color_profile: metadata.color_profile.clone(),
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
            modification_date: metadata.modification_date,
            color_profile: metadata.color_profile,
        })
    }

    pub async fn get_gallery_preview(
        &self,
        max_items: usize,
    ) -> Result<Vec<GalleryItem>, GalleryError> {
        use rand::seq::SliceRandom;
        use rand::{Rng, rng};

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
            let mut rng = rng();
            // Add some extra randomness by shuffling multiple times
            for _ in 0..rng.random_range(1..4) {
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
                    // Check if this subdirectory is hidden
                    let folder_metadata = self.read_folder_metadata_full(&item_path).await;
                    let is_hidden = folder_metadata
                        .as_ref()
                        .map(|m| m.config.hidden)
                        .unwrap_or(false);

                    // Skip hidden directories
                    if is_hidden {
                        continue;
                    }

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
                    let (dimensions, capture_date, modification_date) = {
                        let cache = self.metadata_cache.read().await;
                        if let Some(metadata) = cache.get(&item_path) {
                            (
                                Some(metadata.dimensions),
                                metadata.capture_date,
                                metadata.modification_date,
                            )
                        } else {
                            // If not in cache, try to extract it now
                            drop(cache);
                            match self.get_image_metadata_cached(&item_path).await {
                                Ok(metadata) => (
                                    Some(metadata.dimensions),
                                    metadata.capture_date,
                                    metadata.modification_date,
                                ),
                                Err(_) => (None, None, None),
                            }
                        }
                    };

                    let is_new = self.is_new(modification_date);

                    let encoded_path = urlencoding::encode(&item_path);
                    let thumbnail_url = format!(
                        "/{}/image/{}?size=thumbnail",
                        self.config.url_prefix.trim_start_matches('/'),
                        encoded_path
                    );
                    let gallery_url = format!(
                        "/{}/image/{}?size=gallery",
                        self.config.url_prefix.trim_start_matches('/'),
                        encoded_path
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
                        is_new,
                    });
                }
            }

            items.extend(folder_items);
            Ok(())
        })
    }

    pub async fn build_breadcrumbs(&self, path: &str) -> Vec<BreadcrumbItem> {
        self.build_breadcrumbs_with_mode(path, false).await
    }

    pub async fn build_breadcrumbs_with_mode(
        &self,
        path: &str,
        all_clickable: bool,
    ) -> Vec<BreadcrumbItem> {
        let mut breadcrumbs = vec![BreadcrumbItem {
            name: "Gallery".to_string(),
            display_name: "Gallery".to_string(),
            path: "".to_string(),
            is_current: path.is_empty() && !all_clickable,
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
                    is_current: i == parts.len() - 1 && !all_clickable,
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
    pub modification_date: Option<SystemTime>,
    pub color_profile: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct BreadcrumbItem {
    pub name: String,
    pub display_name: String,
    pub path: String,
    pub is_current: bool,
}
