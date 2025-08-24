use super::{error::PostsError, types::*};
use crate::gallery::SharedGallery;
use chrono::{DateTime, NaiveDate, Utc};
use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd, html};
use serde::Deserialize;
use std::{collections::HashMap, path::Path, sync::Arc};
use tokio::sync::RwLock;
use tracing::{debug, error, info};

pub struct PostsManager {
    config: PostsConfig,
    posts: Arc<RwLock<HashMap<String, Post>>>,
    sorted_slugs: Arc<RwLock<Vec<String>>>,
    galleries: Option<Arc<HashMap<String, SharedGallery>>>,
}

impl PostsManager {
    pub fn new(config: PostsConfig) -> Self {
        Self {
            config,
            posts: Arc::new(RwLock::new(HashMap::new())),
            sorted_slugs: Arc::new(RwLock::new(Vec::new())),
            galleries: None,
        }
    }

    pub fn set_galleries(&mut self, galleries: Arc<HashMap<String, SharedGallery>>) {
        self.galleries = Some(galleries);
    }

    pub async fn refresh_posts(&self) -> Result<(), PostsError> {
        info!(
            "Refreshing posts from directory: {:?}",
            self.config.source_directory
        );

        let mut new_posts = HashMap::new();
        self.scan_directory(&self.config.source_directory, &mut new_posts)
            .await?;

        let mut sorted_slugs: Vec<String> = new_posts.keys().cloned().collect();
        sorted_slugs.sort_by(|a, b| {
            let post_a = &new_posts[a];
            let post_b = &new_posts[b];
            post_b.date.cmp(&post_a.date)
        });

        info!("Found {} posts", new_posts.len());

        let mut posts = self.posts.write().await;
        let mut slugs = self.sorted_slugs.write().await;
        *posts = new_posts;
        *slugs = sorted_slugs;

        Ok(())
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

    async fn scan_directory(
        &self,
        dir: &Path,
        posts: &mut HashMap<String, Post>,
    ) -> Result<(), PostsError> {
        let entries = tokio::fs::read_dir(dir).await?;
        let mut entries = entries;

        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            let file_type = entry.file_type().await?;

            if file_type.is_dir() {
                Box::pin(self.scan_directory(&path, posts)).await?;
            } else if file_type.is_file()
                && let Some(extension) = path.extension()
                && (extension == "md" || extension == "markdown")
            {
                match self.load_post(&path).await {
                    Ok(post) => {
                        debug!("Loaded post: {}", post.slug);
                        posts.insert(post.slug.clone(), post);
                    }
                    Err(e) => {
                        error!("Failed to load post {:?}: {}", path, e);
                    }
                }
            }
        }

        Ok(())
    }

    async fn load_post(&self, path: &Path) -> Result<Post, PostsError> {
        let content = tokio::fs::read_to_string(path).await?;

        // Get file modification time
        let file_metadata = tokio::fs::metadata(path).await?;
        let last_modified = file_metadata.modified().ok();

        let (metadata, markdown_content) = self.parse_front_matter(&content)?;

        let slug = self.generate_slug(path)?;

        let mut options = Options::empty();
        options.insert(Options::ENABLE_STRIKETHROUGH);
        options.insert(Options::ENABLE_TABLES);
        options.insert(Options::ENABLE_FOOTNOTES);
        options.insert(Options::ENABLE_SMART_PUNCTUATION);

        let parser = Parser::new_ext(&markdown_content, options);
        let html_content = self.process_markdown_with_gallery_refs(parser).await;

        Ok(Post {
            slug,
            path: path.to_path_buf(),
            title: metadata.title,
            summary: metadata.summary,
            date: metadata.date,
            content: markdown_content,
            html_content,
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
        struct FrontMatter {
            title: String,
            summary: String,
            date: String,
        }

        let front_matter: FrontMatter = toml::from_str(toml_content)?;

        let date = self.parse_date(&front_matter.date)?;

        let metadata = PostMetadata {
            title: front_matter.title,
            summary: front_matter.summary,
            date,
        };

        Ok((metadata, markdown_content))
    }

    fn parse_date(&self, date_str: &str) -> Result<DateTime<Utc>, PostsError> {
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

    fn generate_slug(&self, path: &Path) -> Result<String, PostsError> {
        let relative_path = path
            .strip_prefix(&self.config.source_directory)
            .map_err(|_| {
                PostsError::InvalidFormat(format!(
                    "Path {:?} is not under source directory {:?}",
                    path, self.config.source_directory
                ))
            })?;

        let slug = relative_path
            .to_str()
            .ok_or_else(|| PostsError::InvalidFormat("Invalid UTF-8 in path".to_string()))?
            .replace('\\', "/");

        let slug = if let Some(slug) = slug.strip_suffix(".md") {
            slug.to_string()
        } else if let Some(slug) = slug.strip_suffix(".markdown") {
            slug.to_string()
        } else {
            slug
        };

        Ok(slug)
    }

    pub async fn get_posts_page(&self, page: usize) -> Vec<PostSummary> {
        let posts = self.posts.read().await;
        let slugs = self.sorted_slugs.read().await;

        let start = page * self.config.posts_per_page;
        let end = (start + self.config.posts_per_page).min(slugs.len());

        slugs[start..end]
            .iter()
            .filter_map(|slug| {
                posts.get(slug).map(|post| PostSummary {
                    slug: post.slug.clone(),
                    title: post.title.clone(),
                    summary: post.summary.clone(),
                    date: post.date,
                    url: format!("{}/{}", self.config.url_prefix, post.slug),
                })
            })
            .collect()
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
            if let Ok(metadata) = tokio::fs::metadata(&post.path).await
                && let (Ok(file_modified), Some(post_modified)) =
                    (metadata.modified(), post.last_modified)
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

    pub async fn get_total_pages(&self) -> usize {
        let slugs = self.sorted_slugs.read().await;
        slugs.len().div_ceil(self.config.posts_per_page)
    }

    pub fn get_config(&self) -> &PostsConfig {
        &self.config
    }

    async fn process_markdown_with_gallery_refs<'a>(&self, parser: Parser<'a>) -> String {
        let mut events = Vec::new();
        let mut in_image = false;
        let mut current_image_alt = String::new();
        let mut current_image_url = String::new();
        let mut current_image_title = String::new();

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
                        && let Some(gallery_html) = self
                            .process_gallery_reference(&current_image_alt, &current_image_url)
                            .await
                    {
                        events.push(Event::Html(gallery_html.into()));
                        continue;
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
        html_output
    }

    async fn process_gallery_reference(&self, alt_text: &str, size_hint: &str) -> Option<String> {
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
            "{}/image/{}?size={}",
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

        Some(html)
    }
}
