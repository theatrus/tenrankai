use axum::{
    body::Body,
    extract::State,
    http::{StatusCode, header},
    response::{IntoResponse, Response},
};
use image::{ImageBuffer, RgbaImage};
use resvg::{tiny_skia, usvg};
use std::{path::PathBuf, sync::Arc};
use tokio::sync::RwLock;
use tracing::error;

#[derive(Clone)]
pub struct FaviconRenderer {
    static_dirs: Vec<PathBuf>,
    cache: Arc<RwLock<FaviconCache>>,
}

#[derive(Default)]
struct FaviconCache {
    ico: Option<Vec<u8>>,
    png_16: Option<Vec<u8>>,
    png_32: Option<Vec<u8>>,
    png_48: Option<Vec<u8>>,
}

impl FaviconRenderer {
    pub fn new(static_dirs: Vec<PathBuf>) -> Self {
        Self {
            static_dirs,
            cache: Arc::new(RwLock::new(FaviconCache::default())),
        }
    }

    pub async fn render_favicon_ico(&self) -> Response {
        // Check cache first
        {
            let cache = self.cache.read().await;
            if let Some(ico_data) = &cache.ico {
                return Response::builder()
                    .status(StatusCode::OK)
                    .header(header::CONTENT_TYPE, "image/vnd.microsoft.icon")
                    .header(header::CACHE_CONTROL, "public, max-age=31536000, immutable")
                    .body(Body::from(ico_data.clone()))
                    .unwrap();
            }
        }

        // Generate ICO file
        match self.generate_ico().await {
            Ok(ico_data) => {
                // Cache the result
                {
                    let mut cache = self.cache.write().await;
                    cache.ico = Some(ico_data.clone());
                }

                Response::builder()
                    .status(StatusCode::OK)
                    .header(header::CONTENT_TYPE, "image/vnd.microsoft.icon")
                    .header(header::CACHE_CONTROL, "public, max-age=31536000, immutable")
                    .body(Body::from(ico_data))
                    .unwrap()
            }
            Err(e) => {
                error!("Failed to generate favicon.ico: {}", e);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Failed to generate favicon",
                )
                    .into_response()
            }
        }
    }

    pub async fn render_favicon_png(&self, size: u32) -> Response {
        // Check cache first
        // Check if the size is supported
        if size != 16 && size != 32 && size != 48 {
            return (StatusCode::BAD_REQUEST, "Unsupported PNG size").into_response();
        }

        {
            let cache = self.cache.read().await;
            let cached_data = match size {
                16 => &cache.png_16,
                32 => &cache.png_32,
                48 => &cache.png_48,
                _ => unreachable!(),
            };

            if let Some(png_data) = cached_data {
                return Response::builder()
                    .status(StatusCode::OK)
                    .header(header::CONTENT_TYPE, "image/png")
                    .header(header::CACHE_CONTROL, "public, max-age=31536000, immutable")
                    .body(Body::from(png_data.clone()))
                    .unwrap();
            }
        }

        // Generate PNG
        match self.generate_png(size).await {
            Ok(png_data) => {
                // Cache the result
                {
                    let mut cache = self.cache.write().await;
                    match size {
                        16 => cache.png_16 = Some(png_data.clone()),
                        32 => cache.png_32 = Some(png_data.clone()),
                        48 => cache.png_48 = Some(png_data.clone()),
                        _ => unreachable!(),
                    };
                }

                Response::builder()
                    .status(StatusCode::OK)
                    .header(header::CONTENT_TYPE, "image/png")
                    .header(header::CACHE_CONTROL, "public, max-age=31536000, immutable")
                    .body(Body::from(png_data))
                    .unwrap()
            }
            Err(e) => {
                error!("Failed to generate favicon PNG: {}", e);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Failed to generate favicon PNG",
                )
                    .into_response()
            }
        }
    }

    async fn load_svg(&self) -> Result<String, String> {
        // Try each directory in order until we find favicon.svg
        for (index, static_dir) in self.static_dirs.iter().enumerate() {
            let svg_path = static_dir.join("favicon.svg");
            match tokio::fs::read_to_string(&svg_path).await {
                Ok(content) => {
                    tracing::debug!("Found favicon.svg in directory {}: {:?}", index, svg_path);
                    return Ok(content);
                }
                Err(e) => {
                    tracing::debug!("favicon.svg not found in directory {}: {:?} - {}", index, svg_path, e);
                }
            }
        }
        Err("Failed to find favicon.svg in any static directory".to_string())
    }

    async fn generate_png(&self, size: u32) -> Result<Vec<u8>, String> {
        let svg_content = self.load_svg().await?;

        // Move CPU-intensive SVG rendering and PNG encoding to blocking thread pool
        tokio::task::spawn_blocking(move || -> Result<Vec<u8>, String> {
            let rtree = usvg::Tree::from_str(&svg_content, &usvg::Options::default())
                .map_err(|e| format!("Failed to parse SVG: {}", e))?;

            let mut pixmap = tiny_skia::Pixmap::new(size, size).ok_or("Failed to create pixmap")?;

            let transform = tiny_skia::Transform::from_scale(
                size as f32 / rtree.size().width(),
                size as f32 / rtree.size().height(),
            );

            resvg::render(&rtree, transform, &mut pixmap.as_mut());

            // Convert to PNG using image crate
            let rgba_image: RgbaImage = ImageBuffer::from_raw(size, size, pixmap.take())
                .ok_or("Failed to create image buffer")?;

            let mut png_data = Vec::new();
            {
                use image::{ImageEncoder, codecs::png::PngEncoder};
                let encoder = PngEncoder::new(&mut png_data);
                encoder
                    .write_image(
                        rgba_image.as_raw(),
                        size,
                        size,
                        image::ExtendedColorType::Rgba8,
                    )
                    .map_err(|e| format!("Failed to encode PNG: {}", e))?;
            }

            Ok(png_data)
        })
        .await
        .map_err(|e| format!("Task join error: {}", e))?
    }

    async fn generate_ico(&self) -> Result<Vec<u8>, String> {
        // Generate raw RGBA data for ICO (not PNG encoded)
        let (rgba_16_result, rgba_32_result, rgba_48_result) = tokio::join!(
            self.generate_rgba_data(16),
            self.generate_rgba_data(32),
            self.generate_rgba_data(48)
        );

        let rgba_16 = rgba_16_result?;
        let rgba_32 = rgba_32_result?;
        let rgba_48 = rgba_48_result?;

        // Create ICO file - this is also CPU intensive, so move to blocking pool
        tokio::task::spawn_blocking(move || -> Result<Vec<u8>, String> {
            let mut ico_dir = ico::IconDir::new(ico::ResourceType::Icon);

            ico_dir.add_entry(
                ico::IconDirEntry::encode(&ico::IconImage::from_rgba_data(16, 16, rgba_16))
                    .map_err(|e| format!("Failed to encode 16x16 icon: {}", e))?,
            );

            ico_dir.add_entry(
                ico::IconDirEntry::encode(&ico::IconImage::from_rgba_data(32, 32, rgba_32))
                    .map_err(|e| format!("Failed to encode 32x32 icon: {}", e))?,
            );

            ico_dir.add_entry(
                ico::IconDirEntry::encode(&ico::IconImage::from_rgba_data(48, 48, rgba_48))
                    .map_err(|e| format!("Failed to encode 48x48 icon: {}", e))?,
            );

            let mut ico_data = Vec::new();
            ico_dir
                .write(&mut ico_data)
                .map_err(|e| format!("Failed to write ICO: {}", e))?;

            Ok(ico_data)
        })
        .await
        .map_err(|e| format!("Task join error: {}", e))?
    }

    async fn generate_rgba_data(&self, size: u32) -> Result<Vec<u8>, String> {
        let svg_content = self.load_svg().await?;

        // Move CPU-intensive SVG rendering to blocking thread pool
        tokio::task::spawn_blocking(move || -> Result<Vec<u8>, String> {
            let rtree = usvg::Tree::from_str(&svg_content, &usvg::Options::default())
                .map_err(|e| format!("Failed to parse SVG: {}", e))?;

            let mut pixmap = tiny_skia::Pixmap::new(size, size).ok_or("Failed to create pixmap")?;

            let transform = tiny_skia::Transform::from_scale(
                size as f32 / rtree.size().width(),
                size as f32 / rtree.size().height(),
            );

            resvg::render(&rtree, transform, &mut pixmap.as_mut());

            // Return raw RGBA data (4 bytes per pixel)
            Ok(pixmap.take())
        })
        .await
        .map_err(|e| format!("Task join error: {}", e))?
    }
}

pub async fn favicon_ico_handler(State(app_state): State<crate::AppState>) -> impl IntoResponse {
    app_state.favicon_renderer.render_favicon_ico().await
}

pub async fn favicon_png_16_handler(State(app_state): State<crate::AppState>) -> impl IntoResponse {
    app_state.favicon_renderer.render_favicon_png(16).await
}

pub async fn favicon_png_32_handler(State(app_state): State<crate::AppState>) -> impl IntoResponse {
    app_state.favicon_renderer.render_favicon_png(32).await
}

pub async fn favicon_png_48_handler(State(app_state): State<crate::AppState>) -> impl IntoResponse {
    app_state.favicon_renderer.render_favicon_png(48).await
}
