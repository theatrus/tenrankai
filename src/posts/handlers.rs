use crate::AppState;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{Html, IntoResponse},
};
use chrono::Datelike;
use serde::Deserialize;
use tracing::error;

#[derive(Deserialize)]
pub struct PostsQuery {
    page: Option<usize>,
}

pub async fn posts_index_handler(
    State(app_state): State<AppState>,
    Path(posts_name): Path<String>,
    Query(query): Query<PostsQuery>,
) -> impl IntoResponse {
    let page = query.page.unwrap_or(0);

    let posts_manager = match app_state.posts_managers.get(&posts_name) {
        Some(manager) => manager,
        None => {
            return (StatusCode::NOT_FOUND, "Posts section not found").into_response();
        }
    };

    let posts_raw = posts_manager.get_posts_page(page).await;

    // Convert posts to include formatted dates
    let posts: Vec<_> = posts_raw
        .into_iter()
        .map(|post| {
            let date = post.date;
            liquid::object!({
                "slug": post.slug,
                "title": post.title,
                "summary": post.summary,
                "url": post.url,
                "date": post.date.to_rfc3339(),
                "date_formatted": format!("{} {}, {}",
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
                ),
            })
        })
        .collect();
    let total_pages = posts_manager.get_total_pages().await;
    let config = posts_manager.get_config();

    let base_url = app_state
        .config
        .app
        .base_url
        .as_deref()
        .unwrap_or("http://localhost:8080");

    let page_title = posts_name
        .chars()
        .next()
        .unwrap()
        .to_uppercase()
        .to_string()
        + &posts_name[1..];
    let meta_description = format!("Browse {} posts", posts_name);

    let globals = liquid::object!({
        "posts": posts,
        "posts_name": posts_name,
        "url_prefix": config.url_prefix,
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
        .template_engine
        .render_template(&config.index_template, globals)
        .await
    {
        Ok(html) => Html(html).into_response(),
        Err(e) => {
            error!("Template rendering error: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, "Template error").into_response()
        }
    }
}

pub async fn post_detail_handler(
    State(app_state): State<AppState>,
    Path((posts_name, slug)): Path<(String, String)>,
) -> impl IntoResponse {
    let posts_manager = match app_state.posts_managers.get(&posts_name) {
        Some(manager) => manager,
        None => {
            return (StatusCode::NOT_FOUND, "Posts section not found").into_response();
        }
    };

    let post = match posts_manager.get_post(&slug).await {
        Some(post) => post,
        None => {
            return (StatusCode::NOT_FOUND, "Post not found").into_response();
        }
    };

    let config = posts_manager.get_config();

    let base_url = app_state
        .config
        .app
        .base_url
        .as_deref()
        .unwrap_or("http://localhost:8080");

    let full_url = format!("{}{}/{}", base_url, config.url_prefix, post.slug);

    let date_formatted = post.date.format("%B %-d, %Y").to_string();

    let globals = liquid::object!({
        "post": {
            "slug": post.slug,
            "title": post.title,
            "summary": post.summary,
            "date": post.date.to_rfc3339(),
            "date_formatted": date_formatted,
            "content": post.content,
            "html_content": post.html_content,
        },
        "posts_name": posts_name,
        "url_prefix": config.url_prefix,
        "base_url": base_url,
        "page_title": post.title,
        "meta_description": post.summary,
        "og_title": post.title,
        "og_description": post.summary,
        "og_url": full_url,
        "og_type": "article",
        "article_published_time": post.date.to_rfc3339(),
    });

    match app_state
        .template_engine
        .render_template(&config.post_template, globals)
        .await
    {
        Ok(html) => Html(html).into_response(),
        Err(e) => {
            error!("Template rendering error: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, "Template error").into_response()
        }
    }
}

pub async fn refresh_posts_handler(
    State(app_state): State<AppState>,
    Path(posts_name): Path<String>,
) -> impl IntoResponse {
    let posts_manager = match app_state.posts_managers.get(&posts_name) {
        Some(manager) => manager,
        None => {
            return (StatusCode::NOT_FOUND, "Posts section not found").into_response();
        }
    };

    match posts_manager.refresh_posts().await {
        Ok(_) => (StatusCode::OK, "Posts refreshed successfully").into_response(),
        Err(e) => {
            error!("Failed to refresh posts: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, "Failed to refresh posts").into_response()
        }
    }
}
