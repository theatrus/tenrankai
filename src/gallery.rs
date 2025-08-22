use axum::{
    body::Body,
    extract::{Path, Query, State},
    http::{StatusCode, header},
    response::{Html, IntoResponse, Response},
};
use image::{ImageFormat, imageops::FilterType};
use pulldown_cmark::{Parser, html};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    path::{Path as StdPath, PathBuf},
    sync::Arc,
    time::SystemTime,
};
use tokio::sync::RwLock;
use tokio_util::io::ReaderStream;
use tracing::error;
use walkdir::WalkDir;

use crate::{GalleryConfig, static_files::StaticFileHandler, templating::TemplateEngine};

#[derive(Debug, Clone, Serialize)]
pub struct GalleryItem {
    pub name: String,
    pub path: String,
    pub is_directory: bool,
    pub thumbnail_url: Option<String>,
    pub item_count: Option<usize>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ImageInfo {
    pub name: String,
    pub path: String,
    pub url: String,
    pub thumbnail_url: String,
    pub medium_url: String,
    pub description: Option<String>,
    pub exif_data: Option<HashMap<String, String>>,
    pub file_size: u64,
    pub dimensions: (u32, u32),
}

#[derive(Debug, Clone, Deserialize)]
pub struct GalleryQuery {
    pub page: Option<usize>,
    pub size: Option<String>,
}

pub struct Gallery {
    config: GalleryConfig,
    cache: Arc<RwLock<HashMap<String, CachedImage>>>,
}

struct CachedImage {
    path: PathBuf,
    modified: SystemTime,
}

impl Gallery {
    pub fn new(config: GalleryConfig) -> Self {
        Self {
            config,
            cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn scan_directory(&self, relative_path: &str) -> Result<Vec<GalleryItem>, String> {
        let full_path = self.config.source_directory.join(relative_path);

        if !full_path.starts_with(&self.config.source_directory) {
            return Err("Invalid path".to_string());
        }

        let mut items = Vec::new();

        let entries = tokio::fs::read_dir(&full_path)
            .await
            .map_err(|e| format!("Failed to read directory: {}", e))?;

        let mut entries = entries;
        while let Some(entry) = entries.next_entry().await.map_err(|e| e.to_string())? {
            let file_name = entry.file_name().to_string_lossy().to_string();

            if file_name.starts_with('.') || file_name.ends_with(".md") {
                continue;
            }

            let metadata = entry.metadata().await.map_err(|e| e.to_string())?;
            let is_directory = metadata.is_dir();

            let item_path = if relative_path.is_empty() {
                file_name.clone()
            } else {
                format!("{}/{}", relative_path, file_name)
            };

            if is_directory {
                let item_count = self.count_images_in_directory(&item_path).await;
                items.push(GalleryItem {
                    name: file_name,
                    path: item_path,
                    is_directory: true,
                    thumbnail_url: None,
                    item_count: Some(item_count),
                });
            } else if self.is_image(&file_name) {
                let thumbnail_url = format!(
                    "/{}/image/{}?size=thumbnail",
                    self.config.path_prefix,
                    urlencoding::encode(&item_path)
                );

                items.push(GalleryItem {
                    name: file_name,
                    path: item_path,
                    is_directory: false,
                    thumbnail_url: Some(thumbnail_url),
                    item_count: None,
                });
            }
        }

        items.sort_by(|a, b| match (a.is_directory, b.is_directory) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => a.name.cmp(&b.name),
        });

        Ok(items)
    }

    async fn count_images_in_directory(&self, relative_path: &str) -> usize {
        let full_path = self.config.source_directory.join(relative_path);
        let mut count = 0;

        for entry in WalkDir::new(full_path).min_depth(1) {
            if let Ok(entry) = entry {
                if entry.file_type().is_file() {
                    if let Some(name) = entry.file_name().to_str() {
                        if self.is_image(name) && !name.starts_with('.') {
                            count += 1;
                        }
                    }
                }
            }
        }

        count
    }

    fn is_image(&self, filename: &str) -> bool {
        let lower = filename.to_lowercase();
        lower.ends_with(".jpg")
            || lower.ends_with(".jpeg")
            || lower.ends_with(".png")
            || lower.ends_with(".gif")
            || lower.ends_with(".webp")
            || lower.ends_with(".bmp")
    }

    pub async fn get_images_for_page(
        &self,
        relative_path: &str,
        page: usize,
    ) -> Result<Vec<ImageInfo>, String> {
        let items = self.scan_directory(relative_path).await?;
        let images: Vec<_> = items
            .into_iter()
            .filter(|item| !item.is_directory)
            .collect();

        let start = page * self.config.images_per_page;
        let end = ((page + 1) * self.config.images_per_page).min(images.len());

        let mut image_infos = Vec::new();

        for item in &images[start..end] {
            let image_info = self.get_image_info(&item.path).await?;
            image_infos.push(image_info);
        }

        Ok(image_infos)
    }

    pub async fn get_image_info(&self, relative_path: &str) -> Result<ImageInfo, String> {
        let full_path = self.config.source_directory.join(relative_path);

        if !full_path.starts_with(&self.config.source_directory) {
            return Err("Invalid path".to_string());
        }

        let metadata = tokio::fs::metadata(&full_path)
            .await
            .map_err(|e| format!("Failed to read file metadata: {}", e))?;

        let file_size = metadata.len();

        let img = image::open(&full_path).map_err(|e| format!("Failed to open image: {}", e))?;

        let dimensions = (img.width(), img.height());

        let description = self.read_sidecar_markdown(relative_path).await;
        let exif_data = self.extract_exif_data(&full_path).await;

        let encoded_path = urlencoding::encode(relative_path);

        Ok(ImageInfo {
            name: StdPath::new(relative_path)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("unknown")
                .to_string(),
            path: relative_path.to_string(),
            url: format!("/{}/image/{}", self.config.path_prefix, encoded_path),
            thumbnail_url: format!(
                "/{}/image/{}?size=thumbnail",
                self.config.path_prefix, encoded_path
            ),
            medium_url: format!(
                "/{}/image/{}?size=medium",
                self.config.path_prefix, encoded_path
            ),
            description,
            exif_data,
            file_size,
            dimensions,
        })
    }

    async fn read_sidecar_markdown(&self, image_path: &str) -> Option<String> {
        let path = StdPath::new(image_path);
        let stem = path.file_stem()?;
        let parent = path.parent()?;

        let md_filename = format!("{}.md", stem.to_str()?);
        let md_path = self.config.source_directory.join(parent).join(md_filename);

        match tokio::fs::read_to_string(&md_path).await {
            Ok(content) => {
                let parser = Parser::new(&content);
                let mut html_output = String::new();
                html::push_html(&mut html_output, parser);
                Some(html_output)
            }
            Err(_) => None,
        }
    }

    async fn extract_exif_data(&self, path: &PathBuf) -> Option<HashMap<String, String>> {
        let file_contents = std::fs::read(path).ok()?;

        match rexif::parse_buffer(&file_contents) {
            Ok(exif_data) => {
                let mut data = HashMap::new();

                for entry in &exif_data.entries {
                    let name = format!("{:?}", entry.tag);
                    let value = format!("{}", entry.value_more_readable);
                    data.insert(name, value);
                }

                Some(data)
            }
            Err(_) => None,
        }
    }

    pub async fn serve_image(&self, relative_path: &str, size: Option<String>) -> Response {
        let full_path = self.config.source_directory.join(relative_path);

        if !full_path.starts_with(&self.config.source_directory) {
            return (StatusCode::FORBIDDEN, "Forbidden").into_response();
        }

        let size = size.as_deref();

        if let Some(size) = size {
            match self
                .get_resized_image(&full_path, relative_path, size)
                .await
            {
                Ok(cached_path) => {
                    return self.serve_file(&cached_path).await;
                }
                Err(e) => {
                    error!("Failed to resize image: {}", e);
                }
            }
        }

        self.serve_file(&full_path).await
    }

    async fn get_resized_image(
        &self,
        original_path: &PathBuf,
        relative_path: &str,
        size: &str,
    ) -> Result<PathBuf, String> {
        let (width, height) = match size {
            "thumbnail" => (300, 300),
            "medium" => (800, 800),
            "large" => (1600, 1600),
            _ => return Err("Invalid size".to_string()),
        };

        let hash = self.generate_cache_key(relative_path, size);
        let cache_filename = format!("{}.jpg", hash);
        let cache_path = self.config.cache_directory.join(cache_filename);

        let original_metadata = tokio::fs::metadata(original_path)
            .await
            .map_err(|e| e.to_string())?;
        let original_modified = original_metadata.modified().map_err(|e| e.to_string())?;

        let cache = self.cache.read().await;
        if let Some(cached) = cache.get(&hash) {
            if cached.modified >= original_modified && cached.path.exists() {
                return Ok(cached.path.clone());
            }
        }
        drop(cache);

        tokio::fs::create_dir_all(&self.config.cache_directory)
            .await
            .map_err(|e| e.to_string())?;

        // Open image with decoder to access ICC profile
        let original_file = std::fs::File::open(original_path)
            .map_err(|e| format!("Failed to open original image: {}", e))?;

        let decoder = image::ImageReader::new(std::io::BufReader::new(original_file))
            .with_guessed_format()
            .map_err(|e| format!("Failed to create decoder: {}", e))?;

        let img = decoder
            .decode()
            .map_err(|e| format!("Failed to decode image: {}", e))?;

        let resized = img.resize(width, height, FilterType::Lanczos3);

        // Save resized image
        // Note: The standard image crate JPEG encoder doesn't support embedding ICC profiles
        // For production use, consider using a library like turbojpeg-sys or mozjpeg
        // that supports ICC profile embedding during encoding
        resized
            .save_with_format(&cache_path, ImageFormat::Jpeg)
            .map_err(|e| format!("Failed to save resized image: {}", e))?;

        let mut cache = self.cache.write().await;
        cache.insert(
            hash,
            CachedImage {
                path: cache_path.clone(),
                modified: original_modified,
            },
        );

        Ok(cache_path)
    }

    fn generate_cache_key(&self, path: &str, size: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(path);
        hasher.update(size);
        format!("{:x}", hasher.finalize())
    }

    async fn serve_file(&self, path: &PathBuf) -> Response {
        let file = match tokio::fs::File::open(path).await {
            Ok(file) => file,
            Err(_) => return (StatusCode::NOT_FOUND, "File not found").into_response(),
        };

        let content_type = mime_guess::from_path(path)
            .first_or_octet_stream()
            .to_string();

        let stream = ReaderStream::new(file);
        let body = Body::from_stream(stream);

        Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, content_type)
            .body(body)
            .unwrap()
    }
}

pub type SharedGallery = Arc<Gallery>;

pub async fn gallery_handler(
    State((template_engine, _, gallery)): State<(
        Arc<TemplateEngine>,
        StaticFileHandler,
        SharedGallery,
    )>,
    Path(path): Path<String>,
    Query(query): Query<GalleryQuery>,
) -> impl IntoResponse {
    let page = query.page.unwrap_or(0);

    let items = match gallery.scan_directory(&path).await {
        Ok(items) => items,
        Err(e) => {
            error!("Failed to scan gallery directory: {}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR).into_response();
        }
    };

    let images = match gallery.get_images_for_page(&path, page).await {
        Ok(images) => images,
        Err(e) => {
            error!("Failed to get images: {}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR).into_response();
        }
    };

    let total_images = items.iter().filter(|i| !i.is_directory).count();
    let total_pages =
        (total_images + gallery.config.images_per_page - 1) / gallery.config.images_per_page;

    let globals = liquid::object!({
        "gallery_path": path,
        "items": items,
        "images": images,
        "current_page": page,
        "total_pages": total_pages,
        "has_prev": page > 0,
        "has_next": page + 1 < total_pages,
        "prev_page": if page > 0 { page - 1 } else { 0 },
        "next_page": page + 1,
    });

    match template_engine
        .render_template("gallery.html.liquid", globals)
        .await
    {
        Ok(html) => Html(html).into_response(),
        Err(e) => {
            error!("Template rendering error: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR).into_response()
        }
    }
}

pub async fn image_detail_handler(
    State((template_engine, _, gallery)): State<(
        Arc<TemplateEngine>,
        StaticFileHandler,
        SharedGallery,
    )>,
    Path(path): Path<String>,
) -> impl IntoResponse {
    let image_info = match gallery.get_image_info(&path).await {
        Ok(info) => info,
        Err(e) => {
            error!("Failed to get image info: {}", e);
            return (StatusCode::NOT_FOUND).into_response();
        }
    };

    let globals = liquid::object!({
        "image": image_info,
    });

    match template_engine
        .render_template("image_detail.html.liquid", globals)
        .await
    {
        Ok(html) => Html(html).into_response(),
        Err(e) => {
            error!("Template rendering error: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR).into_response()
        }
    }
}

pub async fn image_handler(
    State((_, _, gallery)): State<(Arc<TemplateEngine>, StaticFileHandler, SharedGallery)>,
    Path(path): Path<String>,
    Query(query): Query<GalleryQuery>,
) -> impl IntoResponse {
    gallery.serve_image(&path, query.size).await
}
