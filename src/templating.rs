use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{Html, IntoResponse},
};
use liquid::Parser;
use liquid_core::{Filter, FilterReflection, ParseFilter, Runtime, Value, ValueView};
use std::{collections::HashMap, path::PathBuf, sync::Arc, time::SystemTime};
use tokio::sync::RwLock;
use tracing::{debug, error, info};

// Custom filter for asset URLs with cache busting
#[derive(Clone, Debug)]
struct AssetUrlFilter {
    file_versions: Arc<RwLock<HashMap<String, u64>>>,
}

impl AssetUrlFilter {
    fn new(file_versions: Arc<RwLock<HashMap<String, u64>>>) -> Self {
        Self { file_versions }
    }
}

impl std::fmt::Display for AssetUrlFilter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("asset_url")
    }
}

impl Filter for AssetUrlFilter {
    fn evaluate(
        &self,
        input: &dyn ValueView,
        _runtime: &dyn Runtime,
    ) -> liquid_core::Result<Value> {
        let path = input.to_kstr().to_string();

        // Normalize the path
        let normalized_path = if path.starts_with("/static/") {
            path.clone()
        } else if path.starts_with("static/") {
            format!("/{}", path)
        } else {
            format!("/static/{}", path)
        };

        // Extract filename for version lookup
        let filename = normalized_path.rsplit('/').next().unwrap_or(&path);

        // Try to get version from cached versions (blocking read)
        if let Ok(versions) = self.file_versions.try_read()
            && let Some(&version) = versions.get(filename)
        {
            return Ok(Value::scalar(format!("{}?v={}", normalized_path, version)));
        }

        // No version found, return plain URL
        Ok(Value::scalar(normalized_path))
    }
}

impl FilterReflection for AssetUrlFilter {
    fn name(&self) -> &str {
        "asset_url"
    }

    fn description(&self) -> &str {
        "Converts an asset path to a versioned URL for cache busting"
    }

    fn positional_parameters(&self) -> &'static [liquid_core::parser::ParameterReflection] {
        &[]
    }

    fn keyword_parameters(&self) -> &'static [liquid_core::parser::ParameterReflection] {
        &[]
    }
}

impl ParseFilter for AssetUrlFilter {
    fn reflection(&self) -> &dyn FilterReflection {
        self
    }

    fn parse(
        &self,
        _arguments: liquid_core::parser::FilterArguments,
    ) -> liquid_core::Result<Box<dyn Filter>> {
        Ok(Box::new(self.clone()))
    }
}

pub struct TemplateEngine {
    pub template_dirs: Vec<PathBuf>,
    cache: Arc<RwLock<HashMap<String, CachedTemplate>>>,
    static_handler: Option<crate::static_files::StaticFileHandler>,
    has_user_auth: bool,
    file_versions: Arc<RwLock<HashMap<String, u64>>>,
}

struct CachedTemplate {
    content: String,
    modified: SystemTime,
}

