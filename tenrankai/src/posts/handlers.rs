use super::core::PostsManager;
use crate::{ApiResponse, api_response::no_cache_headers, site::ResolvedState};
use axum::{
    extract::{Path, Query},
    response::{Html, IntoResponse},
};
use chrono::{DateTime, Datelike, Utc};
use serde::Deserialize;
use tracing::error;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PostsQuery {
    page: Option<usize>,
    category: Option<String>,
}

fn format_date(date: &DateTime<Utc>) -> String {
    format!(
        "{} {}, {}",
        match date.month() {
            1 => "January",
            2 => "February",
            3 => "March",
            4 => "April",
            5 => "May",
            6 => "June",
            7 => "July",
            8 => "August",
            9 => "September",
            10 => "October",
            11 => "November",
            12 => "December",
            _ => "",
        },
        date.day(),
        date.year()
    )
}

fn category_url(url_prefix: &str, slug: &str) -> String {
    format!("{}/category/{}", url_prefix, slug)
}

fn category_objects(url_prefix: &str, categories: &[String]) -> Vec<liquid::Object> {
    categories
        .iter()
        .map(|name| {
            let slug = PostsManager::category_slug(name);
            liquid::object!({
                "name": name,
                "slug": slug.clone(),
                "url": category_url(url_prefix, &slug),
            })
        })
        .collect()
}

pub async fn posts_index_handler(
    ResolvedState(app_state): ResolvedState,
    Path(posts_name): Path<String>,
    auth: crate::login::OptionalAuth,
    Query(query): Query<PostsQuery>,
) -> axum::response::Response {
    // Legacy ?category= URLs redirect permanently to the canonical path form
    if let Some(category) = query.category.as_deref() {
        let slug = PostsManager::category_slug(category);
        if !slug.is_empty()
            && let Some(manager) = app_state.posts_managers().get(&posts_name)
        {
            let mut target = category_url(&manager.get_config().url_prefix, &slug);
            if let Some(page) = query.page.filter(|p| *p > 0) {
                target.push_str(&format!("?page={}", page));
            }
            return axum::response::Redirect::permanent(&target).into_response();
        }
    }

    render_posts_index(
        app_state,
        posts_name,
        None,
        query.page.unwrap_or(0),
        auth.username(),
    )
    .await
}

pub async fn posts_category_index_handler(
    ResolvedState(app_state): ResolvedState,
    Path((posts_name, category)): Path<(String, String)>,
    auth: crate::login::OptionalAuth,
    Query(query): Query<PostsQuery>,
) -> axum::response::Response {
    render_posts_index(
        app_state,
        posts_name,
        Some(category),
        query.page.unwrap_or(0),
        auth.username(),
    )
    .await
}

