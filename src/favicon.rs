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

use crate::static_files::StaticFileHandler;
use crate::templating::TemplateEngine;
use crate::gallery::SharedGallery;

#[derive(Clone)]
pub struct FaviconRenderer {
    static_dir: PathBuf,
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
    pub fn new(static_dir: PathBuf) -> Self {
        Self {
            static_dir,
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
                (StatusCode::INTERNAL_SERVER_ERROR, "Failed to generate favicon").into_response()
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
                (StatusCode::INTERNAL_SERVER_ERROR, "Failed to generate favicon PNG").into_response()
            }
        }
    }

    async fn load_svg(&self) -> Result<String, String> {
        let svg_path = self.static_dir.join("favicon.svg");
        tokio::fs::read_to_string(svg_path)
            .await
            .map_err(|e| format!("Failed to read favicon.svg: {}", e))
    }

    async fn generate_png(&self, size: u32) -> Result<Vec<u8>, String> {
        let svg_content = self.load_svg().await?;
        
        let rtree = usvg::Tree::from_str(&svg_content, &usvg::Options::default())
            .map_err(|e| format!("Failed to parse SVG: {}", e))?;

        let mut pixmap = tiny_skia::Pixmap::new(size, size)
            .ok_or("Failed to create pixmap")?;

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
                .write_image(rgba_image.as_raw(), size, size, image::ExtendedColorType::Rgba8)
                .map_err(|e| format!("Failed to encode PNG: {}", e))?;
        }

        Ok(png_data)
    }

    async fn generate_ico(&self) -> Result<Vec<u8>, String> {
        // Generate multiple sizes for ICO
        let png_16 = self.generate_png(16).await?;
        let png_32 = self.generate_png(32).await?;
        let png_48 = self.generate_png(48).await?;

        // Create ICO file
        let mut ico_dir = ico::IconDir::new(ico::ResourceType::Icon);
        
        ico_dir.add_entry(ico::IconDirEntry::encode(&ico::IconImage::from_rgba_data(16, 16, png_16))
            .map_err(|e| format!("Failed to encode 16x16 icon: {}", e))?);
        
        ico_dir.add_entry(ico::IconDirEntry::encode(&ico::IconImage::from_rgba_data(32, 32, png_32))
            .map_err(|e| format!("Failed to encode 32x32 icon: {}", e))?);
        
        ico_dir.add_entry(ico::IconDirEntry::encode(&ico::IconImage::from_rgba_data(48, 48, png_48))
            .map_err(|e| format!("Failed to encode 48x48 icon: {}", e))?);

        let mut ico_data = Vec::new();
        ico_dir.write(&mut ico_data)
            .map_err(|e| format!("Failed to write ICO: {}", e))?;

        Ok(ico_data)
    }
}

pub async fn favicon_ico_handler(
    State((_, _, _, favicon)): State<(
        Arc<TemplateEngine>,
        StaticFileHandler,
        SharedGallery,
        FaviconRenderer,
    )>,
) -> impl IntoResponse {
    favicon.render_favicon_ico().await
}

pub async fn favicon_png_16_handler(
    State((_, _, _, favicon)): State<(
        Arc<TemplateEngine>,
        StaticFileHandler,
        SharedGallery,
        FaviconRenderer,
    )>,
) -> impl IntoResponse {
    favicon.render_favicon_png(16).await
}

pub async fn favicon_png_32_handler(
    State((_, _, _, favicon)): State<(
        Arc<TemplateEngine>,
        StaticFileHandler,
        SharedGallery,
        FaviconRenderer,
    )>,
) -> impl IntoResponse {
    favicon.render_favicon_png(32).await
}

pub async fn favicon_png_48_handler(
    State((_, _, _, favicon)): State<(
        Arc<TemplateEngine>,
        StaticFileHandler,
        SharedGallery,
        FaviconRenderer,
    )>,
) -> impl IntoResponse {
    favicon.render_favicon_png(48).await
}