use super::{error::PostsError, types::*};
use crate::gallery::SharedGallery;
use crate::permissions::{PermissionConfig, PermissionResolver, RolePermissions};
use crate::storage::DynStorage;
use chrono::{DateTime, NaiveDate, Utc};
use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd, html};
use serde::Deserialize;
use std::{collections::HashMap, sync::Arc};
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

pub struct PostsManager {
    config: PostsConfig,
    /// Storage backend for posts (filesystem or S3)
    storage: DynStorage,
    posts: Arc<RwLock<HashMap<String, Post>>>,
    sorted_slugs: Arc<RwLock<Vec<String>>>,
    /// Per-directory permission overrides from _folder.md files, keyed by
    /// directory path relative to the posts root ("" for the root)
    folder_permissions: Arc<RwLock<HashMap<String, PermissionConfig>>>,
    /// Per-category options from _categories.md at the posts root, keyed by slug
    category_options: Arc<RwLock<HashMap<String, CategoryOptions>>>,
    galleries: Option<Arc<HashMap<String, SharedGallery>>>,
}

impl PostsManager {
    pub fn new(config: PostsConfig, storage: DynStorage) -> Self {
        Self {
            config,
            storage,
            posts: Arc::new(RwLock::new(HashMap::new())),
            sorted_slugs: Arc::new(RwLock::new(Vec::new())),
            folder_permissions: Arc::new(RwLock::new(HashMap::new())),
            category_options: Arc::new(RwLock::new(HashMap::new())),
            galleries: None,
        }
    }

    pub fn set_galleries(&mut self, galleries: Arc<HashMap<String, SharedGallery>>) {
        self.galleries = Some(galleries);
    }

    pub async fn refresh_posts(&self) -> Result<(), PostsError> {
        info!(
            "Refreshing posts from storage: {} (type: {})",
            self.config.source_directory,
            self.storage.storage_type()
        );

        let mut new_posts = HashMap::new();
        let mut new_folder_permissions = HashMap::new();
        let mut new_category_options = HashMap::new();

        // List all files recursively from storage
        let entries = self.storage.list_recursive("").await?;

        // Filter for markdown files and load each post
        for entry in entries {
            if entry.is_dir {
                continue;
            }

            let file_name = entry.path.rsplit('/').next().unwrap_or(&entry.path);

            // Files starting with underscore are metadata, not posts
            if file_name == "_folder.md" {
                match self.load_folder_permissions(&entry.path).await {
                    Ok(Some(config)) => {
                        let dir = entry
                            .path
                            .rsplit_once('/')
                            .map(|(dir, _)| dir.to_string())
                            .unwrap_or_default();
                        new_folder_permissions.insert(dir, config);
                    }
                    Ok(None) => {}
                    Err(e) => {
                        error!("Failed to load folder metadata {}: {}", entry.path, e);
                    }
                }
                continue;
            }
            if file_name.starts_with('_') {
                // Category definitions live only at the posts root
                if entry.path == "_categories.md" {
                    match self.load_category_options(&entry.path).await {
                        Ok(options) => new_category_options = options,
                        Err(e) => {
                            error!("Failed to load category options {}: {}", entry.path, e);
                        }
                    }
                } else if file_name == "_categories.md" {
                    warn!(
                        "Ignoring {}: _categories.md is only read at the posts root",
                        entry.path
                    );
                }
                continue;
            }

            // Check if it's a markdown file
            if entry.path.ends_with(".md") || entry.path.ends_with(".markdown") {
                match self.load_post(&entry.path).await {
                    Ok(post) => {
                        if post.slug == "category" || post.slug.starts_with("category/") {
                            warn!(
                                "Skipping post {}: the 'category' path segment is reserved \
                                 for category index URLs and the post would be unreachable",
                                entry.path
                            );
                            continue;
                        }
                        debug!("Loaded post: {}", post.slug);
                        new_posts.insert(post.slug.clone(), post);
                    }
                    Err(e) => {
                        error!("Failed to load post {}: {}", entry.path, e);
                    }
                }
            }
        }

        let mut sorted_slugs: Vec<String> = new_posts.keys().cloned().collect();
        sorted_slugs.sort_by(|a, b| {
            let post_a = &new_posts[a];
            let post_b = &new_posts[b];
            post_b.date.cmp(&post_a.date)
        });

        info!("Found {} posts", new_posts.len());

        let mut posts = self.posts.write().await;
        let mut slugs = self.sorted_slugs.write().await;
        let mut folder_perms = self.folder_permissions.write().await;
        let mut category_options = self.category_options.write().await;
        *posts = new_posts;
        *slugs = sorted_slugs;
        *folder_perms = new_folder_permissions;
        *category_options = new_category_options;

        Ok(())
    }

