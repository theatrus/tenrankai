use axum::{
    body::Bytes,
    extract::Path,
    http::{HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
};
use tracing::{debug, info, warn};

use super::UploadError;
use crate::login::AuthUser;
use crate::login::extractors::OptionalAuth;
use crate::permissions::{PermissionResolver, RolePermissions};
use crate::site::ResolvedState;
use crate::storage::create_chunked_storage_from_url;

const TUS_VERSION: &str = "1.0.0";
const TUS_EXTENSION: &str = "creation,termination";
const TUS_MAX_SIZE: u64 = 500 * 1024 * 1024; // 500MB

const ALLOWED_EXTENSIONS: &[&str] = &[
    // Images
    "jpg", "jpeg", "png", "webp", "avif", "heic", "heif", "gif", // RAW formats
    "raw", "cr2", "cr3", "nef", "arw", "dng", "orf", "rw2", "raf", "pef",
    // Sidecar files
    "md", "xmp",
];

fn sanitize_filename(filename: &str) -> Result<String, UploadError> {
    // Extract just the filename part, removing any path components
    let name = std::path::Path::new(filename)
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or(UploadError::InvalidFilename("Invalid filename"))?;

    // Reject if empty or contains suspicious patterns
    if name.is_empty() || name == "." || name == ".." {
        return Err(UploadError::InvalidFilename("Invalid filename"));
    }

    // Reject any remaining path separators (shouldn't happen after file_name(), but be safe)
    if name.contains('/') || name.contains('\\') {
        return Err(UploadError::InvalidFilename(
            "Filename cannot contain path separators",
        ));
    }

    // Reject null bytes
    if name.contains('\0') {
        return Err(UploadError::InvalidFilename(
            "Filename cannot contain null bytes",
        ));
    }

    Ok(name.to_string())
}

fn validate_file_extension(filename: &str) -> Result<(), UploadError> {
    let extension = std::path::Path::new(filename)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase());

    match extension {
        Some(ext) if ALLOWED_EXTENSIONS.contains(&ext.as_str()) => Ok(()),
        Some(ext) => Err(UploadError::InvalidFileType(ext)),
        None => Err(UploadError::InvalidFileType("no extension".to_string())),
    }
}

fn tus_headers() -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert("Tus-Resumable", HeaderValue::from_static(TUS_VERSION));
    headers.insert("Tus-Version", HeaderValue::from_static(TUS_VERSION));
    headers.insert("Tus-Extension", HeaderValue::from_static(TUS_EXTENSION));
    headers.insert(
        "Tus-Max-Size",
        HeaderValue::from_str(&TUS_MAX_SIZE.to_string()).unwrap(),
    );
    headers
}

fn verify_tus_version(headers: &HeaderMap) -> Result<(), UploadError> {
    match headers.get("tus-resumable") {
        Some(v) if v == TUS_VERSION => Ok(()),
        Some(_) => Err(UploadError::UnsupportedVersion),
        None => Ok(()),
    }
}

fn check_manage_images_permission(
    auth: Option<&AuthUser>,
    permissions: &RolePermissions,
) -> Result<(), UploadError> {
    if auth.is_none() {
        return Err(UploadError::Forbidden);
    }
    if !permissions.can_manage_images {
        return Err(UploadError::Forbidden);
    }
    Ok(())
}

async fn resolve_permissions(
    app_state: &crate::AppState,
    gallery_name: &str,
    path: &str,
    username: Option<&str>,
) -> Result<RolePermissions, UploadError> {
    let gallery = app_state
        .galleries()
        .get(gallery_name)
        .ok_or_else(|| UploadError::GalleryNotFound(gallery_name.to_string()))?;

    let folder_config = if !path.is_empty() {
        gallery
            .read_folder_metadata_full(path)
            .await
            .map(|meta| meta.config)
    } else {
        None
    };

    let resolver = PermissionResolver::new(
        &gallery.config.permissions,
        folder_config.as_ref().map(|fc| &fc.permissions),
    );

    resolver
        .resolve_user_permissions(username)
        .map_err(|e| UploadError::StorageError(e.to_string()))
}

pub async fn options_handler() -> impl IntoResponse {
    let headers = tus_headers();
    (StatusCode::NO_CONTENT, headers)
}

