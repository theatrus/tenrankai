use super::{Gallery, GalleryError, GalleryItem, ImageInfo};
use pulldown_cmark::{Parser, html};
use std::path::Path as StdPath;
use std::time::SystemTime;
use tracing::debug;

impl Gallery {
    pub async fn scan_directory(
        &self,
        relative_path: &str,
    ) -> Result<Vec<GalleryItem>, GalleryError> {
        self.scan_directory_with_user(relative_path, None).await
    }

    pub async fn scan_directory_with_user(
        &self,
        relative_path: &str,
        user: Option<&str>,
    ) -> Result<Vec<GalleryItem>, GalleryError> {
        debug!("Scanning directory: {:?}", relative_path);

        // Use cached folder data (cache is mandatory, populated on startup)
        let cached = self
            .get_cached_folder_data(relative_path)
            .await
            .ok_or_else(|| {
                tracing::debug!("Folder '{}' not found in cache", relative_path);
                GalleryError::NotFound(format!("Folder not found: {}", relative_path))
            })?;

        self.build_gallery_items_from_cache(relative_path, &cached, user)
            .await
    }

    /// Build gallery items from cached folder data (fast path - no S3 calls)
    async fn build_gallery_items_from_cache(
        &self,
        relative_path: &str,
        cached: &super::CachedFolderMetadata,
        user: Option<&str>,
    ) -> Result<Vec<GalleryItem>, GalleryError> {
        let mut items = Vec::new();

        // Add subdirectories from cache
        for subdir_name in &cached.subdirectories {
            let subdir_path = if relative_path.is_empty() {
                subdir_name.clone()
            } else {
                format!("{}/{}", relative_path, subdir_name)
            };

            // Get cached data for this subdirectory
            let subdir_cached = self.get_cached_folder_data(&subdir_path).await;

            let (display_name, description, _description_markdown) =
                if let Some(ref sc) = subdir_cached {
                    Self::extract_folder_display_info(sc.metadata.clone())
                } else {
                    (None, None, None)
                };

            let item_count = subdir_cached
                .as_ref()
                .map(|sc| sc.recursive_image_count)
                .unwrap_or(0);

            let preview_images: Vec<String> = subdir_cached
                .as_ref()
                .map(|sc| {
                    sc.preview_items
                        .iter()
                        .map(|p| p.thumbnail_url.clone())
                        .collect()
                })
                .unwrap_or_default();

            items.push(GalleryItem {
                name: subdir_name.clone(),
                display_name,
                description,
                path: subdir_path,
                file_path: None,
                parent_path: Some(relative_path.to_string()),
                is_directory: true,
                thumbnail_url: None,
                gallery_url: None,
                preview_images: Some(preview_images),
                item_count: Some(item_count),
                dimensions: None,
                capture_date: None,
                is_new: false,
                user_metadata: None,
            });
        }

        // Add images from cache
        // Use image_groups if available (shows only primary images), otherwise fall back to flat list
        let image_paths: Vec<&str> = if !cached.image_groups.is_empty() {
            // Only show primary images from each group
            cached
                .image_groups
                .iter()
                .map(|g| g.primary_path.as_str())
                .collect()
        } else {
            // Fall back to flat image list for backward compatibility
            cached.images.iter().map(|s| s.as_str()).collect()
        };

        for image_path in image_paths {
            // Get the indexed identifier for this image
            let url_identifier = {
                let indexer = self.image_indexer.read().await;
                indexer
                    .get_index(image_path)
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| urlencoding::encode(image_path).to_string())
            };

            let thumbnail_url = self.build_thumbnail_url(&url_identifier);
            let gallery_url = self.build_gallery_url(&url_identifier);

            // Get metadata from image cache
            let (dimensions, capture_date, modification_date) = {
                let cache = self.image_cache.read_all().await;
                if let Some(metadata) = cache.get(image_path) {
                    (
                        Some(metadata.dimensions),
                        metadata.capture_date,
                        metadata.modification_date,
                    )
                } else {
                    (None, None, None)
                }
            };

            let is_new = self.is_new(modification_date);

            // Get the display name from the indexer
            let display_name = {
                let indexer = self.image_indexer.read().await;
                indexer.get_display_name(image_path)
            };

            // Load user metadata based on permissions
            let user_metadata = if user.is_some() {
                let folder_metadata = cached.metadata.as_ref();
                let resolver = crate::permissions::PermissionResolver::new(
                    &self.config.permissions,
                    folder_metadata.map(|m| &m.config.permissions),
                );
                let permissions = resolver.resolve_user_permissions(user).unwrap_or_default();

                if permissions.can_read_metadata {
                    self.user_metadata_storage
                        .load(image_path)
                        .await
                        .ok()
                        .flatten()
                } else {
                    None
                }
            } else {
                None
            };

            items.push(GalleryItem {
                name: display_name,
                display_name: None,
                description: None,
                path: url_identifier,
                file_path: Some(image_path.to_string()),
                parent_path: Some(relative_path.to_string()),
                is_directory: false,
                thumbnail_url: Some(thumbnail_url),
                gallery_url: Some(gallery_url),
                preview_images: None,
                item_count: None,
                dimensions,
                capture_date,
                is_new,
                user_metadata,
            });
        }

