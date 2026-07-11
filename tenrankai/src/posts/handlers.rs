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

fn category_objects(url_prefix: &str, categories: &[String]) -> Vec<liquid::Object> {
    categories
        .iter()
        .map(|name| {
            let slug = PostsManager::category_slug(name);
            liquid::object!({
                "name": name,
                "slug": slug.clone(),
                "url": format!("{}?category={}", url_prefix, slug),
            })
        })
        .collect()
}

pub async fn posts_index_handler(
    ResolvedState(app_state): ResolvedState,
    Path(posts_name): Path<String>,
    auth: crate::login::OptionalAuth,
    Query(query): Query<PostsQuery>,
) -> impl IntoResponse {
    let page = query.page.unwrap_or(0);
    let username = auth.username();
    let category_filter = query
        .category
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
                "url": format!("{}?category={}", config.url_prefix, c.slug),
                "active": Some(c.slug.as_str()) == category_filter.as_deref(),
            })
        })
        .collect();

    // Query-string suffix appended to pagination links to preserve the filter
    let category_query = category_filter
        .as_deref()
        .map(|slug| format!("&category={}", slug))
        .unwrap_or_default();

    let base_url = app_state.base_url().unwrap_or("http://localhost:8080");

    let page_title = match &active_category {
        Some(name) => format!(
            "{} – {}",
            posts_name
                .chars()
                .next()
                .unwrap()
                .to_uppercase()
                .to_string()
                + &posts_name[1..],
            name
        ),
        None => {
            posts_name
                .chars()
                .next()
                .unwrap()
                .to_uppercase()
                .to_string()
                + &posts_name[1..]
        }
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
        "category_query": category_query,
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
        "og_url": format!("{}{}", base_url, config.url_prefix),
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