    /// Parse the [categories] tables from _categories.md TOML frontmatter,
    /// normalizing keys to slugs
    async fn load_category_options(
        &self,
        path: &str,
    ) -> Result<HashMap<String, CategoryOptions>, PostsError> {
        let content = self.storage.read_to_string(path).await?;

        let parts: Vec<&str> = content.splitn(3, "+++").collect();
        if parts.len() < 3 || !parts[0].trim().is_empty() {
            return Ok(HashMap::new());
        }

        #[derive(Deserialize)]
        struct CategoriesFrontMatter {
            #[serde(default)]
            categories: HashMap<String, CategoryOptions>,
        }

        let front_matter: CategoriesFrontMatter = toml_edit::de::from_str(parts[1])?;
        let mut options = HashMap::new();
        for (key, value) in front_matter.categories {
            let slug = Self::category_slug(&key);
            if slug.is_empty() {
                warn!("Ignoring category definition with empty slug: {:?}", key);
                continue;
            }
            options.insert(slug, value);
        }
        Ok(options)
    }

    /// A post is archived when any of its categories is flagged `archive`
    fn is_archived(options: &HashMap<String, CategoryOptions>, post: &Post) -> bool {
        post.categories.iter().any(|c| {
            options
                .get(&Self::category_slug(c))
                .is_some_and(|o| o.archive)
        })
    }

    /// Parse the [permissions] table from a _folder.md file's TOML frontmatter
    async fn load_folder_permissions(
        &self,
        path: &str,
    ) -> Result<Option<PermissionConfig>, PostsError> {
        let content = self.storage.read_to_string(path).await?;

        let parts: Vec<&str> = content.splitn(3, "+++").collect();
        if parts.len() < 3 || !parts[0].trim().is_empty() {
            return Ok(None);
        }

        #[derive(Deserialize)]
        struct FolderFrontMatter {
            #[serde(default)]
            permissions: Option<PermissionConfig>,
        }

        let front_matter: FolderFrontMatter = toml_edit::de::from_str(parts[1])?;
        Ok(front_matter.permissions)
    }

    /// Resolve permissions for a path within the posts tree (a post slug or a
    /// directory). The nearest _folder.md walking up the hierarchy overrides
    /// the system-level permission config, mirroring the gallery scheme.
    pub async fn resolve_permissions(&self, path: &str, username: Option<&str>) -> RolePermissions {
        let folder_perms = self.folder_permissions.read().await;
        let folder_config = Self::nearest_folder_config(&folder_perms, path);

        let resolver = PermissionResolver::new(&self.config.permissions, folder_config);
        let mut permissions = resolver
            .resolve_user_permissions(username)
            .unwrap_or_default();
        permissions.apply_owner_override();
        permissions
    }