pub async fn create_upload(
    ResolvedState(app_state): ResolvedState,
    OptionalAuth(auth): OptionalAuth,
    Path(gallery_name): Path<String>,
    headers: HeaderMap,
) -> Result<Response, UploadError> {
    verify_tus_version(&headers)?;

    let upload_length = headers
        .get("upload-length")
        .ok_or(UploadError::MissingHeader("Upload-Length"))?
        .to_str()
        .map_err(|_| UploadError::InvalidHeader("Upload-Length"))?
        .parse::<u64>()
        .map_err(|_| UploadError::InvalidHeader("Upload-Length"))?;

    if upload_length > TUS_MAX_SIZE {
        return Err(UploadError::FileTooLarge {
            size: upload_length,
            max: TUS_MAX_SIZE,
        });
    }

    let upload_metadata = headers
        .get("upload-metadata")
        .and_then(|h| h.to_str().ok())
        .map(String::from);

    let parsed_metadata = parse_tus_metadata(&upload_metadata);

    let raw_filename = parsed_metadata
        .as_ref()
        .and_then(|m| m.get("filename").cloned())
        .ok_or(UploadError::MissingHeader("Upload-Metadata filename"))?;

    // Sanitize filename to prevent path traversal attacks
    let filename = sanitize_filename(&raw_filename)?;

    // Validate file extension
    validate_file_extension(&filename)?;

    let folder_path = parsed_metadata
        .as_ref()
        .and_then(|m| m.get("folderPath").cloned())
        .unwrap_or_default();

    let username = auth.as_ref().map(|a| a.username.as_str());
    let permissions =
        resolve_permissions(&app_state, &gallery_name, &folder_path, username).await?;
    check_manage_images_permission(auth.as_ref(), &permissions)?;

    let file_path = if folder_path.is_empty() {
        filename.clone()
    } else {
        format!("{}/{}", folder_path.trim_end_matches('/'), filename)
    };

    let gallery = app_state
        .galleries()
        .get(&gallery_name)
        .ok_or_else(|| UploadError::GalleryNotFound(gallery_name.clone()))?;

    let source_url = &gallery.config.source_directory;
    let chunked_storage = create_chunked_storage_from_url(source_url)
        .await
        .map_err(|e| UploadError::StorageError(e.to_string()))?;

    let upload_id = chunked_storage
        .create_upload(&file_path, upload_length, upload_metadata.as_deref())
        .await?;

    info!(
        gallery = %gallery_name,
        path = %file_path,
        upload_id = %upload_id,
        size = upload_length,
        "Created upload"
    );

    let location = format!("/_upload/{}/{}", gallery_name, upload_id);

    let mut response_headers = tus_headers();
    response_headers.insert(
        "Location",
        HeaderValue::from_str(&location).unwrap_or_else(|_| HeaderValue::from_static("")),
    );
    response_headers.insert("Upload-Offset", HeaderValue::from_static("0"));

    Ok((StatusCode::CREATED, response_headers).into_response())
}

pub async fn head_upload(
    ResolvedState(app_state): ResolvedState,
    OptionalAuth(auth): OptionalAuth,
    Path((gallery_name, upload_id)): Path<(String, String)>,
    headers: HeaderMap,
) -> Result<Response, UploadError> {
    verify_tus_version(&headers)?;

    let gallery = app_state
        .galleries()
        .get(&gallery_name)
        .ok_or_else(|| UploadError::GalleryNotFound(gallery_name.clone()))?;

    let source_url = &gallery.config.source_directory;
    let chunked_storage = create_chunked_storage_from_url(source_url)
        .await
        .map_err(|e| UploadError::StorageError(e.to_string()))?;

    let info = chunked_storage.get_upload_info(&upload_id).await?;

    let folder_path = std::path::Path::new(&info.path)
        .parent()
        .and_then(|p| p.to_str())
        .unwrap_or("");

    let username = auth.as_ref().map(|a| a.username.as_str());
    let permissions = resolve_permissions(&app_state, &gallery_name, folder_path, username).await?;
    check_manage_images_permission(auth.as_ref(), &permissions)?;

    let mut response_headers = tus_headers();
    response_headers.insert(
        "Upload-Offset",
        HeaderValue::from_str(&info.current_offset.to_string())
            .unwrap_or_else(|_| HeaderValue::from_static("0")),
    );
    response_headers.insert(
        "Upload-Length",
        HeaderValue::from_str(&info.total_size.to_string())
            .unwrap_or_else(|_| HeaderValue::from_static("0")),
    );
    response_headers.insert("Cache-Control", HeaderValue::from_static("no-store"));

    Ok((StatusCode::OK, response_headers).into_response())
}

