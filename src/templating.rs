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
    static_handler: Option<crate::static_files::StaticFileHandler>,
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
            static_handler: None,
        }
    }
    
    pub fn set_static_handler(&mut self, handler: crate::static_files::StaticFileHandler) {
        self.static_handler = Some(handler);
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
            && cached.modified >= modified
        {
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

    pub async fn render_with_gallery(&self, path: &str) -> Result<Html<String>, StatusCode> {
        let template_path = if path.is_empty() || path == "/" {
            "pages/index.html.liquid"
        } else {
            &format!("pages/{}.html.liquid", path.trim_start_matches('/'))
        };

        let globals = liquid::object!({});

        match self.render_template(template_path, globals).await {
            Ok(html) => Ok(Html(html)),
            Err(e) => {
                error!("Template rendering error: {}", e);
                Err(StatusCode::INTERNAL_SERVER_ERROR)
            }
        }
    }

    pub async fn render_404_page(&self) -> Result<Html<String>, StatusCode> {
        let globals = liquid::object!({});

        match self.render_template("pages/404.html.liquid", globals).await {
            Ok(html) => {
                // Create custom response with 404 status
                Ok(Html(html))
            }
            Err(e) => {
                error!("Failed to render 404 template: {}", e);
                Err(StatusCode::NOT_FOUND)
            }
        }
    }

    pub async fn render_template(
        &self,
        template_name: &str,
        mut globals: liquid::Object,
    ) -> Result<String, String> {
        // Add current year to globals for footer
        let current_year = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
            / (365 * 24 * 3600)
            + 1970;

        globals.insert(
            "current_year".into(),
            liquid::model::Value::scalar(current_year as i64),
        );
        
        // Add versioned URLs for common static files
        if let Some(ref static_handler) = self.static_handler {
            let style_css_url = tokio::task::block_in_place(|| {
                tokio::runtime::Handle::current().block_on(async {
                    static_handler.get_versioned_url("/static/style.css").await
                })
            });
            globals.insert(
                "style_css_url".into(),
                liquid::model::Value::scalar(style_css_url),
            );
        } else {
            globals.insert(
                "style_css_url".into(),
                liquid::model::Value::scalar("/static/style.css"),
            );
        }

        // Load common partials first (before loading main template)
        let header_content = self
            .load_template("partials/_header.html.liquid")
            .await
            .unwrap_or_else(|e| {
                error!("Failed to load header partial: {}", e);
                String::new()
            });
        let footer_content = self
            .load_template("partials/_footer.html.liquid")
            .await
            .unwrap_or_else(|e| {
                error!("Failed to load footer partial: {}", e);
                String::new()
            });
        let gallery_preview_content = self
            .load_template("partials/_gallery_preview.html.liquid")
            .await
            .unwrap_or_else(|e| {
                error!("Failed to load gallery preview partial: {}", e);
                String::new()
            });

        let template_content = self.load_template(template_name).await?;

        // Create a partials source for includes
        let mut partials_source = liquid::partials::InMemorySource::new();
        partials_source.add("_header.html.liquid", header_content.clone());
        partials_source.add("_footer.html.liquid", footer_content.clone());
        partials_source.add(
            "_gallery_preview.html.liquid",
            gallery_preview_content.clone(),
        );

        let partials = liquid::partials::EagerCompiler::new(partials_source);

        let parser = liquid::ParserBuilder::with_stdlib()
            .partials(partials)
            .build()
            .map_err(|e| format!("Failed to create parser: {}", e))?;

        let template = parser
            .parse(&template_content)
            .map_err(|e| format!("Failed to parse template: {}", e))?;

        template
            .render(&globals)
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
        "pages/index.html.liquid"
    } else {
        &format!("pages/{}.html.liquid", path.trim_start_matches('/'))
    };

    let template_file_path = app_state.template_engine.template_dir.join(template_path);
    if !template_file_path.exists() {
        debug!(
            "Template not found: {}, checking for static file",
            template_path
        );

        // Check if there's a matching static file
        // If the path starts with "static/", strip it before checking
        let check_path = if path.starts_with("static/") {
            path.trim_start_matches("static/")
        } else {
            &path
        };

        let static_file_path = app_state.static_handler.static_dir.join(check_path);
        if static_file_path.exists()
            && static_file_path.starts_with(&app_state.static_handler.static_dir)
        {
            debug!("Found static file for path: {}, serving it", path);
            // Pass the path without the "static/" prefix to the serve method
            // Templates don't have version parameters, so pass false
            return app_state.static_handler.serve(check_path, false).await;
        }

        debug!(
            "No template or static file found for: {}, returning 404",
            path
        );
        return match app_state.template_engine.render_404_page().await {
            Ok(html) => (StatusCode::NOT_FOUND, html).into_response(),
            Err(_) => StatusCode::NOT_FOUND.into_response(),
        };
    }

    match app_state.template_engine.render_with_gallery(&path).await {
        Ok(html) => html.into_response(),
        Err(status) => status.into_response(),
    }
}

#[cfg(test)]
#[path = "templating_tests.rs"]
mod templating_tests;
