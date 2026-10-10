use super::{Gallery, video::VideoState};
use crate::{ApiResponse, site::ResolvedState};
use axum::{
    body::Body,
    extract::Path,
    http::{HeaderMap, Method, StatusCode, header},
    response::{IntoResponse, Response},
};

#[derive(Debug, PartialEq, Eq)]
enum ByteRange {
    Full,
    Partial(u64, u64),
    Unsatisfiable,
}

fn parse_range(value: &str, size: u64) -> ByteRange {
    let Some(value) = value.strip_prefix("bytes=") else {
        return ByteRange::Full;
    };
    // Multiple ranges are optional; ignore them and stream the complete object.
    if value.contains(',') {
        return ByteRange::Full;
    }
    let Some((start, end)) = value.trim().split_once('-') else {
        return ByteRange::Unsatisfiable;
    };
    if size == 0 {
        return ByteRange::Unsatisfiable;
    }
    if start.is_empty() {
        let Ok(suffix) = end.parse::<u64>() else {
            return ByteRange::Unsatisfiable;
        };
        if suffix == 0 {
            return ByteRange::Unsatisfiable;
        }
        let length = suffix.min(size);
        return ByteRange::Partial(size - length, length);
    }
    let Ok(start) = start.parse::<u64>() else {
        return ByteRange::Unsatisfiable;
    };
    let end = if end.is_empty() {
        size - 1
    } else {
        let Ok(end) = end.parse::<u64>() else {
            return ByteRange::Unsatisfiable;
        };
        end.min(size - 1)
    };
    if start >= size || end < start {
        ByteRange::Unsatisfiable
    } else {
        ByteRange::Partial(start, end - start + 1)
    }
}

fn not_after(modified: Option<std::time::SystemTime>, value: &str) -> bool {
    let Ok(date) = httpdate::parse_http_date(value) else {
        return false;
    };
    modified
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .is_some_and(|t| {
            std::time::UNIX_EPOCH + std::time::Duration::from_secs(t.as_secs()) <= date
        })
}

pub async fn video_handler_for_named(
    ResolvedState(state): ResolvedState,
    Path((name, path)): Path<(String, String)>,
    method: Method,
    headers: HeaderMap,
    auth: crate::login::OptionalAuth,
) -> Response {
    let Some(gallery) = state.galleries().get(&name) else {
        return ApiResponse::NotFound.into_response();
    };
    let resolved = gallery
        .image_indexer
        .read()
        .await
        .get_path(&path)
        .unwrap_or(&path)
        .to_string();
    if !valid_path(&resolved)
        || gallery.config.video.is_none()
        || !tenrankai_video::is_video_path(&resolved)
    {
        return ApiResponse::NotFound.into_response();
    }
    let parent = resolved
        .rsplit_once('/')
        .map(|(parent, _)| parent)
        .unwrap_or("");
    let permissions = match crate::permissions::resolve_permissions_for_path(
        &state,
        &name,
        parent,
        auth.username(),
    )
    .await
    {
        Ok(p) => p.permissions,
        Err(_) => return ApiResponse::InternalServerError.into_response(),
    };
    if !permissions.can_view {
        return ApiResponse::NotFound.into_response();
    }
    if !permissions.can_see_hidden
        && let Some(folder) = gallery.get_cached_folder_data(parent).await
        && folder.metadata.as_ref().is_some_and(|m| {
            m.config
                .hidden_images
                .iter()
                .any(|p| Some(p.as_str()) == resolved.rsplit('/').next())
        })
    {
        return ApiResponse::NotFound.into_response();
    }
    gallery
        .serve_video_asset(&resolved, VideoAsset::Playback, &headers, &method)
        .await
}

fn valid_path(path: &str) -> bool {
    !path.is_empty()
        && !path.starts_with('/')
        && !path.contains('\\')
        && !path
            .split('/')
            .any(|part| part == "." || part == ".." || part.is_empty())
}

#[derive(Clone, Copy)]
pub(crate) enum VideoAsset {
    Playback,
    Poster,
    Original,
}

