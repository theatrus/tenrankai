use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::time::SystemTime;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Post {
    pub slug: String,
    /// Relative path within the storage backend
    pub path: String,
    pub title: String,
    pub summary: String,
    pub date: DateTime<Utc>,
    pub content: String,
    pub html_content: String,
    #[serde(default)]
    pub categories: Vec<String>,
    /// Resolved URL for the post's hero image (frontmatter or first image in content)
    #[serde(default)]
    pub hero_image: Option<String>,
    /// True when the hero came from frontmatter (not derived from content),
    /// so the detail page should render it above the post body
    #[serde(default)]
    pub hero_image_explicit: bool,
    #[serde(default)]
    pub reading_time_minutes: usize,
    #[serde(skip)]
    pub last_modified: Option<SystemTime>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PostMetadata {
    pub title: String,
    pub summary: String,
    pub date: DateTime<Utc>,
    #[serde(default)]
    pub categories: Vec<String>,
    #[serde(default)]
    pub hero_image: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PostSummary {
    pub slug: String,
    pub title: String,
    pub summary: String,
    pub date: DateTime<Utc>,
    pub url: String,
    pub categories: Vec<String>,
    pub hero_image: Option<String>,
    pub reading_time_minutes: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CategoryInfo {
    /// Display label as written in frontmatter, or the declared `name`
    /// from _categories.md when the category is defined there
    pub name: String,
    /// URL-safe identifier used for filtering
    pub slug: String,
    pub count: usize,
    #[serde(default)]
    pub description: Option<String>,
    /// Posts in an archive category are hidden from the unfiltered index
    /// and main feed, but still listed on their category pages
    #[serde(default)]
    pub archive: bool,
}

/// Per-category options declared in _categories.md at the posts root,
/// keyed by category slug. Categories without an entry keep the defaults.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CategoryOptions {
    /// Canonical display label (otherwise taken from post frontmatter)
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub archive: bool,
    /// Chip ordering: lower weights sort first, undeclared weights last
    #[serde(default)]
    pub weight: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct PostsConfig {
    /// Storage URL for posts source (filesystem path or s3://...)
    pub source_directory: String,
    pub url_prefix: String,
    pub index_template: String,
    pub post_template: String,
    pub posts_per_page: usize,
    pub refresh_interval_minutes: Option<u64>,
    /// System-level permission configuration (folder _folder.md files override)
    pub permissions: crate::permissions::PermissionConfig,
}

impl Default for PostsConfig {
    fn default() -> Self {
        Self {
            source_directory: "posts".to_string(),
            url_prefix: String::from("/posts"),
            index_template: String::from("modules/posts_index.html.liquid"),
            post_template: String::from("modules/post_detail.html.liquid"),
            posts_per_page: 20,
            refresh_interval_minutes: None,
            permissions: Default::default(),
        }
    }
}

impl From<&crate::config::types::PostsSystemConfig> for PostsConfig {
    fn from(config: &crate::config::types::PostsSystemConfig) -> Self {
        Self {
            source_directory: config.source_directory.clone(),
            url_prefix: config.url_prefix.clone(),
            index_template: config.index_template.clone(),
            post_template: config.post_template.clone(),
            posts_per_page: config.posts_per_page,
            refresh_interval_minutes: config.refresh_interval_minutes,
            permissions: config.permissions.clone(),
        }
    }
}