pub async fn patch_upload(
    ResolvedState(app_state): ResolvedState,
    OptionalAuth(auth): OptionalAuth,
    Path((gallery_name, upload_id)): Path<(String, String)>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, UploadError> {
    verify_tus_version(&headers)?;

    let gallery = app_state
        .galleries()
        .get(&gallery_name)
        .ok_or_else(|| UploadError::GalleryNotFound(gallery_name.clone()))?;

    let source_url = &gallery.config.source_directory;
    let chunked_storage = create_chunked_storage_from_url(source_url)
        .await
        .map_err(|e| UploadError::StorageError(e.to_string()))?;

    let info = chunked_storage.get_upload_info(&upload_id).await?;

    let folder_path = std::path::Path::new(&info.path)
        .parent()
        .and_then(|p| p.to_str())
        .unwrap_or("");

    let username = auth.as_ref().map(|a| a.username.as_str());
    let permissions = resolve_permissions(&app_state, &gallery_name, folder_path, username).await?;
    check_manage_images_permission(auth.as_ref(), &permissions)?;

    let content_type = headers
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    if content_type != "application/offset+octet-stream" {
        return Err(UploadError::InvalidHeader(
            "Content-Type must be application/offset+octet-stream",
        ));
    }

    let upload_offset = headers
        .get("upload-offset")
        .ok_or(UploadError::MissingHeader("Upload-Offset"))?
        .to_str()
        .map_err(|_| UploadError::InvalidHeader("Upload-Offset"))?
        .parse::<u64>()
        .map_err(|_| UploadError::InvalidHeader("Upload-Offset"))?;

    debug!(
        upload_id = %upload_id,
        offset = upload_offset,
        chunk_size = body.len(),
        "Receiving chunk"
    );

    let new_offset = chunked_storage
        .append_chunk(&upload_id, upload_offset, body)
        .await?;

    let info = chunked_storage.get_upload_info(&upload_id).await?;
    let is_complete = new_offset >= info.total_size;

    if is_complete {
        chunked_storage.complete_upload(&upload_id).await?;

        info!(
            gallery = %gallery_name,
            path = %info.path,
            "Upload complete"
        );

        // Refresh metadata synchronously so the image appears immediately on page refresh
        if let Err(e) = gallery.refresh_single_image_metadata(&info.path).await {
            warn!(path = %info.path, error = %e, "Failed to refresh metadata for uploaded image");
        }

        // Refresh folder cache so the image appears in directory listings
        let folder_path = std::path::Path::new(&info.path)
            .parent()
            .and_then(|p| p.to_str())
            .unwrap_or("");
        if let Err(e) = gallery.refresh_single_folder_cache(folder_path).await {
            warn!(path = %folder_path, error = %e, "Failed to refresh folder cache after upload");
        }

        // Queue cache generation in the process-global priority queue.
        let site_name = app_state.site.name.clone();
        let gallery_key = format!("{}:{}", site_name, gallery_name);
        app_state
            .generation_manager()
            .enqueue_pregenerate(
                gallery_key,
                gallery.clone(),
                &info.path,
                crate::generation::GenerationPriority::Normal,
            )
            .await;
    }

    let mut response_headers = tus_headers();
    response_headers.insert(
        "Upload-Offset",
        HeaderValue::from_str(&new_offset.to_string())
            .unwrap_or_else(|_| HeaderValue::from_static("0")),
    );

    Ok((StatusCode::NO_CONTENT, response_headers).into_response())
}

pub async fn delete_upload(
    ResolvedState(app_state): ResolvedState,
    OptionalAuth(auth): OptionalAuth,
    Path((gallery_name, upload_id)): Path<(String, String)>,
    headers: HeaderMap,
) -> Result<Response, UploadError> {
    verify_tus_version(&headers)?;

    let gallery = app_state
        .galleries()
        .get(&gallery_name)
        .ok_or_else(|| UploadError::GalleryNotFound(gallery_name.clone()))?;

    let source_url = &gallery.config.source_directory;
    let chunked_storage = create_chunked_storage_from_url(source_url)
        .await
        .map_err(|e| UploadError::StorageError(e.to_string()))?;

    let info = match chunked_storage.get_upload_info(&upload_id).await {
        Ok(info) => info,
        Err(crate::storage::StorageError::UploadNotFound(_)) => {
            return Ok((StatusCode::NO_CONTENT, tus_headers()).into_response());
        }
        Err(e) => return Err(e.into()),
    };

    let folder_path = std::path::Path::new(&info.path)
        .parent()
        .and_then(|p| p.to_str())
        .unwrap_or("");

    let username = auth.as_ref().map(|a| a.username.as_str());
    let permissions = resolve_permissions(&app_state, &gallery_name, folder_path, username).await?;
    check_manage_images_permission(auth.as_ref(), &permissions)?;

    chunked_storage.terminate_upload(&upload_id).await?;

    info!(
        gallery = %gallery_name,
        upload_id = %upload_id,
        "Upload terminated"
    );

    Ok((StatusCode::NO_CONTENT, tus_headers()).into_response())
}

fn parse_tus_metadata(
    metadata: &Option<String>,
) -> Option<std::collections::HashMap<String, String>> {
    let metadata = metadata.as_ref()?;
    let mut result = std::collections::HashMap::new();

    for pair in metadata.split(',') {
        let parts: Vec<&str> = pair.trim().splitn(2, ' ').collect();
        if parts.is_empty() {
            continue;
        }

        let key = parts[0].to_string();
        let value = if parts.len() > 1 {
            use base64::Engine;
            base64::engine::general_purpose::STANDARD
                .decode(parts[1])
                .ok()
                .and_then(|bytes| String::from_utf8(bytes).ok())
                .unwrap_or_default()
        } else {
            String::new()
        };

        result.insert(key, value);
    }

    Some(result)
}