async fn render_posts_index(
    app_state: crate::AppState,
    posts_name: String,
    category: Option<String>,
    page: usize,
    username: Option<&str>,
) -> axum::response::Response {
    let category_filter = category
        .as_deref()
        .map(PostsManager::category_slug)
        .filter(|slug| !slug.is_empty());

    let posts_manager = match app_state.posts_managers().get(&posts_name) {
        Some(manager) => manager,
        None => {
            return ApiResponse::PostNotFound.into_response();
        }
    };

    let config = posts_manager.get_config();

    let root_permissions = posts_manager.resolve_permissions("", username).await;

    let posts_raw = posts_manager
        .get_posts_page(page, category_filter.as_deref(), username)
        .await;

    let posts: Vec<_> = posts_raw
        .into_iter()
        .map(|post| {
            liquid::object!({
                "slug": post.slug,
                "title": post.title,
                "summary": post.summary,
                "url": post.url,
                "date": post.date.to_rfc3339(),
                "date_formatted": format_date(&post.date),
                "categories": category_objects(&config.url_prefix, &post.categories),
                "hero_image": post.hero_image,
                "reading_time_minutes": post.reading_time_minutes,
            })
        })
        .collect();

    let total_pages = posts_manager
        .get_total_pages(category_filter.as_deref(), username)
        .await;

    let all_categories = posts_manager.get_categories(username).await;
    let active_category = category_filter.as_deref().and_then(|slug| {
        all_categories
            .iter()
            .find(|c| c.slug == slug)
            .map(|c| c.name.clone())
    });
    let categories: Vec<_> = all_categories
        .iter()
        .map(|c| {
            liquid::object!({
                "name": c.name,
                "slug": c.slug,
                "count": c.count,
                "url": category_url(&config.url_prefix, &c.slug),
                "active": Some(c.slug.as_str()) == category_filter.as_deref(),
            })
        })
        .collect();

    // Base URL for pagination links and og:url; the category form keeps the filter
    let page_base = match category_filter.as_deref() {
        Some(slug) => category_url(&config.url_prefix, slug),
        None => config.url_prefix.clone(),
    };
    let page_base_for_og = page_base.clone();

    let feed_url = match category_filter.as_deref() {
        Some(slug) => format!("{}/feed.xml", category_url(&config.url_prefix, slug)),
        None => format!("{}/feed.xml", config.url_prefix),
    };

    let base_url = app_state.base_url().unwrap_or("http://localhost:8080");

    let page_title = match &active_category {
        Some(name) => format!("{} – {}", capitalize(&posts_name), name),
        None => capitalize(&posts_name),
    };
    let meta_description = match &active_category {
        Some(name) => format!("Browse {} posts in {}", posts_name, name),
        None => format!("Browse {} posts", posts_name),
    };

    let globals = liquid::object!({
        "posts": posts,
        "posts_name": posts_name,
        "url_prefix": config.url_prefix,
        "can_edit": root_permissions.can_edit_content,
        "categories": categories,
        "active_category": active_category,
        "page_base": page_base,
        "feed_url": feed_url.clone(),
        "rss_feed_url": feed_url,
        "current_page": page,
        "total_pages": total_pages,
        "has_prev": page > 0,
        "has_next": page + 1 < total_pages,
        "prev_page": if page > 0 { page - 1 } else { 0 },
        "next_page": page + 1,
        "base_url": base_url,
        "page_title": page_title.clone(),
        "meta_description": meta_description.clone(),
        "og_title": page_title,
        "og_description": meta_description,
        "og_url": format!("{}{}", base_url, page_base_for_og),
        "og_type": "website",
    });

    match app_state
        .template_engine()
        .render_template(&config.index_template, globals)
        .await
    {
        Ok(html) => Html(html).into_response(),
        Err(e) => {
            error!("Template rendering error: {}", e);
            ApiResponse::TemplateRenderError.into_response()
        }
    }
}

pub async fn post_detail_handler(
    ResolvedState(app_state): ResolvedState,
    Path((posts_name, slug)): Path<(String, String)>,
    auth: crate::login::OptionalAuth,
) -> impl IntoResponse {
    let username = auth.username();

    let posts_manager = match app_state.posts_managers().get(&posts_name) {
        Some(manager) => manager,
        None => {
            return ApiResponse::PostNotFound.into_response();
        }
    };

    let permissions = posts_manager.resolve_permissions(&slug, username).await;
    if !permissions.can_view {
        return ApiResponse::PostNotFound.into_response();
    }

    let post = match posts_manager.get_post(&slug).await {
        Some(post) => post,
        None => {
            return ApiResponse::PostNotFound.into_response();
        }
    };

    let config = posts_manager.get_config();

    let base_url = app_state.base_url().unwrap_or("http://localhost:8080");

    let full_url = format!("{}{}/{}", base_url, config.url_prefix, post.slug);

    let date_formatted = post.date.format("%B %-d, %Y").to_string();

    let og_image = post.hero_image.as_deref().map(|url| {
        if url.starts_with('/') {
            format!("{}{}", base_url, url)
        } else {
            url.to_string()
        }
    });

    let globals = liquid::object!({
        "post": {
            "slug": post.slug,
            "title": post.title,
            "summary": post.summary,
            "date": post.date.to_rfc3339(),
            "date_formatted": date_formatted,
            "content": post.content,
            "html_content": post.html_content,
            "categories": category_objects(&config.url_prefix, &post.categories),
            "hero_image": post.hero_image,
            "reading_time_minutes": post.reading_time_minutes,
        },
        "posts_name": posts_name,
        "url_prefix": config.url_prefix,
        "can_edit": permissions.can_edit_content,
        "rss_feed_url": format!("{}/feed.xml", config.url_prefix),
        "base_url": base_url,
        "page_title": post.title.clone(),
        "meta_description": post.summary.clone(),
        "og_title": post.title.clone(),
        "og_description": post.summary.clone(),
        "og_url": full_url.clone(),
        "og_type": "article",
        "og_image": og_image.clone(),
        "twitter_title": post.title,
        "twitter_description": post.summary,
        "twitter_image": og_image,
        "article_published_time": post.date.to_rfc3339(),
        "share_url": full_url,
    });

    match app_state
        .template_engine()
        .render_template(&config.post_template, globals)
        .await
    {
        Ok(html) => Html(html).into_response(),
        Err(e) => {
            error!("Template rendering error: {}", e);
            ApiResponse::TemplateRenderError.into_response()
        }
    }
}

