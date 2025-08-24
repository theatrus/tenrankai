use super::{error::PostsError, types::*};
use chrono::{DateTime, NaiveDate, Utc};
use pulldown_cmark::{html, Options, Parser};
use serde::Deserialize;
use std::{
    collections::HashMap,
    path::Path,
    sync::Arc,
};
use tokio::sync::RwLock;
use tracing::{debug, error, info};

pub struct PostsManager {
    config: PostsConfig,
    posts: Arc<RwLock<HashMap<String, Post>>>,
    sorted_slugs: Arc<RwLock<Vec<String>>>,
}

impl PostsManager {
    pub fn new(config: PostsConfig) -> Self {
        Self {
            config,
            posts: Arc::new(RwLock::new(HashMap::new())),
            sorted_slugs: Arc::new(RwLock::new(Vec::new())),
        }
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

        let (metadata, markdown_content) = self.parse_front_matter(&content)?;

        let slug = self.generate_slug(path)?;

        let mut options = Options::empty();
        options.insert(Options::ENABLE_STRIKETHROUGH);
        options.insert(Options::ENABLE_TABLES);
        options.insert(Options::ENABLE_FOOTNOTES);
        options.insert(Options::ENABLE_SMART_PUNCTUATION);

        let parser = Parser::new_ext(&markdown_content, options);
        let mut html_content = String::new();
        html::push_html(&mut html_content, parser);

        Ok(Post {
            slug,
            path: path.to_path_buf(),
            title: metadata.title,
            summary: metadata.summary,
            date: metadata.date,
            content: markdown_content,
            html_content,
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
            return Ok(
                date.and_hms_opt(0, 0, 0)
                    .unwrap()
                    .and_local_timezone(Utc)
                    .unwrap(),
            );
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
        let posts = self.posts.read().await;
        posts.get(slug).cloned()
    }

    pub async fn get_total_pages(&self) -> usize {
        let slugs = self.sorted_slugs.read().await;
        slugs.len().div_ceil(self.config.posts_per_page)
    }

    pub fn get_config(&self) -> &PostsConfig {
        &self.config
    }
}