use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::{path::PathBuf, time::SystemTime};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Post {
    pub slug: String,
    pub path: PathBuf,
    pub title: String,
    pub summary: String,
    pub date: DateTime<Utc>,
    pub content: String,
    pub html_content: String,
    #[serde(skip)]
    pub last_modified: Option<SystemTime>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PostMetadata {
    pub title: String,
    pub summary: String,
    pub date: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PostSummary {
    pub slug: String,
    pub title: String,
    pub summary: String,
    pub date: DateTime<Utc>,
    pub url: String,
}

#[derive(Debug, Clone)]
pub struct PostsConfig {
    pub source_directory: PathBuf,
    pub url_prefix: String,
    pub index_template: String,
    pub post_template: String,
    pub posts_per_page: usize,
    pub refresh_interval_minutes: Option<u64>,
}

impl Default for PostsConfig {
    fn default() -> Self {
        Self {
            source_directory: PathBuf::from("posts"),
            url_prefix: String::from("/posts"),
            index_template: String::from("modules/posts_index.html.liquid"),
            post_template: String::from("modules/post_detail.html.liquid"),
            posts_per_page: 20,
            refresh_interval_minutes: None,
        }
    }
}