/// Number of items included in RSS feeds
const FEED_ITEM_LIMIT: usize = 20;

pub async fn posts_feed_handler(
    ResolvedState(app_state): ResolvedState,
    Path(posts_name): Path<String>,
    auth: crate::login::OptionalAuth,
) -> axum::response::Response {
    render_posts_feed(app_state, posts_name, None, auth.username()).await
}

pub async fn posts_category_feed_handler(
    ResolvedState(app_state): ResolvedState,
    Path((posts_name, category)): Path<(String, String)>,
    auth: crate::login::OptionalAuth,
) -> axum::response::Response {
    render_posts_feed(app_state, posts_name, Some(category), auth.username()).await
}

async fn render_posts_feed(
    app_state: crate::AppState,
    posts_name: String,
    category: Option<String>,
    username: Option<&str>,
) -> axum::response::Response {
    use crate::sitemap::xml_escape;

    let posts_manager = match app_state.posts_managers().get(&posts_name) {
        Some(manager) => manager,
        None => return ApiResponse::NotFound.into_response(),
    };
    let config = posts_manager.get_config();

    let category_slug = category
        .as_deref()
        .map(PostsManager::category_slug)
        .filter(|slug| !slug.is_empty());

    // A category feed for an unknown category is a 404, not an empty feed
    let category_name = match category_slug.as_deref() {
        Some(slug) => {
            let categories = posts_manager.get_categories(username).await;
            match categories.into_iter().find(|c| c.slug == slug) {
                Some(c) => Some(c.name),
                None => return ApiResponse::NotFound.into_response(),
            }
        }
        None => None,
    };

    let posts = posts_manager
        .get_recent_posts(FEED_ITEM_LIMIT, category_slug.as_deref(), username)
        .await;

    let base_url = app_state.base_url().unwrap_or("http://localhost:8080");
    let site_title = app_state.template_engine().site_title();

    let channel_path = match category_slug.as_deref() {
        Some(slug) => category_url(&config.url_prefix, slug),
        None => config.url_prefix.clone(),
    };
    let channel_link = format!("{}{}", base_url, channel_path);
    let self_url = format!("{}/feed.xml", channel_link);

    let channel_title = match &category_name {
        Some(name) => format!("{} — {} — {}", site_title, capitalize(&posts_name), name),
        None => format!("{} — {}", site_title, capitalize(&posts_name)),
    };
    let channel_description = match &category_name {
        Some(name) => format!("{} posts in {}", posts_name, name),
        None => format!("{} posts", posts_name),
    };

    let mut xml = String::with_capacity(4096);
    xml.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    xml.push_str("<rss version=\"2.0\" xmlns:atom=\"http://www.w3.org/2005/Atom\" xmlns:content=\"http://purl.org/rss/1.0/modules/content/\">\n");
    xml.push_str("<channel>\n");
    xml.push_str(&format!("<title>{}</title>\n", xml_escape(&channel_title)));
    xml.push_str(&format!("<link>{}</link>\n", xml_escape(&channel_link)));
    xml.push_str(&format!(
        "<atom:link href=\"{}\" rel=\"self\" type=\"application/rss+xml\"/>\n",
        xml_escape(&self_url)
    ));
    xml.push_str(&format!(
        "<description>{}</description>\n",
        xml_escape(&channel_description)
    ));
    if let Some(newest) = posts.first() {
        xml.push_str(&format!(
            "<lastBuildDate>{}</lastBuildDate>\n",
            newest.date.to_rfc2822()
        ));
    }

    for post in &posts {
        let link = format!("{}{}/{}", base_url, config.url_prefix, post.slug);
        xml.push_str("<item>\n");
        xml.push_str(&format!("<title>{}</title>\n", xml_escape(&post.title)));
        xml.push_str(&format!("<link>{}</link>\n", xml_escape(&link)));
        xml.push_str(&format!(
            "<guid isPermaLink=\"true\">{}</guid>\n",
            xml_escape(&link)
        ));
        xml.push_str(&format!(
            "<description>{}</description>\n",
            xml_escape(&post.summary)
        ));
        xml.push_str(&format!("<pubDate>{}</pubDate>\n", post.date.to_rfc2822()));
        for category in &post.categories {
            xml.push_str(&format!("<category>{}</category>\n", xml_escape(category)));
        }
        xml.push_str(&format!(
            "<content:encoded><![CDATA[{}]]></content:encoded>\n",
            // CDATA cannot contain "]]>"; split any occurrence across sections
            post.html_content.replace("]]>", "]]]]><![CDATA[>")
        ));
        xml.push_str("</item>\n");
    }

    xml.push_str("</channel>\n</rss>\n");

    (
        [(
            axum::http::header::CONTENT_TYPE,
            "application/rss+xml; charset=utf-8",
        )],
        xml,
    )
        .into_response()
}