        // Sort items
        self.sort_gallery_items(&mut items);

        debug!(
            "Built {} items from cache ({} directories, {} images)",
            items.len(),
            items.iter().filter(|i| i.is_directory).count(),
            items.iter().filter(|i| !i.is_directory).count()
        );

        Ok(items)
    }

    /// Sort gallery items: directories first, then by name/date
    fn sort_gallery_items(&self, items: &mut [GalleryItem]) {
        items.sort_by(|a, b| match (a.is_directory, b.is_directory) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => {
                if a.is_directory && b.is_directory {
                    let a_sort_name = a.display_name.as_ref().unwrap_or(&a.name);
                    let b_sort_name = b.display_name.as_ref().unwrap_or(&b.name);
                    a_sort_name.cmp(b_sort_name)
                } else {
                    match (&a.capture_date, &b.capture_date) {
                        (Some(a_date), Some(b_date)) => a_date.cmp(b_date),
                        (Some(_), None) => std::cmp::Ordering::Less,
                        (None, Some(_)) => std::cmp::Ordering::Greater,
                        (None, None) => a.name.cmp(&b.name),
                    }
                }
            }
        });
    }

    pub async fn list_directory(
        &self,
        path: &str,
        page: usize,
    ) -> Result<(Vec<GalleryItem>, Vec<GalleryItem>, usize), GalleryError> {
        self.list_directory_with_user(path, page, None).await
    }

    pub async fn list_directory_with_user(
        &self,
        path: &str,
        page: usize,
        user: Option<&str>,
    ) -> Result<(Vec<GalleryItem>, Vec<GalleryItem>, usize), GalleryError> {
        let items = self.scan_directory_with_user(path, user).await?;

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

    pub async fn get_image_info(&self, relative_path: &str) -> Result<ImageInfo, GalleryError> {
        self.get_image_info_with_user(relative_path, None).await
    }

    /// Find the image group that contains a given image path.
    /// Returns the group and whether this path is the primary image.
    ///
    /// TODO: For folders with many images, consider adding a HashMap<path, group_index>
    /// to CachedFolderMetadata for O(1) lookups instead of O(n) iteration.
    pub(crate) async fn find_image_group(
        &self,
        image_path: &str,
    ) -> Option<(super::ImageGroup, bool)> {
        let parent_path = if let Some(last_slash) = image_path.rfind('/') {
            &image_path[..last_slash]
        } else {
            ""
        };

        let cached = self.get_cached_folder_data(parent_path).await?;

        for group in &cached.image_groups {
            if group.primary_path == image_path {
                return Some((group.clone(), true));
            }
            if group.all_image_paths.contains(&image_path.to_string()) {
                return Some((group.clone(), false));
            }
        }

        None
    }

    pub async fn get_image_info_with_user(
        &self,
        relative_path: &str,
        user: Option<&str>,
    ) -> Result<ImageInfo, GalleryError> {
        // Security check - prevent path traversal attacks
        if relative_path.contains("..") || relative_path.starts_with('/') {
            return Err(GalleryError::InvalidPath);
        }

        // Get cached metadata (includes dimensions and file size)
        let cached_metadata = self.get_image_metadata_cached(relative_path).await?;
        let file_size = cached_metadata.file_size;
        let dimensions = cached_metadata.dimensions;

        // Extract title and description from user metadata and XMP
        let (title, description) = {
            // Build XMP sidecar path (replace extension with .xmp)
            let xmp_path = if let Some(dot_pos) = relative_path.rfind('.') {
                format!("{}.xmp", &relative_path[..dot_pos])
            } else {
                format!("{}.xmp", relative_path)
            };

            // Check for XMP sidecar file using storage
            let xmp_metadata = super::metadata_sources::read_xmp_metadata_from_storage(
                &self.source_storage,
                &xmp_path,
            )
            .await;

            // Load user metadata from storage (handles both .md and .toml sidecars with caching)
            match self
                .user_metadata_storage
                .load(relative_path)
                .await
                .ok()
                .flatten()
            {
                Some(user_metadata) => {
                    // Use title from user metadata if available
                    let title = user_metadata
                        .title
                        .clone()
                        .or_else(|| {
                            // Fall back to extracting title from description markdown content
                            user_metadata.description.as_ref().and_then(|desc| {
                                desc.lines()
                                    .find(|line| line.trim().starts_with("# "))
                                    .map(|line| line.trim_start_matches("# ").trim().to_string())
                            })
                        })
                        .or_else(|| {
                            // Fall back to XMP title
                            xmp_metadata.as_ref().and_then(|xmp| xmp.title.clone())
                        });

                    // Process markdown description to HTML, removing title if present
                    let description_html = user_metadata.description.as_ref().map(|description| {
                        let content_without_title = if title.is_some() && description.contains("# ")
                        {
                            description
                                .lines()
                                .skip_while(|line| !line.trim().starts_with("# "))
                                .skip(1)
                                .collect::<Vec<_>>()
                                .join("\n")
                        } else {
                            description.clone()
                        };

                        let parser = Parser::new(&content_without_title);
                        let mut html_output = String::new();
                        html::push_html(&mut html_output, parser);
                        html_output
                    });

                    (title, description_html)
                }
                None => {
                    // Fall back to XMP title/description if no user metadata exists
                    let title = xmp_metadata.as_ref().and_then(|xmp| xmp.title.clone());
                    let description = xmp_metadata
                        .as_ref()
                        .and_then(|xmp| xmp.description.clone());
                    (title, description)
                }
            }
        };

        // Get the indexed identifier for this image
        let url_identifier = {
            let indexer = self.image_indexer.read().await;
            indexer
                .get_index(relative_path)
                .map(|s| s.to_string())
                .unwrap_or_else(|| urlencoding::encode(relative_path).to_string())
        };

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

        // Get parent path for permission checking
        let parent_path = std::path::Path::new(relative_path)
            .parent()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default();

        // Get folder metadata to check permissions
        let folder_metadata = self.read_folder_metadata_full(&parent_path).await;

        // Create permission resolver
        let resolver = crate::permissions::PermissionResolver::new(
            &self.config.permissions,
            folder_metadata.as_ref().map(|m| &m.config.permissions),
        );

        // Resolve permissions for the user
        let permissions = match resolver.resolve_user_permissions(user) {
            Ok(perms) => perms,
            Err(_) => {
                // On error, use most restrictive permissions
                crate::permissions::RolePermissions {
                    can_view: true, // They can view if they got this far
                    can_see_location: false,
                    can_see_technical_details: false,
                    can_see_exact_dates: false,
                    can_download_medium: false,
                    can_download_large: false,
                    can_download_original: false,
                    can_download_gallery: false,
                    can_download_raw: false,
                    can_see_versions: false,
                    can_read_metadata: false,
                    can_edit_content: false,
                    can_add_comments: false,
                    can_edit_own_comments: false,
                    can_delete_own_comments: false,
                    can_set_picks: false,
                    can_add_tags: false,
                    can_edit_any_comments: false,
                    can_delete_any_comments: false,
                    can_use_zoom: false,
                    can_use_tile_zoom: false,
                    can_analyze_images: false,
                    can_see_ai_analysis: false,
                    can_see_ai_alt_text: false,
                    owner_access: false,
                }
            }
        };

        // Filter location info based on permissions
        let location_info = if permissions.can_see_location {
            cached_metadata.location_info
        } else {
            None
        };

        // Filter technical details based on permissions
        let (camera_info, color_profile) = if permissions.can_see_technical_details {
            (cached_metadata.camera_info, cached_metadata.color_profile)
        } else {
            (None, None)
        };

        // Load user metadata if the user has permission to see any part of it
        let user_metadata = if permissions.can_read_metadata || permissions.can_see_ai_analysis {
            match self.user_metadata_storage.load(relative_path).await {
                Ok(Some(mut metadata)) => {
                    // Filter metadata based on permissions
                    if !permissions.can_read_metadata {
                        // User can only see AI analysis, not other metadata
                        metadata.comments = vec![];
                        metadata.highlighted = false;
                        metadata.pick_status = None;
                        metadata.tags = vec![];
                        metadata.last_modified = None;
                        metadata.modified_by = None;
                    }
                    if !permissions.can_see_ai_analysis {
                        // User cannot see AI analysis
                        metadata.ai_keywords = vec![];
                        metadata.ai_alt_text = None;
                        metadata.ai_analyzed_at = None;
                    }
                    Some(metadata)
                }
                Ok(None) => None,
                Err(e) => {
                    debug!("Failed to load user metadata for {}: {}", relative_path, e);
                    None
                }
            }
        } else {
            None
        };

        // Look up the image group for RAW files and versions
        let (raw_files, versions, is_primary) =
            if let Some((group, is_primary)) = self.find_image_group(relative_path).await {
                // Include RAW files only if user has permission
                let raw_files = if permissions.can_download_raw && !group.raw_files.is_empty() {
                    Some(
                        group
                            .raw_files
                            .into_iter()
                            .map(|mut raw| {
                                raw.download_url =
                                    Some(format!("{}/_raw/{}", self.config.url_prefix, raw.path));
                                raw
                            })
                            .collect(),
                    )
                } else {
                    None
                };

                // Include versions only if user has permission
                // Build complete version list including the primary so users can navigate
                // between all versions from any version
                let versions = if permissions.can_see_versions {
                    let mut all_versions = group.versions.clone();

                    // Add the primary version to the list so it can be navigated to
                    // when viewing an older version
                    let primary_url_id = {
                        let indexer = self.image_indexer.read().await;
                        indexer
                            .get_index(&group.primary_path)
                            .map(|s| s.to_string())
                            .unwrap_or_else(|| urlencoding::encode(&group.primary_path).to_string())
                    };
                    let primary_version = super::ImageVersion {
                        path: group.primary_path.clone(),
                        version_number: super::grouping::extract_version_number(
                            std::path::Path::new(&group.primary_path)
                                .file_name()
                                .and_then(|n| n.to_str())
                                .unwrap_or(""),
                        ),
                        modification_date: None, // Will be filled from cache if needed
                        url_id: primary_url_id.clone(),
                        thumbnail_url: self.build_thumbnail_url(&primary_url_id),
                    };
                    all_versions.push(primary_version);

                    if all_versions.len() > 1 {
                        Some(all_versions)
                    } else {
                        None
                    }
                } else {
                    None
                };

                (raw_files, versions, is_primary)
            } else {
                (None, None, true) // No group found, assume primary
            };

        Ok(ImageInfo {
            name: StdPath::new(relative_path)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("unknown")
                .to_string(),
            title,
            path: relative_path.to_string(),
            url: self.build_image_base_url(&url_identifier),
            thumbnail_url: self.build_thumbnail_url(&url_identifier),
            gallery_url: self.build_gallery_url(&url_identifier),
            medium_url: self.build_medium_url(&url_identifier),
            description,
            camera_info,
            location_info,
            file_size,
            dimensions,
            capture_date,
            is_new,
            color_profile,
            user_metadata,
            raw_files,
            versions,
            is_primary,
        })
    }

    pub(crate) async fn read_folder_metadata(
        &self,
        folder_path: &str,
    ) -> (Option<String>, Option<String>) {
        let metadata = self.read_folder_metadata_full(folder_path).await;
        let (title, description, _markdown) = Self::extract_folder_display_info(metadata);
        (title, description)
    }

    /// Extract display name, description HTML, and description markdown from pre-fetched FolderMetadata.
    /// Use this when you already have the FolderMetadata to avoid duplicate S3 calls.
    /// Returns (title, description_html, description_markdown):
    /// - title: extracted from # Title in markdown (for display in <h2>)
    /// - description_html: HTML with title stripped (to avoid showing title twice)
    /// - description_markdown: full markdown including # Title (for editing)
    pub(crate) fn extract_folder_display_info(
        metadata: Option<super::FolderMetadata>,
    ) -> (Option<String>, Option<String>, Option<String>) {
        match metadata {
            Some(meta) => {
                // Title is always extracted from markdown (# Title), never from config
                let title = meta
                    .description_markdown
                    .lines()
                    .find(|line| line.trim().starts_with("# "))
                    .map(|line| line.trim_start_matches("# ").trim().to_string());

                // For display HTML, strip the title heading to avoid showing it twice
                let description = if meta.description_markdown.trim().is_empty() {
                    None
                } else {
                    // Remove title line from HTML rendering
                    let desc_for_html = if title.is_some() {
                        meta.description_markdown
                            .lines()
                            .skip_while(|line| !line.trim().starts_with("# "))
                            .skip(1)
                            .collect::<Vec<_>>()
                            .join("\n")
                            .trim()
                            .to_string()
                    } else {
                        meta.description_markdown.clone()
                    };

                    if desc_for_html.is_empty() {
                        None
                    } else {
                        let parser = Parser::new(&desc_for_html);
                        let mut html_output = String::new();
                        html::push_html(&mut html_output, parser);
                        Some(html_output)
                    }
                };

                // Return full markdown for editing (includes # Title)
                let description_markdown = if meta.description_markdown.trim().is_empty() {
                    None
                } else {
                    Some(meta.description_markdown)
                };

                (title, description, description_markdown)
            }
            None => (None, None, None),
        }
    }

    /// Get folder metadata (permissions, hidden status, title, description)
    /// Returns cached metadata only - cache is mandatory and populated on startup.
    pub(crate) async fn read_folder_metadata_full(
        &self,
        folder_path: &str,
    ) -> Option<super::FolderMetadata> {
        if let Some(cached) = self.folder_cache.get(folder_path).await {
            return cached.metadata;
        }

        // Cache miss - this should not happen after startup
        // Return None (no special permissions/metadata for this folder)
        tracing::debug!(
            "Folder metadata cache miss for '{}' - using default permissions",
            folder_path
        );
        None
    }

    /// Get the full cached folder data (metadata, contents, counts, previews)
    /// Returns None if not in cache - use this for fast directory listings
    pub(crate) async fn get_cached_folder_data(
        &self,
        folder_path: &str,
    ) -> Option<super::CachedFolderMetadata> {
        self.folder_cache.get(folder_path).await
    }

    /// Read folder metadata directly from storage (bypasses cache)
    /// Use read_folder_metadata_full() for cached access
    pub(crate) async fn read_folder_metadata_from_storage(
        &self,
        folder_path: &str,
    ) -> Option<super::FolderMetadata> {
        // Build the path to _folder.md using storage abstraction
        let folder_md_path = if folder_path.is_empty() {
            "_folder.md".to_string()
        } else {
            format!("{}/_folder.md", folder_path)
        };

        match self.source_storage.read_to_string(&folder_md_path).await {
            Ok(content) => {
                // Check if content starts with TOML front matter
                if content.trim_start().starts_with("+++") {
                    // Parse TOML front matter
                    let parts: Vec<&str> = content.splitn(3, "+++").collect();

                    if parts.len() >= 3 {
                        let toml_content = parts[1];
                        let markdown_content = parts[2].trim().to_string();

                        match toml_edit::de::from_str::<super::FolderConfig>(toml_content) {
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
                        permissions: Default::default(),
                    },
                    description_markdown: content,
                })
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
            let cache = self.image_cache.read_all().await;
            if let Some(metadata) = cache.get(relative_path) {
                // We have metadata, just need to add file size from storage
                let storage_metadata = self.source_storage.metadata(relative_path).await?;

                return Ok(ImageMetadataWithSize {
                    dimensions: metadata.dimensions,
                    capture_date: metadata.capture_date,
                    camera_info: metadata.camera_info.clone(),
                    location_info: metadata.location_info.clone(),
                    file_size: storage_metadata.size,
                    modification_date: metadata.modification_date,
                    color_profile: metadata.color_profile.clone(),
                });
            }
        }

        // No cached metadata, extract it using storage
        let storage_metadata = self.source_storage.metadata(relative_path).await?;
        let file_size = storage_metadata.size;
        let modification_date = storage_metadata.last_modified;

        // Extract metadata using storage
        let metadata = self
            .extract_image_metadata(relative_path, modification_date)
            .await?;

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
        self.get_gallery_preview_for_user(max_items, None).await
    }

    pub async fn get_gallery_preview_for_user(
        &self,
        max_items: usize,
        user: Option<&str>,
    ) -> Result<Vec<GalleryItem>, GalleryError> {
        use rand::seq::SliceRandom;
        use rand::{Rng, rng};

        // Use pre-computed preview items from root folder cache (mandatory)
        let cached = self.get_cached_folder_data("").await.ok_or_else(|| {
            tracing::warn!("Root folder cache miss - cache should be populated on startup");
            GalleryError::NotFound("Gallery root folder not found in cache".to_string())
        })?;

        // Check permission for root folder
        let resolver = crate::permissions::PermissionResolver::new(
            &self.config.permissions,
            cached.metadata.as_ref().map(|m| &m.config.permissions),
        );

        if let Ok(perms) = resolver.resolve_user_permissions(user)
            && perms.can_view
        {
            // Convert cached preview items to GalleryItem (minimal conversion)
            let mut items: Vec<GalleryItem> = cached
                .preview_items
                .iter()
                .map(|p| GalleryItem {
                    name: p.path.rsplit('/').next().unwrap_or(&p.path).to_string(),
                    display_name: None,
                    description: None,
                    path: p.url_id.clone(),
                    file_path: Some(p.path.clone()),
                    parent_path: Some(
                        p.path
                            .rfind('/')
                            .map(|pos| p.path[..pos].to_string())
                            .unwrap_or_default(),
                    ),
                    is_directory: false,
                    thumbnail_url: Some(p.thumbnail_url.clone()),
                    gallery_url: Some(p.gallery_url.clone()),
                    preview_images: None,
                    item_count: None,
                    dimensions: p.dimensions,
                    capture_date: None, // Not needed for preview
                    is_new: false,      // Not needed for preview
                    user_metadata: None,
                })
                .collect();

            // Shuffle items for variety, then truncate to requested count
            if !items.is_empty() {
                let mut rng = rng();
                for _ in 0..rng.random_range(1..4) {
                    items.shuffle(&mut rng);
                }
                if items.len() > max_items {
                    items.truncate(max_items);
                }
            }

            return Ok(items);
        }

        // No permission to view
        Ok(Vec::new())
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::FilesystemStorage;
    use std::sync::Arc;
    use tempfile::TempDir;
    use tokio::fs;

    fn create_test_storage(dir: &str) -> crate::storage::DynStorage {
        let path = std::path::PathBuf::from(dir);
        std::fs::create_dir_all(&path).ok();
        Arc::new(FilesystemStorage::new(path))
    }

    fn create_test_storage_from_path(path: &std::path::Path) -> crate::storage::DynStorage {
        std::fs::create_dir_all(path).ok();
        Arc::new(FilesystemStorage::new(path))
    }

    #[tokio::test]
    async fn test_folder_config_no_toml_defaults_to_false() {
        let temp_dir = TempDir::new().unwrap();
        let config = crate::GallerySystemConfig {
            name: "test".to_string(),
            source_directory: temp_dir.path().to_string_lossy().to_string(),
            ..Default::default()
        };
        let source_storage = create_test_storage_from_path(temp_dir.path());
        let cache_storage = create_test_storage(&config.cache_directory);
        let gallery = Gallery::new(config, source_storage, cache_storage);

        // Test folder with just markdown (no TOML front matter)
        let folder_path = temp_dir.path().join("markdown-only");
        fs::create_dir_all(&folder_path).await.unwrap();

        let folder_md_content = "# Markdown Only Gallery\n\nJust markdown content.";

        let folder_md_path = folder_path.join("_folder.md");
        fs::write(&folder_md_path, folder_md_content).await.unwrap();

        // Populate the folder cache first (mandatory for read_folder_metadata_full)
        gallery.refresh_folder_cache().await.unwrap();

        // Test reading the folder metadata
        let metadata = gallery.read_folder_metadata_full("markdown-only").await;
        assert!(
            metadata.is_some(),
            "metadata should be cached after refresh"
        );

        let metadata = metadata.unwrap();
        assert!(
            metadata
                .description_markdown
                .contains("Just markdown content.")
        );
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct BreadcrumbItem {
    pub name: String,
    pub display_name: String,
    pub path: String,
    pub is_current: bool,
}
