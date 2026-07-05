use crate::{
    ApiResponse, TemplateType, api_response::no_cache_headers, site::ResolvedState,
    storage::DynStorage,
};
use axum::{
    extract::Path,
    http::StatusCode,
    response::{Html, IntoResponse},
};
use liquid::Parser;
use liquid_core::{Filter, FilterReflection, ParseFilter, Runtime, Value, ValueView};
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
    time::{Duration, Instant, SystemTime},
};
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

// Custom filter for JSON encoding
#[derive(Clone, Debug, Default)]
struct JsonFilter;

impl std::fmt::Display for JsonFilter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("json")
    }
}

impl Filter for JsonFilter {
    fn evaluate(
        &self,
        input: &dyn ValueView,
        _runtime: &dyn Runtime,
    ) -> liquid_core::Result<Value> {
        // Convert Liquid value to serde_json::Value first, then serialize
        let json_value = liquid_value_to_serde_json(input);
        let json_string = serde_json::to_string(&json_value)
            .map_err(|e| liquid_core::Error::with_msg(format!("JSON encoding error: {}", e)))?;

        Ok(Value::scalar(json_string))
    }
}

// Helper function to convert liquid Value to serde_json::Value
fn liquid_value_to_serde_json(input: &dyn ValueView) -> serde_json::Value {
    if let Some(scalar) = input.as_scalar() {
        if let Some(i) = scalar.to_integer() {
            serde_json::Value::Number(serde_json::Number::from(i))
        } else if let Some(f) = scalar.to_float() {
            serde_json::Number::from_f64(f)
                .map(serde_json::Value::Number)
                .unwrap_or(serde_json::Value::Null)
        } else if let Some(b) = scalar.to_bool() {
            serde_json::Value::Bool(b)
        } else {
            // Handle as string using to_kstr
            let kstr = scalar.to_kstr();
            serde_json::Value::String(kstr.to_string())
        }
    } else if let Some(array) = input.as_array() {
        let array_values: Vec<serde_json::Value> =
            array.values().map(liquid_value_to_serde_json).collect();
        serde_json::Value::Array(array_values)
    } else if let Some(object) = input.as_object() {
        let mut map = serde_json::Map::new();
        for (key, value) in object.iter() {
            map.insert(key.to_string(), liquid_value_to_serde_json(value));
        }
        serde_json::Value::Object(map)
    } else {
        serde_json::Value::Null
    }
}

impl FilterReflection for JsonFilter {
    fn name(&self) -> &str {
        "json"
    }

    fn description(&self) -> &str {
        "Converts a value to a JSON string"
    }

    fn positional_parameters(&self) -> &'static [liquid_core::parser::ParameterReflection] {
        &[]
    }

    fn keyword_parameters(&self) -> &'static [liquid_core::parser::ParameterReflection] {
        &[]
    }
}

impl ParseFilter for JsonFilter {
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

/// Default TTL for template cache entries (5 minutes).
/// Templates are re-checked against storage only after this duration.
const DEFAULT_TEMPLATE_CACHE_TTL: Duration = Duration::from_secs(5 * 60);

pub struct TemplateEngine {
    /// Storage backends for templates (first match wins)
    storages: Vec<DynStorage>,
    cache: Arc<RwLock<HashMap<String, CachedTemplate>>>,
    static_handler: Option<crate::static_files::StaticFileHandler>,
    has_user_auth: bool,
    force_color_scheme: Option<String>,
    google_fonts: Vec<tenrankai_config_storage::GoogleFontConfig>,
    /// Display title exposed to templates as `site_title`. Falls back to
    /// "Tenrankai" when unset.
    site_title: Option<String>,
    /// Copyright holder exposed to templates as `copyright_holder`. Falls back
    /// to the resolved site title when unset.
    copyright_holder: Option<String>,
    file_versions: Arc<RwLock<HashMap<String, u64>>>,
    /// TTL for cached templates. Within this duration, cached templates are
    /// returned without checking file modification time (useful for S3 backends).
    cache_ttl: Duration,
}

struct CachedTemplate {
    content: String,
    modified: SystemTime,
    fetched_at: Instant,
}

impl TemplateEngine {
    /// Create a new TemplateEngine with the given storage backends
    pub fn new(storages: Vec<DynStorage>) -> Self {
        Self {
            storages,
            cache: Arc::new(RwLock::new(HashMap::new())),
            static_handler: None,
            has_user_auth: false,
            force_color_scheme: None,
            google_fonts: Vec::new(),
            site_title: None,
            copyright_holder: None,
            file_versions: Arc::new(RwLock::new(HashMap::new())),
            cache_ttl: DEFAULT_TEMPLATE_CACHE_TTL,
        }
    }

    /// Set a custom TTL for template caching.
    /// Use Duration::ZERO to disable TTL caching and always check modification time.
    pub fn with_cache_ttl(mut self, ttl: Duration) -> Self {
        self.cache_ttl = ttl;
        self
    }