impl TemplateEngine {
    pub fn new(template_dirs: Vec<PathBuf>) -> Self {
        Self {
            template_dirs,
            cache: Arc::new(RwLock::new(HashMap::new())),
            static_handler: None,
            has_user_auth: false,
            file_versions: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn set_static_handler(&mut self, handler: crate::static_files::StaticFileHandler) {
        debug!("Setting static handler on template engine");
        self.static_handler = Some(handler);
    }

    pub async fn update_file_versions(&self) {
        if let Some(ref handler) = self.static_handler {
            // Get all file versions from the static handler
            let all_versions = handler.get_all_versions().await;
            let mut versions = self.file_versions.write().await;
            *versions = all_versions;
            debug!(
                "Updated template engine with {} file versions",
                versions.len()
            );
        }
    }

    pub fn set_has_user_auth(&mut self, has_auth: bool) {
        self.has_user_auth = has_auth;
    }

    fn create_parser_with_filters(
        &self,
        partials: liquid::partials::EagerCompiler<liquid::partials::InMemorySource>,
    ) -> Result<Parser, String> {
        let asset_filter = AssetUrlFilter::new(self.file_versions.clone());

        liquid::ParserBuilder::with_stdlib()
            .partials(partials)
            .filter(asset_filter)
            .build()
            .map_err(|e| format!("Failed to create parser: {}", e))
    }

    async fn load_template(&self, path: &str) -> Result<String, String> {
        // Try to find the template in each directory, returning the first match
        for template_dir in &self.template_dirs {
            let template_path = template_dir.join(path);

            // Check if file exists in this directory
            match tokio::fs::metadata(&template_path).await {
                Ok(metadata) => {
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

                    info!("Loading template: {} from {:?}", path, template_dir);

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

                    return Ok(content);
                }
                Err(_) => {
                    // File doesn't exist in this directory, try the next one
                    continue;
                }
            }
        }

        Err(format!(
            "Template {} not found in any of the configured directories: {:?}",
            path, self.template_dirs
        ))
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
        debug!("render_template called for: {}", template_name);
        debug!(
            "static_handler available: {}",
            self.static_handler.is_some()
        );
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

        // Add user auth flag
        globals.insert(
            "has_user_auth".into(),
            liquid::model::Value::scalar(self.has_user_auth),
        );

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

        // Load user menu partial if user auth is enabled
        let user_menu_content = if self.has_user_auth {
            self.load_template("partials/_user_menu.html.liquid")
                .await
                .unwrap_or_else(|e| {
                    error!("Failed to load user menu partial: {}", e);
                    String::new()
                })
        } else {
            String::new()
        };

        let template_content = self.load_template(template_name).await?;

        // Create a partials source for includes
        let mut partials_source = liquid::partials::InMemorySource::new();
        partials_source.add("_header.html.liquid", header_content.clone());
        partials_source.add("_footer.html.liquid", footer_content.clone());
        partials_source.add(
            "_gallery_preview.html.liquid",
            gallery_preview_content.clone(),
        );
        if self.has_user_auth {
            partials_source.add("_user_menu.html.liquid", user_menu_content.clone());
        }

        let partials = liquid::partials::EagerCompiler::new(partials_source);

        // Create parser with custom filters
        let parser = self.create_parser_with_filters(partials)?;

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

    // Check if template exists in any of the template directories
    let mut template_exists = false;
    for template_dir in &app_state.template_engine.template_dirs {
        let template_file_path = template_dir.join(template_path);
        if template_file_path.exists() {
            template_exists = true;
            break;
        }
    }

    if !template_exists {
        debug!(
            "Template not found: {}, checking for static file",
            template_path
        );

        // Check if there's a matching static file in any of the directories
        // If the path starts with "static/", strip it before checking
        let check_path = if path.starts_with("static/") {
            path.trim_start_matches("static/")
        } else {
            &path
        };

        // Check each static directory in order
        for (index, static_dir) in app_state.static_handler.static_dirs.iter().enumerate() {
            let static_file_path = static_dir.join(check_path);
            if static_file_path.exists() && static_file_path.starts_with(static_dir) {
                debug!(
                    "Found static file for path: {} in directory {}, serving it",
                    path, index
                );
                // Pass the path without the "static/" prefix to the serve method
                // Templates don't have version parameters, so pass false
                return app_state.static_handler.serve(check_path, false).await;
            }
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
mod tests {
    use super::*;
    use liquid::model;

    #[tokio::test]
    async fn test_asset_url_filter_with_versions() {
        // Create a mock file versions map
        let file_versions = Arc::new(RwLock::new(HashMap::new()));
        {
            let mut versions = file_versions.write().await;
            versions.insert("style.css".to_string(), 123456789);
            versions.insert("login.js".to_string(), 987654321);
            versions.insert("app.js".to_string(), 555555555);
        }

        let filter = AssetUrlFilter::new(file_versions);
        let runtime = liquid_core::runtime::RuntimeBuilder::new().build();

        // Test with just filename
        let input = model::Value::scalar("style.css");
        let result = filter.evaluate(&input, &runtime).unwrap();
        assert_eq!(
            result,
            model::Value::scalar("/static/style.css?v=123456789")
        );

        // Test with static/ prefix
        let input = model::Value::scalar("static/login.js");
        let result = filter.evaluate(&input, &runtime).unwrap();
        assert_eq!(result, model::Value::scalar("/static/login.js?v=987654321"));

        // Test with /static/ prefix
        let input = model::Value::scalar("/static/app.js");
        let result = filter.evaluate(&input, &runtime).unwrap();
        assert_eq!(result, model::Value::scalar("/static/app.js?v=555555555"));
    }

    #[tokio::test]
    async fn test_asset_url_filter_without_versions() {
        // Create an empty file versions map
        let file_versions = Arc::new(RwLock::new(HashMap::new()));
        let filter = AssetUrlFilter::new(file_versions);
        let runtime = liquid_core::runtime::RuntimeBuilder::new().build();

        // Test with just filename - no version
        let input = model::Value::scalar("unknown.css");
        let result = filter.evaluate(&input, &runtime).unwrap();
        assert_eq!(result, model::Value::scalar("/static/unknown.css"));

        // Test with static/ prefix - no version
        let input = model::Value::scalar("static/missing.js");
        let result = filter.evaluate(&input, &runtime).unwrap();
        assert_eq!(result, model::Value::scalar("/static/missing.js"));
    }

    #[tokio::test]
    async fn test_template_with_asset_url_filter() {
        // Create a template engine with file versions
        let mut template_engine = TemplateEngine::new(vec![PathBuf::from("templates")]);

        // Set up file versions
        let file_versions = Arc::new(RwLock::new(HashMap::new()));
        {
            let mut versions = file_versions.write().await;
            versions.insert("test.css".to_string(), 111111111);
            versions.insert("test.js".to_string(), 222222222);
        }
        template_engine.file_versions = file_versions;

        // Create a parser with the filter
        let partials =
            liquid::partials::EagerCompiler::new(liquid::partials::InMemorySource::new());
        let parser = template_engine
            .create_parser_with_filters(partials)
            .unwrap();

        // Test CSS filter
        let template = parser.parse("{{ 'test.css' | asset_url }}").unwrap();
        let output = template.render(&liquid::object!({})).unwrap();
        assert_eq!(output, "/static/test.css?v=111111111");

        // Test JS filter
        let template = parser.parse("{{ 'test.js' | asset_url }}").unwrap();
        let output = template.render(&liquid::object!({})).unwrap();
        assert_eq!(output, "/static/test.js?v=222222222");

        // Test multiple filters in one template
        let template = parser
            .parse(
                r#"<link href="{{ 'test.css' | asset_url }}">
<script src="{{ 'test.js' | asset_url }}"></script>"#,
            )
            .unwrap();
        let output = template.render(&liquid::object!({})).unwrap();
        assert_eq!(
            output,
            r#"<link href="/static/test.css?v=111111111">
<script src="/static/test.js?v=222222222"></script>"#
        );
    }
}

#[cfg(test)]
#[path = "templating_tests.rs"]
mod templating_tests;

#[cfg(test)]
#[path = "templating_multi_dir_tests.rs"]
mod templating_multi_dir_tests;