fn capitalize(name: &str) -> String {
    let mut chars = name.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().to_string() + chars.as_str(),
        None => String::new(),
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreatePostRequest {
    pub slug: String,
    pub title: String,
    pub summary: String,
    #[serde(default)]
    pub date: Option<String>,
    #[serde(default)]
    pub categories: Vec<String>,
    #[serde(default)]
    pub hero_image: Option<String>,
    #[serde(default)]
    pub content: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpdatePostRequest {
    pub title: String,
    pub summary: String,
    #[serde(default)]
    pub date: Option<String>,
    #[serde(default)]
    pub categories: Vec<String>,
    #[serde(default)]
    pub hero_image: Option<String>,
    #[serde(default)]
    pub content: String,
}

fn json_response(value: serde_json::Value) -> axum::response::Response {
    let mut response = axum::Json(value).into_response();
    response.headers_mut().extend(no_cache_headers());
    response
}

fn api_error(kind: ApiResponse, message: &str) -> axum::response::Response {
    let mut response = kind.with_message(message);
    response.headers_mut().extend(no_cache_headers());
    response
}

fn posts_error_response(error: super::error::PostsError) -> axum::response::Response {
    use super::error::PostsError;
    match &error {
        PostsError::InvalidSlug(_) | PostsError::DateParseError(_) => {
            api_error(ApiResponse::BadRequest, &error.to_string())
        }
        PostsError::PostAlreadyExists(_) => api_error(ApiResponse::Conflict, &error.to_string()),
        PostsError::PostNotFound(_) => api_error(ApiResponse::NotFound, &error.to_string()),
        _ => {
            error!("Posts API error: {}", error);
            api_error(ApiResponse::InternalServerError, "Failed to save post")
        }
    }
}

fn normalize_categories(categories: Vec<String>) -> Vec<String> {
    categories
        .into_iter()
        .map(|c| c.trim().to_string())
        .filter(|c| !c.is_empty())
        .collect()
}

/// Raw markdown + metadata for the editor. Requires edit permission.
pub async fn get_post_source_handler(
    ResolvedState(app_state): ResolvedState,
    Path((posts_name, slug)): Path<(String, String)>,
    auth: crate::login::OptionalAuth,
) -> impl IntoResponse {
    let posts_manager = match app_state.posts_managers().get(&posts_name) {
        Some(manager) => manager,
        None => return api_error(ApiResponse::NotFound, "Posts system not found"),
    };

    let permissions = posts_manager
        .resolve_permissions(&slug, auth.username())
        .await;
    if !permissions.can_edit_content {
        return api_error(ApiResponse::Forbidden, "Edit permission required");
    }

    let post = match posts_manager.get_post(&slug).await {
        Some(post) => post,
        None => return api_error(ApiResponse::NotFound, "Post not found"),
    };

    json_response(serde_json::json!({
        "slug": post.slug,
        "title": post.title,
        "summary": post.summary,
        "date": post.date.to_rfc3339(),
        "categories": post.categories,
        "hero_image": post.hero_image,
        "content": post.content,
    }))
}

pub async fn create_post_handler(
    ResolvedState(app_state): ResolvedState,
    Path(posts_name): Path<String>,
    auth: crate::login::OptionalAuth,
    axum::Json(request): axum::Json<CreatePostRequest>,
) -> impl IntoResponse {
    let posts_manager = match app_state.posts_managers().get(&posts_name) {
        Some(manager) => manager,
        None => return api_error(ApiResponse::NotFound, "Posts system not found"),
    };

    let permissions = posts_manager
        .resolve_permissions(&request.slug, auth.username())
        .await;
    if !permissions.can_edit_content {
        return api_error(ApiResponse::Forbidden, "Edit permission required");
    }

    let date = match &request.date {
        Some(date_str) if !date_str.trim().is_empty() => match PostsManager::parse_date(date_str) {
            Ok(date) => date,
            Err(e) => return posts_error_response(e),
        },
        _ => chrono::Utc::now(),
    };

    let metadata = super::types::PostMetadata {
        title: request.title,
        summary: request.summary,
        date,
        categories: normalize_categories(request.categories),
        hero_image: request.hero_image.filter(|h| !h.trim().is_empty()),
    };

    if let Err(e) = posts_manager
        .create_post(&request.slug, &metadata, &request.content)
        .await
    {
        return posts_error_response(e);
    }

    let config = posts_manager.get_config();
    json_response(serde_json::json!({
        "slug": request.slug,
        "url": format!("{}/{}", config.url_prefix, request.slug),
    }))
}

pub async fn update_post_handler(
    ResolvedState(app_state): ResolvedState,
    Path((posts_name, slug)): Path<(String, String)>,
    auth: crate::login::OptionalAuth,
    axum::Json(request): axum::Json<UpdatePostRequest>,
) -> impl IntoResponse {
    let posts_manager = match app_state.posts_managers().get(&posts_name) {
        Some(manager) => manager,
        None => return api_error(ApiResponse::NotFound, "Posts system not found"),
    };

    let permissions = posts_manager
        .resolve_permissions(&slug, auth.username())
        .await;
    if !permissions.can_edit_content {
        return api_error(ApiResponse::Forbidden, "Edit permission required");
    }

    let existing = match posts_manager.get_post(&slug).await {
        Some(post) => post,
        None => return api_error(ApiResponse::NotFound, "Post not found"),
    };

    // Keep the original publication date unless the editor supplies one
    let date = match &request.date {
        Some(date_str) if !date_str.trim().is_empty() => match PostsManager::parse_date(date_str) {
            Ok(date) => date,
            Err(e) => return posts_error_response(e),
        },
        _ => existing.date,
    };

    let metadata = super::types::PostMetadata {
        title: request.title,
        summary: request.summary,
        date,
        categories: normalize_categories(request.categories),
        hero_image: request.hero_image.filter(|h| !h.trim().is_empty()),
    };

    if let Err(e) = posts_manager
        .update_post(&slug, &metadata, &request.content)
        .await
    {
        return posts_error_response(e);
    }

    let config = posts_manager.get_config();
    json_response(serde_json::json!({
        "slug": slug,
        "url": format!("{}/{}", config.url_prefix, slug),
    }))
}

pub async fn delete_post_handler(
    ResolvedState(app_state): ResolvedState,
    Path((posts_name, slug)): Path<(String, String)>,
    auth: crate::login::OptionalAuth,
) -> impl IntoResponse {
    let posts_manager = match app_state.posts_managers().get(&posts_name) {
        Some(manager) => manager,
        None => return api_error(ApiResponse::NotFound, "Posts system not found"),
    };

    let permissions = posts_manager
        .resolve_permissions(&slug, auth.username())
        .await;
    if !permissions.can_edit_content {
        return api_error(ApiResponse::Forbidden, "Edit permission required");
    }

    if let Err(e) = posts_manager.delete_post(&slug).await {
        return posts_error_response(e);
    }

    let mut response = ApiResponse::Ok.with_message("Post deleted");
    response.headers_mut().extend(no_cache_headers());
    response
}

pub async fn refresh_posts_handler(
    ResolvedState(app_state): ResolvedState,
    Path(posts_name): Path<String>,
) -> impl IntoResponse {
    let posts_manager = match app_state.posts_managers().get(&posts_name) {
        Some(manager) => manager,
        None => {
            let mut response = ApiResponse::PostNotFound.into_response();
            response.headers_mut().extend(no_cache_headers());
            return response;
        }
    };

    let mut response = match posts_manager.refresh_posts().await {
        Ok(_) => ApiResponse::Ok.with_message("Posts refreshed successfully"),
        Err(e) => {
            error!("Failed to refresh posts: {}", e);
            ApiResponse::ProcessingError.with_message("Failed to refresh posts")
        }
    };

    response.headers_mut().extend(no_cache_headers());
    response
}