    /// Check if a template exists in any storage backend
    pub async fn template_exists(&self, path: &str) -> bool {
        for storage in &self.storages {
            match storage.exists(path).await {
                Ok(true) => return true,
                _ => continue,
            }
        }
        false
    }

    /// List the relative route paths backed by `pages/*.html.liquid` templates,
    /// excluding the homepage (`index`) and the error page (`404`).
    ///
    /// Paths are returned without the `pages/` prefix or `.html.liquid` suffix,
    /// e.g. `about` or `legal/privacy`. Results are de-duplicated across all
    /// template storage backends and sorted for deterministic output.
    pub async fn list_page_routes(&self) -> Vec<String> {
        let mut seen = HashSet::new();
        let mut routes = Vec::new();
        for storage in &self.storages {
            let entries = match storage.list_recursive("pages").await {
                Ok(entries) => entries,
                Err(_) => continue,
            };
            for entry in entries {
                if entry.is_dir {
                    continue;
                }
                let rel = entry.path.replace('\\', "/");
                let Some(rel) = rel.strip_suffix(".html.liquid") else {
                    continue;
                };
                if rel == "index" || rel == "404" {
                    continue;
                }
                if seen.insert(rel.to_string()) {
                    routes.push(rel.to_string());
                }
            }
        }
        routes.sort();
        routes
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

    pub fn set_force_color_scheme(&mut self, scheme: Option<String>) {
        self.force_color_scheme = scheme;
    }

    pub fn set_google_fonts(&mut self, fonts: Vec<tenrankai_config_storage::GoogleFontConfig>) {
        self.google_fonts = fonts;
    }

    pub fn set_site_title(&mut self, site_title: Option<String>) {
        self.site_title = site_title;
    }

    pub fn set_copyright_holder(&mut self, copyright_holder: Option<String>) {
        self.copyright_holder = copyright_holder;
    }

    fn create_parser_with_filters(
        &self,
        partials: liquid::partials::EagerCompiler<liquid::partials::InMemorySource>,
    ) -> Result<Parser, String> {
        let asset_filter = AssetUrlFilter::new(self.file_versions.clone());
        let json_filter = JsonFilter;

        liquid::ParserBuilder::with_stdlib()
            .partials(partials)
            .filter(asset_filter)
            .filter(json_filter)
            .build()
            .map_err(|e| format!("Failed to create parser: {}", e))
    }

    async fn load_template(&self, path: &str) -> Result<String, String> {
        // Fast path: check if we have a fresh cache entry (within TTL)
        // This avoids storage/network calls for frequently accessed templates
        if self.cache_ttl > Duration::ZERO {
            let cache = self.cache.read().await;
            if let Some(cached) = cache.get(path)
                && cached.fetched_at.elapsed() < self.cache_ttl
            {
                debug!(
                    "Using TTL-cached template for {} (age: {:?})",
                    path,
                    cached.fetched_at.elapsed()
                );
                return Ok(cached.content.clone());
            }
        }

        // Collect storage locations for error message
        let storage_locations: Vec<String> = self.storages.iter().map(|s| s.root_path()).collect();

        // Try to find the template in each storage backend, returning the first match
        for (idx, storage) in self.storages.iter().enumerate() {
            // Check if file exists in this storage
            match storage.metadata(path).await {
                Ok(metadata) => {
                    let modified = metadata.last_modified.unwrap_or(SystemTime::UNIX_EPOCH);

                    let mut cache = self.cache.write().await;

                    // Check modification time for cache validation
                    if let Some(cached) = cache.get(path)
                        && cached.modified >= modified
                    {
                        debug!("Using cached template for {} (modification check)", path);
                        // Update fetched_at to extend TTL
                        let content = cached.content.clone();
                        cache.insert(
                            path.to_string(),
                            CachedTemplate {
                                content: content.clone(),
                                modified,
                                fetched_at: Instant::now(),
                            },
                        );
                        return Ok(content);
                    }

                    info!(
                        "Loading template: {} from storage {} ({})",
                        path,
                        idx,
                        storage.storage_type()
                    );

                    let data = storage
                        .read(path)
                        .await
                        .map_err(|e| format!("Failed to read template {}: {}", path, e))?;

                    let content = String::from_utf8(data.to_vec())
                        .map_err(|e| format!("Template {} is not valid UTF-8: {}", path, e))?;

                    cache.insert(
                        path.to_string(),
                        CachedTemplate {
                            content: content.clone(),
                            modified,
                            fetched_at: Instant::now(),
                        },
                    );

                    return Ok(content);
                }
                Err(_) => {
                    // File doesn't exist in this storage, try the next one
                    continue;
                }
            }
        }

        // Provide detailed error with searched locations
        let locations_msg = if storage_locations.is_empty() {
            "no template directories configured".to_string()
        } else {
            format!("searched: {}", storage_locations.join(", "))
        };
        Err(format!("Template '{}' not found ({})", path, locations_msg))
    }

    pub async fn render_with_gallery(&self, path: &str) -> Result<Html<String>, StatusCode> {
        let template_path = if path.is_empty() || path == "/" {
            TemplateType::Index.path().to_string()
        } else {
            TemplateType::dynamic_page_path(path.trim_start_matches('/')).path()
        };

        let globals = liquid::object!({});

        match self.render_template(&template_path, globals).await {
            Ok(html) => Ok(Html(html)),
            Err(e) => {
                error!("Template rendering error: {}", e);
                Err(ApiResponse::TemplateRenderError.status_code())
            }
        }
    }

    pub async fn render_404_page(&self) -> Result<Html<String>, StatusCode> {
        let globals = liquid::object!({});

        match self
            .render_template(TemplateType::NotFound.path(), globals)
            .await
        {
            Ok(html) => {
                // Create custom response with 404 status
                Ok(Html(html))
            }
            Err(e) => {
                error!("Failed to render 404 template: {}", e);
                Err(ApiResponse::TemplateNotFound.status_code())
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

        // Add site identity for header/footer branding. `site_title` falls back
        // to "Tenrankai"; `copyright_holder` falls back to the resolved title.
        let site_title = self
            .site_title
            .clone()
            .unwrap_or_else(|| "Tenrankai".to_string());
        let copyright_holder = self
            .copyright_holder
            .clone()
            .unwrap_or_else(|| site_title.clone());
        globals.insert(
            "site_title".into(),
            liquid::model::Value::scalar(site_title),
        );
        globals.insert(
            "copyright_holder".into(),
            liquid::model::Value::scalar(copyright_holder),
        );

        // Add user auth flag
        globals.insert(
            "has_user_auth".into(),
            liquid::model::Value::scalar(self.has_user_auth),
        );

        // Add force color scheme (if set)
        if let Some(ref scheme) = self.force_color_scheme {
            globals.insert(
                "force_color_scheme".into(),
                liquid::model::Value::scalar(scheme.clone()),
            );
        }

        // Add google_fonts array for dynamic font loading
        if !self.google_fonts.is_empty() {
            let fonts_value: Vec<liquid::model::Value> = self
                .google_fonts
                .iter()
                .map(|f| {
                    liquid::model::to_value(&serde_json::json!({
                        "family": f.family,
                        "weights": f.weights.join(";")
                    }))
                    .unwrap_or(liquid::model::Value::Nil)
                })
                .collect();
            globals.insert(
                "google_fonts".into(),
                liquid::model::Value::Array(fonts_value),
            );
        }

        // Load common partials first (before loading main template)
        let header_content = self
            .load_template(TemplateType::Header.path())
            .await
            .unwrap_or_else(|e| {
                error!("Failed to load header partial: {}", e);
                String::new()
            });
        let footer_content = self
            .load_template(TemplateType::Footer.path())
            .await
            .unwrap_or_else(|e| {
                error!("Failed to load footer partial: {}", e);
                String::new()
            });
        let gallery_preview_content = self
            .load_template(TemplateType::GalleryPreview.path())
            .await
            .unwrap_or_else(|e| {
                error!("Failed to load gallery preview partial: {}", e);
                String::new()
            });

        // Load user menu partial if user auth is enabled
        let user_menu_content = if self.has_user_auth {
            self.load_template(TemplateType::UserMenu.path())
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

#[axum::debug_handler(state = crate::AppState)]
pub async fn template_with_gallery_handler(
    ResolvedState(app_state): ResolvedState,
    path: Option<Path<String>>,
) -> impl IntoResponse {
    let path = path.map(|p| p.0).unwrap_or_default();

    // Check if template exists first
    let template_path = if path.is_empty() || path == "/" {
        TemplateType::Index.path().to_string()
    } else {
        TemplateType::dynamic_page_path(path.trim_start_matches('/')).path()
    };

    // Check if template exists in any of the storage backends
    let template_exists = app_state
        .template_engine()
        .template_exists(&template_path)
        .await;

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

        // Check if file exists in any storage backend
        if app_state.static_handler().exists(check_path).await {
            debug!("Found static file for path: {}, serving it", path);
            // Pass the path without the "static/" prefix to the serve method
            // Templates don't have version parameters, so pass false
            return app_state.static_handler().serve(check_path, false).await;
        }

        debug!(
            "No template or static file found for: {}, returning 404",
            path
        );
        return match app_state.template_engine().render_404_page().await {
            Ok(html) => ApiResponse::NotFound.with_html(html.0),
            Err(_) => ApiResponse::NotFound.into_response(),
        };
    }

    match app_state.template_engine().render_with_gallery(&path).await {
        Ok(html) => (no_cache_headers(), html).into_response(),
        Err(status) => status.into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::FilesystemStorage;
    use liquid::model;
    use std::path::PathBuf;

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
        let storage = Arc::new(FilesystemStorage::new(PathBuf::from("templates")));
        let mut template_engine = TemplateEngine::new(vec![storage]);

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