impl Gallery {
    pub(crate) async fn serve_video_asset(
        &self,
        path: &str,
        asset: VideoAsset,
        request: &HeaderMap,
        method: &Method,
    ) -> Response {
        if !valid_path(path) || self.config.video.is_none() || !tenrankai_video::is_video_path(path)
        {
            return ApiResponse::NotFound.into_response();
        }
        let source_metadata = match self.source_storage.metadata(path).await {
            Ok(metadata) => metadata,
            Err(_) => return ApiResponse::NotFound.into_response(),
        };
        let record = self.video_record(path).await;
        let current = record.as_ref().is_some_and(|r| {
            self.config.video.as_ref().is_some_and(|settings| {
                r.identity == self.video_identity(path, &source_metadata, settings)
            })
        });
        let (storage, key, content_type, etag) = match asset {
            VideoAsset::Original => (
                &self.source_storage,
                path.to_string(),
                match path
                    .rsplit('.')
                    .next()
                    .unwrap_or("")
                    .to_ascii_lowercase()
                    .as_str()
                {
                    "mov" => "video/quicktime",
                    "webm" => "video/webm",
                    "mkv" => "video/x-matroska",
                    _ => "video/mp4",
                },
                format!(
                    "\"{}\"",
                    self.video_identity(
                        path,
                        &source_metadata,
                        self.config.video.as_ref().unwrap()
                    )
                ),
            ),
            VideoAsset::Playback | VideoAsset::Poster => {
                let ready = record.as_ref().is_some_and(|r| {
                    if matches!(asset, VideoAsset::Poster) {
                        r.poster_ready
                    } else {
                        r.state == VideoState::Ready
                    }
                });
                if !current || !ready {
                    self.video_wakeup.notify_one();
                    if matches!(asset, VideoAsset::Poster) {
                        let label = if record
                            .as_ref()
                            .is_some_and(|r| r.state == VideoState::Failed)
                        {
                            "Video unavailable"
                        } else {
                            "Preparing video"
                        };
                        let svg = format!(
                            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 640 360"><rect width="640" height="360" fill="#25282d"/><path d="M295 125v70l60-35z" fill="#ddd"/><text x="320" y="245" text-anchor="middle" font-family="sans-serif" font-size="20" fill="#ddd">{label}</text></svg>"##
                        );
                        return (
                            [
                                (header::CONTENT_TYPE, "image/svg+xml"),
                                (header::CACHE_CONTROL, "private, no-store"),
                            ],
                            svg,
                        )
                            .into_response();
                    }
                    return (
                        StatusCode::ACCEPTED,
                        [
                            (header::RETRY_AFTER, "5"),
                            (header::CACHE_CONTROL, "private, no-store"),
                        ],
                        "Video is being prepared",
                    )
                        .into_response();
                }
                let record = record.as_ref().unwrap();
                let (filename, mime) = if matches!(asset, VideoAsset::Poster) {
                    ("poster.jpg", "image/jpeg")
                } else {
                    ("playback.mp4", "video/mp4")
                };
                (
                    &self.cache_storage,
                    format!("{}/{filename}", record.artifact_prefix),
                    mime,
                    format!("\"{}-{filename}\"", record.identity),
                )
            }
        };
        let metadata = match storage.metadata(&key).await {
            Ok(m) => m,
            Err(_) => return ApiResponse::NotFound.into_response(),
        };
        // Include the published object's validator: a regenerated rendition
        // can have different bytes even when its original source is unchanged.
        let etag = format!(
            "\"{}\"",
            self.video_identity(
                &format!("{key}\0{etag}"),
                &metadata,
                self.config.video.as_ref().unwrap()
            )
        );
        let mut response_headers = HeaderMap::new();
        response_headers.insert(header::CACHE_CONTROL, "private, no-store".parse().unwrap());
        response_headers.insert(header::ACCEPT_RANGES, "bytes".parse().unwrap());
        response_headers.insert(header::CONTENT_TYPE, content_type.parse().unwrap());
        response_headers.insert(header::ETAG, etag.parse().unwrap());
        response_headers.insert("x-content-type-options", "nosniff".parse().unwrap());
        if let Some(modified) = metadata.last_modified.filter(|time| {
            // httpdate supports dates from 1970 through 9999; older filesystem
            // timestamps must not panic the serving task.
            time.duration_since(std::time::UNIX_EPOCH)
                .is_ok_and(|elapsed| elapsed.as_secs() < 253_402_300_800)
        }) && let Ok(value) = httpdate::fmt_http_date(modified).parse()
        {
            response_headers.insert(header::LAST_MODIFIED, value);
        }
        if matches!(asset, VideoAsset::Original) {
            let filename = urlencoding::encode(path.rsplit('/').next().unwrap_or("video"));
            if let Ok(value) = format!("attachment; filename*=UTF-8''{filename}").parse() {
                response_headers.insert(header::CONTENT_DISPOSITION, value);
            }
        }
        let get_header = |name| request.get(name).and_then(|v| v.to_str().ok());
        let not_modified = if let Some(value) = get_header(header::IF_NONE_MATCH) {
            value
                .split(',')
                .any(|tag| tag.trim() == "*" || tag.trim().trim_start_matches("W/") == etag)
        } else {
            get_header(header::IF_MODIFIED_SINCE)
                .is_some_and(|value| not_after(metadata.last_modified, value))
        };
        if not_modified {
            return (StatusCode::NOT_MODIFIED, response_headers, Body::empty()).into_response();
        }
        let range_allowed = get_header(header::IF_RANGE)
            .is_none_or(|value| value == etag || not_after(metadata.last_modified, value));
        let range = if method == Method::GET && range_allowed {
            get_header(header::RANGE)
                .map(|v| parse_range(v, metadata.size))
                .unwrap_or(ByteRange::Full)
        } else {
            ByteRange::Full
        };
        let (status, offset, length) = match range {
            ByteRange::Unsatisfiable => {
                response_headers.insert(
                    header::CONTENT_RANGE,
                    format!("bytes */{}", metadata.size).parse().unwrap(),
                );
                return (
                    StatusCode::RANGE_NOT_SATISFIABLE,
                    response_headers,
                    Body::empty(),
                )
                    .into_response();
            }
            ByteRange::Partial(offset, length) => {
                response_headers.insert(
                    header::CONTENT_RANGE,
                    format!("bytes {}-{}/{}", offset, offset + length - 1, metadata.size)
                        .parse()
                        .unwrap(),
                );
                (StatusCode::PARTIAL_CONTENT, offset, length)
            }
            ByteRange::Full => (StatusCode::OK, 0, metadata.size),
        };
        response_headers.insert(header::CONTENT_LENGTH, length.to_string().parse().unwrap());
        if method == Method::HEAD {
            return (status, response_headers, Body::empty()).into_response();
        }
        match storage.read_range_stream(&key, offset, length).await {
            Ok(stream) => (status, response_headers, Body::from_stream(stream)).into_response(),
            Err(error) => {
                tracing::warn!(%error, "Cannot stream video asset");
                ApiResponse::InternalServerError.into_response()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[tokio::test]
    async fn streams_ranges_head_and_validators_and_rejects_stale_artifacts() {
        let source = tempfile::tempdir().unwrap();
        let cache = tempfile::tempdir().unwrap();
        let gallery = Gallery::new(
            crate::GallerySystemConfig {
                source_directory: source.path().display().to_string(),
                cache_directory: cache.path().display().to_string(),
                video: Some(Default::default()),
                ..Default::default()
            },
            Arc::new(crate::storage::FilesystemStorage::new(source.path())),
            Arc::new(crate::storage::FilesystemStorage::new(cache.path())),
        );
        gallery
            .source_storage
            .write("clip.mov", bytes::Bytes::from_static(b"original"))
            .await
            .unwrap();
        gallery
            .cache_storage
            .write(
                "video/test/playback.mp4",
                bytes::Bytes::from_static(b"0123456789"),
            )
            .await
            .unwrap();
        let metadata = gallery.source_storage.metadata("clip.mov").await.unwrap();
        let identity = gallery.video_identity(
            "clip.mov",
            &metadata,
            gallery.config.video.as_ref().unwrap(),
        );
        gallery.video_records.write().await.insert(
            "clip.mov".into(),
            super::super::video::VideoRecord {
                identity,
                artifact_prefix: "video/test".into(),
                state: VideoState::Ready,
                attempts: 1,
                retry_after: std::time::SystemTime::UNIX_EPOCH,
                metadata: None,
                poster_ready: true,
                error: None,
            },
        );
        let mut headers = HeaderMap::new();
        headers.insert(header::RANGE, "bytes=2-5".parse().unwrap());
        let response = gallery
            .serve_video_asset("clip.mov", VideoAsset::Playback, &headers, &Method::GET)
            .await;
        assert_eq!(response.status(), StatusCode::PARTIAL_CONTENT);
        assert_eq!(response.headers()[header::CONTENT_RANGE], "bytes 2-5/10");
        assert_eq!(response.headers()[header::CONTENT_LENGTH], "4");
        assert_eq!(
            response.headers()[header::CACHE_CONTROL],
            "private, no-store"
        );
        let etag = response.headers()[header::ETAG].clone();
        assert_eq!(
            axum::body::to_bytes(response.into_body(), 100)
                .await
                .unwrap()
                .as_ref(),
            b"2345"
        );
        let response = gallery
            .serve_video_asset("clip.mov", VideoAsset::Playback, &headers, &Method::HEAD)
            .await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()[header::CONTENT_LENGTH], "10");
        assert!(
            axum::body::to_bytes(response.into_body(), 100)
                .await
                .unwrap()
                .is_empty()
        );
        headers.insert(header::IF_RANGE, "\"old\"".parse().unwrap());
        assert_eq!(
            gallery
                .serve_video_asset("clip.mov", VideoAsset::Playback, &headers, &Method::GET)
                .await
                .status(),
            StatusCode::OK
        );
        headers.remove(header::IF_RANGE);
        headers.insert(header::IF_NONE_MATCH, etag.clone());
        assert_eq!(
            gallery
                .serve_video_asset("clip.mov", VideoAsset::Playback, &headers, &Method::GET)
                .await
                .status(),
            StatusCode::NOT_MODIFIED
        );
        headers.remove(header::IF_NONE_MATCH);
        headers.insert(header::RANGE, "bytes=10-".parse().unwrap());
        let response = gallery
            .serve_video_asset("clip.mov", VideoAsset::Playback, &headers, &Method::GET)
            .await;
        assert_eq!(response.status(), StatusCode::RANGE_NOT_SATISFIABLE);
        assert_eq!(response.headers()[header::CONTENT_RANGE], "bytes */10");
        gallery
            .cache_storage
            .write(
                "video/test/playback.mp4",
                bytes::Bytes::from_static(b"regenerated rendition"),
            )
            .await
            .unwrap();
        let mut validators = HeaderMap::new();
        validators.insert(header::IF_NONE_MATCH, etag.clone());
        let recreated = gallery
            .serve_video_asset("clip.mov", VideoAsset::Playback, &validators, &Method::GET)
            .await;
        assert_eq!(recreated.status(), StatusCode::OK);
        assert_ne!(recreated.headers()[header::ETAG], etag);
        gallery
            .source_storage
            .write("clip.mov", bytes::Bytes::from_static(b"changed source"))
            .await
            .unwrap();
        assert_eq!(
            gallery
                .serve_video_asset(
                    "clip.mov",
                    VideoAsset::Playback,
                    &HeaderMap::new(),
                    &Method::GET
                )
                .await
                .status(),
            StatusCode::ACCEPTED
        );
        gallery.source_storage.delete("clip.mov").await.unwrap();
        assert_eq!(
            gallery
                .serve_video_asset(
                    "clip.mov",
                    VideoAsset::Playback,
                    &HeaderMap::new(),
                    &Method::GET
                )
                .await
                .status(),
            StatusCode::NOT_FOUND
        );
    }

    #[test]
    fn parses_seek_suffix_and_unsatisfiable_ranges() {
        assert_eq!(parse_range("bytes=2-5", 10), ByteRange::Partial(2, 4));
        assert_eq!(parse_range("bytes=5-", 10), ByteRange::Partial(5, 5));
        assert_eq!(parse_range("bytes=-4", 10), ByteRange::Partial(6, 4));
        assert_eq!(parse_range("bytes=-100", 10), ByteRange::Partial(0, 10));
        for range in ["bytes=10-", "bytes=5-2", "bytes=-0", "bytes=oops"] {
            assert_eq!(parse_range(range, 10), ByteRange::Unsatisfiable);
        }
        assert_eq!(parse_range("bytes=0-2,4-5", 10), ByteRange::Full);
        assert_eq!(parse_range("items=1-3", 10), ByteRange::Full);
    }
}