    /// Find the closest _folder.md permission config at or above `path`
    fn nearest_folder_config<'a>(
        configs: &'a HashMap<String, PermissionConfig>,
        path: &str,
    ) -> Option<&'a PermissionConfig> {
        // The path may be a post slug; its directory is everything before the
        // last '/'. Walk from the deepest directory up to the root.
        let mut dir = match path.rsplit_once('/') {
            Some((dir, _)) => dir,
            None => "",
        };
        loop {
            if let Some(config) = configs.get(dir) {
                return Some(config);
            }
            match dir.rsplit_once('/') {
                Some((parent, _)) => dir = parent,
                None => {
                    if dir.is_empty() {
                        return None;
                    }
                    dir = "";
                }
            }
        }
    }

    pub fn start_background_refresh(posts_manager: Arc<PostsManager>, interval_minutes: u64) {
        tokio::spawn(async move {
            let mut interval =
                tokio::time::interval(tokio::time::Duration::from_secs(interval_minutes * 60));
            interval.tick().await; // Skip the first immediate tick

            loop {
                interval.tick().await;
                info!("Starting scheduled posts refresh");

                if let Err(e) = posts_manager.refresh_posts().await {
                    error!("Failed to refresh posts: {}", e);
                } else {
                    info!("Posts refresh completed successfully");
                }
            }
        });
    }

    async fn load_post(&self, path: &str) -> Result<Post, PostsError> {
        // Read content from storage
        let content = self.storage.read_to_string(path).await?;

        // Get file modification time from storage metadata
        let last_modified = match self.storage.metadata(path).await {
            Ok(meta) => meta.last_modified,
            Err(e) => {
                warn!("Could not get metadata for {}: {}", path, e);
                None
            }
        };

        let (metadata, markdown_content) = self.parse_front_matter(&content)?;

        let slug = self.generate_slug(path);

        let mut options = Options::empty();
        options.insert(Options::ENABLE_STRIKETHROUGH);
        options.insert(Options::ENABLE_TABLES);
        options.insert(Options::ENABLE_FOOTNOTES);
        options.insert(Options::ENABLE_SMART_PUNCTUATION);

        let parser = Parser::new_ext(&markdown_content, options);
        let (html_content, first_image) = self.process_markdown_with_gallery_refs(parser).await;

        let hero_image_explicit = metadata.hero_image.is_some();
        let hero_image = match &metadata.hero_image {
            Some(reference) => self.resolve_image_reference(reference).await,
            None => first_image,
        };

        let reading_time_minutes = (markdown_content.split_whitespace().count() / 220).max(1);

        Ok(Post {
            slug,
            path: path.to_string(),
            title: metadata.title,
            summary: metadata.summary,
            date: metadata.date,
            content: markdown_content,
            html_content,
            categories: metadata.categories,
            hero_image,
            hero_image_explicit,
            reading_time_minutes,
            last_modified,
        })
    }

    fn parse_front_matter(&self, content: &str) -> Result<(PostMetadata, String), PostsError> {
        let parts: Vec<&str> = content.splitn(3, "+++").collect();

        if parts.len() < 3 || !parts[0].trim().is_empty() {
            return Err(PostsError::InvalidFormat(
                "Post must start with +++ front matter delimiter".to_string(),
            ));
        }

        let toml_content = parts[1];
        let markdown_content = parts[2].trim().to_string();

        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct FrontMatter {
            title: String,
            summary: String,
            date: String,
            #[serde(default)]
            categories: Vec<String>,
            #[serde(default)]
            hero_image: Option<String>,
        }

        let front_matter: FrontMatter = toml_edit::de::from_str(toml_content)?;

        let date = Self::parse_date(&front_matter.date)?;

        let metadata = PostMetadata {
            title: front_matter.title,
            summary: front_matter.summary,
            date,
            categories: front_matter
                .categories
                .into_iter()
                .map(|c| c.trim().to_string())
                .filter(|c| !c.is_empty())
                .collect(),
            hero_image: front_matter.hero_image,
        };

        Ok((metadata, markdown_content))
    }

    /// Normalize a category label into a URL-safe slug used for filtering
    pub fn category_slug(name: &str) -> String {
        let mut slug = String::with_capacity(name.len());
        let mut last_dash = true;
        for c in name.chars() {
            if c.is_alphanumeric() {
                slug.extend(c.to_lowercase());
                last_dash = false;
            } else if !last_dash {
                slug.push('-');
                last_dash = true;
            }
        }
        while slug.ends_with('-') {
            slug.pop();
        }
        slug
    }

    pub fn parse_date(date_str: &str) -> Result<DateTime<Utc>, PostsError> {
        if let Ok(date) = DateTime::parse_from_rfc3339(date_str) {
            return Ok(date.with_timezone(&Utc));
        }

        if let Ok(date) = NaiveDate::parse_from_str(date_str, "%Y-%m-%d") {
            return Ok(date
                .and_hms_opt(0, 0, 0)
                .unwrap()
                .and_local_timezone(Utc)
                .unwrap());
        }

        Err(PostsError::DateParseError(format!(
            "Unable to parse date: {}",
            date_str
        )))
    }

    fn generate_slug(&self, path: &str) -> String {
        // Path is already relative to the storage root
        // Normalize path separators and remove extension
        let slug = path.replace('\\', "/");

        if let Some(slug) = slug.strip_suffix(".md") {
            slug.to_string()
        } else if let Some(slug) = slug.strip_suffix(".markdown") {
            slug.to_string()
        } else {
            slug
        }
    }

    /// Posts visible to the user (newest first), optionally filtered by
    /// category, windowed by skip/limit. The unfiltered view excludes
    /// archived posts; category views list everything in the category.
    async fn visible_posts(
        &self,
        category: Option<&str>,
        username: Option<&str>,
        skip: usize,
        limit: usize,
    ) -> Vec<Post> {
        let posts = self.posts.read().await;
        let slugs = self.sorted_slugs.read().await;
        let folder_perms = self.folder_permissions.read().await;
        let options = self.category_options.read().await;
        let mut visibility = HashMap::new();

        slugs
            .iter()
            .filter_map(|slug| posts.get(slug))
            .filter(|post| Self::matches_category(post, category))
            .filter(|post| category.is_some() || !Self::is_archived(&options, post))
            .filter(|post| self.dir_visible(&folder_perms, &mut visibility, &post.slug, username))
            .skip(skip)
            .take(limit)
            .cloned()
            .collect()
    }

    fn summarize(&self, post: Post) -> PostSummary {
        PostSummary {
            url: format!("{}/{}", self.config.url_prefix, post.slug),
            slug: post.slug,
            title: post.title,
            summary: post.summary,
            date: post.date,
            categories: post.categories,
            hero_image: post.hero_image,
            reading_time_minutes: post.reading_time_minutes,
        }
    }

    pub async fn get_posts_page(
        &self,
        page: usize,
        category: Option<&str>,
        username: Option<&str>,
    ) -> Vec<PostSummary> {
        let start = page * self.config.posts_per_page;

        self.visible_posts(category, username, start, self.config.posts_per_page)
            .await
            .into_iter()
            .map(|post| self.summarize(post))
            .collect()
    }

    /// All posts visible to anonymous users, newest first, including archived
    /// posts — the sitemap lists archived permalinks even though the
    /// unfiltered index hides them
    pub async fn get_public_post_summaries(&self) -> Vec<PostSummary> {
        let posts = self.posts.read().await;
        let slugs = self.sorted_slugs.read().await;
        let folder_perms = self.folder_permissions.read().await;
        let mut visibility = HashMap::new();

        slugs
            .iter()
            .filter_map(|slug| posts.get(slug))
            .filter(|post| self.dir_visible(&folder_perms, &mut visibility, &post.slug, None))
            .map(|post| self.summarize(post.clone()))
            .collect()
    }

    /// The most recent posts visible to the user, with full content — used
    /// for feed generation
    pub async fn get_recent_posts(
        &self,
        limit: usize,
        category: Option<&str>,
        username: Option<&str>,
    ) -> Vec<Post> {
        self.visible_posts(category, username, 0, limit).await
    }

    fn matches_category(post: &Post, category: Option<&str>) -> bool {
        match category {
            Some(wanted) => post
                .categories
                .iter()
                .any(|c| Self::category_slug(c) == wanted),
            None => true,
        }
    }

    /// Whether the user can view posts in the directory containing `path`,
    /// memoized per directory
    fn dir_visible(
        &self,
        configs: &HashMap<String, PermissionConfig>,
        memo: &mut HashMap<String, bool>,
        path: &str,
        username: Option<&str>,
    ) -> bool {
        let dir = path.rsplit_once('/').map(|(d, _)| d).unwrap_or("");
        if let Some(visible) = memo.get(dir) {
            return *visible;
        }

        let folder_config = Self::nearest_folder_config(configs, path);
        let resolver = PermissionResolver::new(&self.config.permissions, folder_config);
        let visible = resolver
            .resolve_user_permissions(username)
            .map(|p| p.can_view)
            .unwrap_or(false);

        memo.insert(dir.to_string(), visible);
        visible
    }

    /// All categories across posts visible to the user, with display label,
    /// slug, post count, and any options declared in _categories.md. Sorted
    /// by declared weight then label, with archive categories last.
    pub async fn get_categories(&self, username: Option<&str>) -> Vec<CategoryInfo> {
        let posts = self.posts.read().await;
        let folder_perms = self.folder_permissions.read().await;
        let options = self.category_options.read().await;
        let mut visibility = HashMap::new();

        let mut categories: HashMap<String, CategoryInfo> = HashMap::new();
        for post in posts.values() {
            if !self.dir_visible(&folder_perms, &mut visibility, &post.slug, username) {
                continue;
            }
            for name in &post.categories {
                let slug = Self::category_slug(name);
                if slug.is_empty() {
                    continue;
                }
                categories
                    .entry(slug.clone())
                    .or_insert_with(|| {
                        let opts = options.get(&slug);
                        CategoryInfo {
                            name: opts
                                .and_then(|o| o.name.clone())
                                .unwrap_or_else(|| name.clone()),
                            slug,
                            count: 0,
                            description: opts.and_then(|o| o.description.clone()),
                            archive: opts.is_some_and(|o| o.archive),
                        }
                    })
                    .count += 1;
            }
        }

        let mut result: Vec<CategoryInfo> = categories.into_values().collect();
        result.sort_by(|a, b| {
            let weight = |c: &CategoryInfo| {
                options
                    .get(&c.slug)
                    .and_then(|o| o.weight)
                    .unwrap_or(i64::MAX)
            };
            (a.archive, weight(a), a.name.to_lowercase()).cmp(&(
                b.archive,
                weight(b),
                b.name.to_lowercase(),
            ))
        });
        result
    }

    /// Options declared for a category slug in _categories.md, if any
    pub async fn get_category_options(&self, slug: &str) -> Option<CategoryOptions> {
        self.category_options.read().await.get(slug).cloned()
    }

    pub async fn get_post(&self, slug: &str) -> Option<Post> {
        // First check if the post needs reloading
        if let Some(post) = self.get_post_if_fresh(slug).await {
            return Some(post);
        }

        // Post is stale or doesn't exist, try to reload it
        if let Err(e) = self.reload_post_by_slug(slug).await {
            debug!("Failed to reload post {}: {}", slug, e);
        }

        // Return the post (either freshly loaded or existing)
        let posts = self.posts.read().await;
        posts.get(slug).cloned()
    }

    async fn get_post_if_fresh(&self, slug: &str) -> Option<Post> {
        let posts = self.posts.read().await;

        if let Some(post) = posts.get(slug) {
            // Check if the file has been modified since we loaded it
            if let Ok(meta) = self.storage.metadata(&post.path).await
                && let (Some(file_modified), Some(post_modified)) =
                    (meta.last_modified, post.last_modified)
                && file_modified <= post_modified
            {
                // Post is still fresh
                return Some(post.clone());
            }
        }

        None
    }

    async fn reload_post_by_slug(&self, slug: &str) -> Result<(), PostsError> {
        // Find the path for this slug
        let path = {
            let posts = self.posts.read().await;
            posts.get(slug).map(|p| p.path.clone())
        };

        if let Some(path) = path {
            // Reload the post
            let post = self.load_post(&path).await?;

            // Update the post in our cache
            let mut posts = self.posts.write().await;
            posts.insert(slug.to_string(), post);

            debug!("Reloaded post: {}", slug);
        }

        Ok(())
    }

    pub async fn get_total_pages(&self, category: Option<&str>, username: Option<&str>) -> usize {
        let posts = self.posts.read().await;
        let folder_perms = self.folder_permissions.read().await;
        let options = self.category_options.read().await;
        let mut visibility = HashMap::new();

        let count = posts
            .values()
            .filter(|post| Self::matches_category(post, category))
            .filter(|post| category.is_some() || !Self::is_archived(&options, post))
            .filter(|post| self.dir_visible(&folder_perms, &mut visibility, &post.slug, username))
            .count();
        count.div_ceil(self.config.posts_per_page)
    }

    pub fn get_config(&self) -> &PostsConfig {
        &self.config
    }

    /// Validate a slug for a new post: `/`-separated segments of letters,
    /// digits, hyphens, and underscores, where no segment starts with `_`.
    /// The first segment `category` is reserved for category index routes.
    pub fn validate_slug(slug: &str) -> Result<(), PostsError> {
        if slug.is_empty() {
            return Err(PostsError::InvalidSlug("slug cannot be empty".to_string()));
        }

        if slug == "category" || slug.starts_with("category/") {
            return Err(PostsError::InvalidSlug(
                "'category' is reserved for category index URLs".to_string(),
            ));
        }

        for segment in slug.split('/') {
            if segment.is_empty() {
                return Err(PostsError::InvalidSlug(format!(
                    "'{}' contains an empty path segment",
                    slug
                )));
            }
            if segment.starts_with('_') || segment.starts_with('.') {
                return Err(PostsError::InvalidSlug(format!(
                    "'{}' has a segment starting with '_' or '.'",
                    slug
                )));
            }
            if !segment
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
            {
                return Err(PostsError::InvalidSlug(format!(
                    "'{}' may only contain letters, digits, hyphens, and underscores",
                    slug
                )));
            }
        }

        Ok(())
    }

    /// Serialize front matter + markdown into the on-disk post format
    fn render_post_file(metadata: &PostMetadata, content: &str) -> Result<String, PostsError> {
        #[derive(serde::Serialize)]
        struct FrontMatter<'a> {
            title: &'a str,
            summary: &'a str,
            date: String,
            #[serde(skip_serializing_if = "<[_]>::is_empty")]
            categories: &'a [String],
            #[serde(skip_serializing_if = "Option::is_none")]
            hero_image: &'a Option<String>,
        }

        let front_matter = FrontMatter {
            title: &metadata.title,
            summary: &metadata.summary,
            date: metadata.date.to_rfc3339(),
            categories: &metadata.categories,
            hero_image: &metadata.hero_image,
        };

        let toml = toml_edit::ser::to_string(&front_matter)?;
        Ok(format!("+++\n{}+++\n\n{}\n", toml, content.trim_end()))
    }

    /// Create a new post at `slug`. Fails if a post with that slug exists.
    pub async fn create_post(
        &self,
        slug: &str,
        metadata: &PostMetadata,
        content: &str,
    ) -> Result<(), PostsError> {
        Self::validate_slug(slug)?;

        let path = format!("{}.md", slug);
        {
            let posts = self.posts.read().await;
            if posts.contains_key(slug) {
                return Err(PostsError::PostAlreadyExists(slug.to_string()));
            }
        }
        if self.storage.exists(&path).await.unwrap_or(false) {
            return Err(PostsError::PostAlreadyExists(slug.to_string()));
        }

        let file = Self::render_post_file(metadata, content)?;
        self.storage.write(&path, file.into()).await?;

        self.refresh_posts().await
    }

    /// Overwrite an existing post's metadata and content
    pub async fn update_post(
        &self,
        slug: &str,
        metadata: &PostMetadata,
        content: &str,
    ) -> Result<(), PostsError> {
        let path = {
            let posts = self.posts.read().await;
            posts
                .get(slug)
                .map(|p| p.path.clone())
                .ok_or_else(|| PostsError::PostNotFound(slug.to_string()))?
        };

        let file = Self::render_post_file(metadata, content)?;
        self.storage.write(&path, file.into()).await?;

        self.refresh_posts().await
    }

    /// Delete a post from storage
    pub async fn delete_post(&self, slug: &str) -> Result<(), PostsError> {
        let path = {
            let posts = self.posts.read().await;
            posts
                .get(slug)
                .map(|p| p.path.clone())
                .ok_or_else(|| PostsError::PostNotFound(slug.to_string()))?
        };

        self.storage.delete(&path).await?;

        self.refresh_posts().await
    }

    async fn process_markdown_with_gallery_refs<'a>(
        &self,
        parser: Parser<'a>,
    ) -> (String, Option<String>) {
        let mut events = Vec::new();
        let mut in_image = false;
        let mut current_image_alt = String::new();
        let mut current_image_url = String::new();
        let mut current_image_title = String::new();
        let mut first_image: Option<String> = None;

        for event in parser {
            match event {
                Event::Start(Tag::Image {
                    dest_url, title, ..
                }) => {
                    in_image = true;
                    current_image_alt.clear();
                    current_image_url = dest_url.to_string();
                    current_image_title = title.to_string();
                }
                Event::Text(text) if in_image => {
                    current_image_alt.push_str(&text);
                }
                Event::End(TagEnd::Image) => {
                    in_image = false;

                    // Check if this is a gallery reference
                    if current_image_alt.starts_with("gallery:")
                        && let Some((gallery_html, image_url)) = self
                            .process_gallery_reference(&current_image_alt, &current_image_url)
                            .await
                    {
                        if first_image.is_none() {
                            first_image = Some(image_url);
                        }
                        events.push(Event::Html(gallery_html.into()));
                        continue;
                    }

                    if first_image.is_none() {
                        first_image = Some(current_image_url.clone());
                    }

                    // Not a gallery reference, reconstruct the original image
                    events.push(Event::Start(Tag::Image {
                        link_type: pulldown_cmark::LinkType::Inline,
                        dest_url: current_image_url.clone().into(),
                        title: current_image_title.clone().into(),
                        id: "".into(),
                    }));
                    events.push(Event::Text(current_image_alt.clone().into()));
                    events.push(Event::End(TagEnd::Image));
                }
                _ => events.push(event),
            }
        }

        let mut html_output = String::new();
        html::push_html(&mut html_output, events.into_iter());
        (html_output, first_image)
    }

    /// Resolve a hero image reference: either a `gallery:name:path` reference
    /// (served at gallery size) or a plain URL passed through unchanged
    async fn resolve_image_reference(&self, reference: &str) -> Option<String> {
        if let Some(rest) = reference.strip_prefix("gallery:") {
            let (gallery_name, image_path) = rest.split_once(':')?;
            let galleries = self.galleries.as_ref()?;
            let gallery = galleries.get(gallery_name)?;
            let gallery_config = gallery.get_config();
            let encoded_path = urlencoding::encode(image_path);
            return Some(format!(
                "{}/_image/{}/gallery",
                gallery_config.url_prefix, encoded_path
            ));
        }

        Some(reference.to_string())
    }

    async fn process_gallery_reference(
        &self,
        alt_text: &str,
        size_hint: &str,
    ) -> Option<(String, String)> {
        // Parse gallery reference format: gallery:gallery_name:path/to/image.jpg
        let parts: Vec<&str> = alt_text.splitn(3, ':').collect();
        if parts.len() != 3 {
            return None;
        }

        let gallery_name = parts[1];
        let image_path = parts[2];

        // Determine size from the URL/hint (default to thumbnail)
        let size = match size_hint.to_lowercase().as_str() {
            "gallery" | "medium" | "large" => size_hint,
            _ => "thumbnail",
        };

        // Get the gallery
        let galleries = self.galleries.as_ref()?;
        let gallery = galleries.get(gallery_name)?;
        let gallery_config = gallery.get_config();

        // Generate URLs
        let encoded_path = urlencoding::encode(image_path);
        let image_url = format!(
            "{}/_image/{}/{}",
            gallery_config.url_prefix, encoded_path, size
        );
        let detail_url = format!("{}/detail/{}", gallery_config.url_prefix, encoded_path);

        // Generate HTML with proper link
        let html = format!(
            r#"<a href="{}" class="gallery-image-link">
                <img src="{}" alt="{}" loading="lazy" class="gallery-image gallery-image-{}" />
            </a>"#,
            detail_url,
            image_url,
            image_path.split('/').next_back().unwrap_or(image_path),
            size
        );

        Some((html, image_url))
    }
}
