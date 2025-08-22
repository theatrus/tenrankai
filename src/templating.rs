use crate::gallery::SharedGallery;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{Html, IntoResponse},
};
use std::{collections::HashMap, path::PathBuf, sync::Arc, time::SystemTime};
use tokio::sync::RwLock;
use tracing::{debug, error, info};

pub struct TemplateEngine {
    pub template_dir: PathBuf,
    cache: Arc<RwLock<HashMap<String, CachedTemplate>>>,
}

struct CachedTemplate {
    content: String,
    modified: SystemTime,
}

impl TemplateEngine {
    pub fn new(template_dir: PathBuf) -> Self {
        Self {
            template_dir,
            cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    async fn load_template(&self, path: &str) -> Result<String, String> {
        let template_path = self.template_dir.join(path);

        let metadata = tokio::fs::metadata(&template_path)
            .await
            .map_err(|e| format!("Failed to get metadata for {}: {}", path, e))?;

        let modified = metadata
            .modified()
            .map_err(|e| format!("Failed to get modified time: {}", e))?;

        let mut cache = self.cache.write().await;

        if let Some(cached) = cache.get(path)
            && cached.modified >= modified {
            debug!("Using cached template for {}", path);
            return Ok(cached.content.clone());
        }

        info!("Loading template: {}", path);

        let content = tokio::fs::read_to_string(&template_path)
            .await
            .map_err(|e| format!("Failed to read template {}: {}", path, e))?;

        cache.insert(
            path.to_string(),
            CachedTemplate {
                content: content.clone(),
                modified,
            },
        );

        Ok(content)
    }

    pub async fn render_with_gallery(
        &self,
        path: &str,
        gallery: &SharedGallery,
    ) -> Result<Html<String>, StatusCode> {
        let template_path = if path.is_empty() || path == "/" {
            "index.html.liquid"
        } else {
            &format!("{}.html.liquid", path.trim_start_matches('/'))
        };

        let gallery_preview = gallery.get_gallery_preview(6).await.unwrap_or_default();
        let gallery_preview_json =
            serde_json::to_string(&gallery_preview).unwrap_or_else(|_| "[]".to_string());

        let globals = liquid::object!({
            "gallery_preview": gallery_preview,
            "gallery_preview_json": gallery_preview_json,
        });

        match self.render_template(template_path, globals).await {
            Ok(html) => Ok(Html(html)),
            Err(e) => {
                error!("Template rendering error: {}", e);
                Err(StatusCode::INTERNAL_SERVER_ERROR)
            }
        }
    }

    pub async fn render_404_page(
        &self,
        gallery: &SharedGallery,
    ) -> Result<Html<String>, StatusCode> {
        let gallery_preview = gallery.get_gallery_preview(6).await.unwrap_or_default();
        let gallery_preview_json =
            serde_json::to_string(&gallery_preview).unwrap_or_else(|_| "[]".to_string());

        let globals = liquid::object!({
            "gallery_preview": gallery_preview,
            "gallery_preview_json": gallery_preview_json,
        });

        match self.render_template("404.html.liquid", globals).await {
            Ok(html) => {
                // Create custom response with 404 status
                Ok(Html(html))
            },
            Err(e) => {
                error!("Failed to render 404 template: {}", e);
                Err(StatusCode::NOT_FOUND)
            }
        }
    }

    pub async fn render_template(
        &self,
        template_name: &str,
        globals: liquid::Object,
    ) -> Result<String, String> {
        let header_content = self
            .load_template("_header.html.liquid")
            .await
            .unwrap_or_else(|e| {
                error!("Failed to load header: {}", e);
                String::new()
            });

        let footer_template = self
            .load_template("_footer.html.liquid")
            .await
            .unwrap_or_else(|e| {
                error!("Failed to load footer: {}", e);
                String::new()
            });

        // Render footer template with current year
        let current_year = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
            / (365 * 24 * 3600)
            + 1970;

        let footer_globals = liquid::object!({
            "current_year": current_year.to_string(),
        });

        let footer_parser = liquid::ParserBuilder::with_stdlib()
            .build()
            .map_err(|e| format!("Failed to create footer parser: {}", e))?;

        let footer_template_parsed = footer_parser
            .parse(&footer_template)
            .map_err(|e| format!("Failed to parse footer template: {}", e))?;

        let footer_content = footer_template_parsed
            .render(&footer_globals)
            .unwrap_or_else(|e| {
                error!("Failed to render footer template: {}", e);
                footer_template
            });

        let template_content = self.load_template(template_name).await?;

        let parser = liquid::ParserBuilder::with_stdlib()
            .build()
            .map_err(|e| format!("Failed to create parser: {}", e))?;

        let template = parser
            .parse(&template_content)
            .map_err(|e| format!("Failed to parse template: {}", e))?;

        // Render the gallery preview component if gallery_preview data exists
        let gallery_preview_rendered = if let Some(gallery_preview) = globals.get("gallery_preview")
        {
            let gallery_preview_template = self
                .load_template("_gallery_preview.html.liquid")
                .await
                .unwrap_or_else(|e| {
                    error!("Failed to load gallery preview: {}", e);
                    String::new()
                });

            if !gallery_preview_template.is_empty() {
                let preview_parser = liquid::ParserBuilder::with_stdlib()
                    .build()
                    .map_err(|e| format!("Failed to create preview parser: {}", e))?;

                let preview_template = preview_parser
                    .parse(&gallery_preview_template)
                    .map_err(|e| format!("Failed to parse preview template: {}", e))?;

                let mut preview_globals = liquid::object!({
                    "preview_title": "Recent from the Gallery",
                    "show_explore_link": true,
                });
                preview_globals.insert("gallery_preview".into(), gallery_preview.clone());

                // Add gallery_preview_json if it exists in the main globals
                if let Some(json_value) = globals.get("gallery_preview_json") {
                    preview_globals.insert("gallery_preview_json".into(), json_value.clone());
                } else {
                    preview_globals.insert(
                        "gallery_preview_json".into(),
                        liquid::model::Value::Scalar("[]".into()),
                    );
                }

                preview_template
                    .render(&preview_globals)
                    .unwrap_or_else(|e| {
                        error!("Failed to render gallery preview component: {}", e);
                        String::new()
                    })
            } else {
                String::new()
            }
        } else {
            String::new()
        };

        let mut full_globals = globals;
        full_globals.insert(
            "header".into(),
            liquid::model::Value::Scalar(header_content.into()),
        );
        full_globals.insert(
            "footer".into(),
            liquid::model::Value::Scalar(footer_content.into()),
        );
        full_globals.insert(
            "gallery_preview_component".into(),
            liquid::model::Value::Scalar(gallery_preview_rendered.into()),
        );

        // Ensure gallery_preview exists with default empty array if not provided
        if !full_globals.contains_key("gallery_preview") {
            full_globals.insert(
                "gallery_preview".into(),
                liquid::model::Value::Array(Vec::new()),
            );
        }

        template
            .render(&full_globals)
            .map_err(|e| format!("Failed to render template: {}", e))
    }
}

#[axum::debug_handler]
pub async fn template_with_gallery_handler(
    State(app_state): State<crate::AppState>,
    path: Option<Path<String>>,
) -> impl IntoResponse {
    let path = path.map(|p| p.0).unwrap_or_default();
    
    // Check if template exists first
    let template_path = if path.is_empty() || path == "/" {
        "index.html.liquid"
    } else {
        &format!("{}.html.liquid", path.trim_start_matches('/'))
    };
    
    let template_file_path = app_state.template_engine.template_dir.join(template_path);
    if !template_file_path.exists() {
        debug!("Template not found: {}, checking for static file", template_path);
        
        // Check if there's a matching static file
        let static_file_path = app_state.static_handler.static_dir.join(&path);
        if static_file_path.exists() && static_file_path.starts_with(&app_state.static_handler.static_dir) {
            debug!("Found static file for path: {}, serving it", path);
            return app_state.static_handler.serve(&path).await;
        }
        
        debug!("No template or static file found for: {}, returning 404", path);
        return match app_state.template_engine.render_404_page(&app_state.gallery).await {
            Ok(html) => (StatusCode::NOT_FOUND, html).into_response(),
            Err(_) => StatusCode::NOT_FOUND.into_response(),
        };
    }
    
    match app_state
        .template_engine
        .render_with_gallery(&path, &app_state.gallery)
        .await
    {
        Ok(html) => html.into_response(),
        Err(status) => status.into_response(),
    }
}
