//! Persistent, bounded preparation of short gallery clips.
use super::{CameraInfo, Gallery, ImageMetadata, LocationInfo, SharedGallery};
use crate::storage::ObjectMetadata;
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::time::{Duration, SystemTime};
use tenrankai_video::{VideoMetadata, VideoProcessor, VideoSettings, VideoTools};
use tokio::io::AsyncWriteExt;

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum VideoState {
    #[default]
    Pending,
    Processing,
    Ready,
    Failed,
}

/// Private manifest. Never serialize this object into a public API response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct VideoRecord {
    pub identity: String,
    pub artifact_prefix: String,
    pub state: VideoState,
    pub attempts: u32,
    pub retry_after: SystemTime,
    pub metadata: Option<VideoMetadata>,
    pub poster_ready: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct VideoInfo {
    pub state: VideoState,
    pub duration_seconds: Option<f64>,
    pub playback_url: Option<String>,
    pub poster_url: Option<String>,
    /// Technical fields are only populated for authorized viewers.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub codec: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frame_rate: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub has_audio: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hdr: Option<bool>,
}

fn hash(data: impl AsRef<[u8]>) -> String {
    hex::encode(Sha256::digest(data.as_ref()))
}

impl Gallery {
    fn video_namespace(&self) -> String {
        hash(format!(
            "{}\0{}",
            self.config.name, self.config.source_directory
        ))
    }

    fn video_manifest_path(&self, path: &str) -> String {
        format!("video/{}/{}.json", self.video_namespace(), hash(path))
    }

    pub(crate) fn video_identity(
        &self,
        path: &str,
        metadata: &ObjectMetadata,
        settings: &VideoSettings,
    ) -> String {
        hash(
            serde_json::to_vec(&(
                tenrankai_video::PIPELINE_VERSION,
                self.video_namespace(),
                path,
                metadata.size,
                metadata.last_modified.map(|time| {
                    let (before_epoch, elapsed) = match time.duration_since(SystemTime::UNIX_EPOCH)
                    {
                        Ok(elapsed) => (false, elapsed),
                        Err(error) => (true, error.duration()),
                    };
                    (before_epoch, elapsed.as_secs(), elapsed.subsec_nanos())
                }),
                &metadata.etag,
                settings,
            ))
            .expect("validated video settings serialize"),
        )
    }

    pub(crate) async fn video_record(&self, path: &str) -> Option<VideoRecord> {
        self.video_records.read().await.get(path).cloned()
    }

    async fn save_video_record(
        &self,
        path: &str,
        record: &VideoRecord,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Small manifests can use buffered storage; media bytes always stream.
        self.cache_storage
            .write(
                &self.video_manifest_path(path),
                serde_json::to_vec(record)?.into(),
            )
            .await?;
        self.video_records
            .write()
            .await
            .insert(path.to_string(), record.clone());
        Ok(())
    }

    async fn discard_video_artifacts(&self, record: &VideoRecord) {
        for filename in ["poster.jpg", "playback.mp4"] {
            let key = format!("{}/{filename}", record.artifact_prefix);
            if let Err(error) = self.cache_storage.delete(&key).await {
                tracing::debug!(%error, key, "Cannot remove obsolete video artifact");
            }
        }
    }

    pub(crate) async fn video_info(
        &self,
        path: &str,
        url_id: &str,
        technical: bool,
    ) -> Option<VideoInfo> {
        if self.config.video.is_none() || !tenrankai_video::is_video_path(path) {
            return None;
        }
        let record = self.video_record(path).await;
        let metadata = record.as_ref().and_then(|r| r.metadata.as_ref());
        let state = record.as_ref().map(|r| r.state).unwrap_or_default();
        Some(VideoInfo {
            state,
            duration_seconds: metadata.map(|m| m.duration_seconds),
            playback_url: (state == VideoState::Ready)
                .then(|| format!("{}/_video/{url_id}", self.config.url_prefix)),
            poster_url: record
                .as_ref()
                .is_some_and(|r| r.poster_ready)
                .then(|| self.build_medium_url(url_id)),
            codec: technical
                .then(|| metadata.map(|m| m.video.codec.clone()))
                .flatten(),
            frame_rate: if technical {
                metadata.and_then(|m| m.video.frame_rate)
            } else {
                None
            },
            has_audio: technical
                .then(|| metadata.map(|m| m.audio.is_some()))
                .flatten(),
            hdr: technical
                .then(|| metadata.map(|m| m.video.is_hdr))
                .flatten(),
        })
    }

    pub(crate) async fn video_image_metadata(
        &self,
        path: &str,
        modified: Option<SystemTime>,
    ) -> ImageMetadata {
        let record = self.video_record(path).await;
        let source = record.as_ref().and_then(|r| r.metadata.as_ref());
        let camera = source.map(|m| CameraInfo {
            camera_make: m.camera_make.clone(),
            camera_model: m.camera_model.clone(),
            lens_model: m.lens_model.clone(),
            iso: None,
            aperture: None,
            shutter_speed: None,
            focal_length: None,
            telescope: None,
            mount: None,
            filters: None,
            total_exposure_time: None,
            ra: None,
            dec: None,
            additional_details: None,
        });
        let location = source
            .and_then(|m| m.location.as_ref())
            .map(|l| LocationInfo {
                latitude: l.latitude,
                longitude: l.longitude,
                google_maps_url: format!(
                    "https://maps.google.com/?q={},{}",
                    l.latitude, l.longitude
                ),
                apple_maps_url: format!("https://maps.apple.com/?q={},{}", l.latitude, l.longitude),
            });
        let sidecars = super::path_utils::SidecarPaths::for_image(path);
        let xmp = super::metadata_sources::read_xmp_metadata_from_storage(
            &self.source_storage,
            &sidecars.xmp,
        )
        .await;
        let user = self.user_metadata_storage.load(path).await.ok().flatten();
        let xmp_capture_date = xmp.as_ref().and_then(|m| m.capture_date);
        let (camera_info, location_info) =
            super::metadata_sources::merge_metadata_sources(camera, location, xmp, user.as_ref());
        let capture_date = user
            .as_ref()
            .and_then(|m| m.capture_date.as_ref())
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
            .map(SystemTime::from)
            .or(xmp_capture_date)
            .or_else(|| {
                source
                    .and_then(|m| m.capture_time.as_ref())
                    .and_then(|t| t.utc)
                    .map(SystemTime::from)
            });
        ImageMetadata {
            dimensions: source
                .map(|m| m.video.display_dimensions)
                .unwrap_or((640, 360)),
            capture_date,
            camera_info,
            location_info,
            modification_date: modified,
            color_profile: None,
            preview_ready: record.as_ref().is_some_and(|r| r.poster_ready),
        }
    }

    pub async fn start_video_worker(gallery: SharedGallery) {
        if gallery.config.video.is_none() {
            return;
        }
        let mut task = gallery.video_task.lock().await;
        if task.is_some() {
            return;
        }
        let worker = gallery.clone();
        *task = Some(tokio::spawn(async move {
            loop {
                if worker.shutdown_token.is_cancelled() {
                    break;
                }
                let paths: Vec<_> = worker
                    .image_cache
                    .read_all()
                    .await
                    .keys()
                    .filter(|p| tenrankai_video::is_video_path(p))
                    .cloned()
                    .collect();
                let removed: Vec<_> = worker
                    .video_records
                    .read()
                    .await
                    .iter()
                    .filter(|(path, _)| !paths.contains(path))
                    .map(|(path, record)| (path.clone(), record.clone()))
                    .collect();
                for (path, record) in removed {
                    // A refresh can briefly omit a file; confirm deletion at the source.
                    if matches!(worker.source_storage.exists(&path).await, Ok(false)) {
                        worker.discard_video_artifacts(&record).await;
                        let _ = worker
                            .cache_storage
                            .delete(&worker.video_manifest_path(&path))
                            .await;
                        worker.video_records.write().await.remove(&path);
                    }
                }
                for path in paths {
                    if worker.shutdown_token.is_cancelled() {
                        break;
                    }
                    if let Err(error) = worker.prepare_gallery_video(&path).await {
                        tracing::warn!(gallery = %worker.config.name, path, %error, "Video preparation failed");
                    }
                }
                tokio::select! {
                    _ = worker.shutdown_token.cancelled() => break,
                    _ = worker.video_wakeup.notified() => {},
                    _ = tokio::time::sleep(Duration::from_secs(15)) => {},
                }
            }
        }));
    }

    pub(crate) async fn prepare_gallery_video(
        &self,
        path: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let task = self
            .task_deduplicator
            .should_execute(format!("video:{path}"))
            .await;
        if !task.is_executor() {
            return Ok(());
        }
        // Queue-dispatched jobs also need to drain before a reload starts the
        // next worker; joining the gallery's discovery task alone is insufficient.
        let _task_guard = self.track_blocking_task();
        if self.shutdown_token.is_cancelled() {
            return Ok(());
        }
        let result = self.prepare_gallery_video_inner(path).await;
        task.complete().await;
        result
    }

    async fn prepare_gallery_video_inner(
        &self,
        path: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let settings = self.config.video.as_ref().ok_or("video disabled")?;
        settings.validate()?;
        let original = self.source_storage.metadata(path).await?;
        let identity = self.video_identity(path, &original, settings);
        let previous = match self.video_record(path).await {
            Some(record) => Some(record),
            None => self
                .cache_storage
                .read(&self.video_manifest_path(path))
                .await
                .ok()
                .and_then(|data| serde_json::from_slice::<VideoRecord>(&data).ok()),
        };
        if let Some(old) = previous.as_ref().filter(|r| r.identity != identity) {
            self.discard_video_artifacts(old).await;
        }
        let mut record = previous
            .filter(|r| r.identity == identity)
            .unwrap_or(VideoRecord {
                artifact_prefix: format!("video/{}/{identity}", self.video_namespace()),
                identity,
                state: VideoState::Pending,
                attempts: 0,
                retry_after: SystemTime::UNIX_EPOCH,
                metadata: None,
                poster_ready: false,
                error: None,
            });
        if record.state == VideoState::Ready {
            if self
                .cache_storage
                .exists(&format!("{}/playback.mp4", record.artifact_prefix))
                .await?
                && self
                    .cache_storage
                    .exists(&format!("{}/poster.jpg", record.artifact_prefix))
                    .await?
            {
                self.video_records
                    .write()
                    .await
                    .insert(path.to_string(), record);
                let metadata = self
                    .video_image_metadata(path, original.last_modified)
                    .await;
                self.insert_metadata_with_tracking(path.to_string(), metadata)
                    .await;
                return Ok(());
            }
            record.state = VideoState::Pending;
        }
        if record.state == VideoState::Failed
            && (record.attempts >= 3 || SystemTime::now() < record.retry_after)
        {
            self.video_records
                .write()
                .await
                .insert(path.to_string(), record);
            return Ok(());
        }
        // Synced files must be settled. Upload completion wakes the worker, and
        // the next tick will prepare files younger than this small quiet period.
        if original.last_modified.is_some_and(|t| {
            SystemTime::now()
                .duration_since(t)
                .is_ok_and(|age| age < Duration::from_secs(3))
        }) {
            self.video_records
                .write()
                .await
                .insert(path.to_string(), record);
            return Ok(());
        }
        // Share the existing gallery processing budget with image work. Acquire
        // before staging so queued clips cannot exhaust scratch space either.
        let _permit = tokio::select! {
            biased;
            _ = self.shutdown_token.cancelled() => return Ok(()),
            permit = self.image_processing_semaphore.acquire() => permit?,
        };
        record.state = VideoState::Processing;
        record.attempts += 1;
        record.error = None;
        self.save_video_record(path, &record).await?;
        let result = self
            .process_video_artifacts(path, &original, settings, &mut record)
            .await;
        match &result {
            Ok(()) => record.state = VideoState::Ready,
            Err(error) => {
                record.state = if self.shutdown_token.is_cancelled() {
                    VideoState::Pending
                } else {
                    VideoState::Failed
                };
                record.retry_after =
                    SystemTime::now() + Duration::from_secs(60 * u64::from(record.attempts.min(3)));
                record.error = Some(error.to_string());
            }
        }
        self.save_video_record(path, &record).await?;
        self.insert_metadata_with_tracking(
            path.to_string(),
            self.video_image_metadata(path, original.last_modified)
                .await,
        )
        .await;
        self.save_metadata_cache().await?;
        result
    }

    async fn process_video_artifacts(
        &self,
        path: &str,
        original: &ObjectMetadata,
        settings: &VideoSettings,
        record: &mut VideoRecord,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if original.size > settings.max_source_bytes {
            return Err("video source exceeds the configured size limit".into());
        }
        let tools = VideoTools::default();
        let required = settings
            .max_source_bytes
            .checked_add(settings.max_output_bytes)
            .and_then(|n| n.checked_add(tools.min_free_space_bytes + 32 * 1024 * 1024))
            .ok_or("video disk budget overflow")?;
        // Use the prerequisite report for filesystem space and service-user access.
        let processor = VideoProcessor::new(tools.clone())?;
        let report = processor.check_prerequisites(&self.shutdown_token).await?;
        if !report.ready() || report.available_space_bytes < required {
            return Err(
                "video prerequisites or scratch disk space are insufficient; run video doctor"
                    .into(),
            );
        }
        let scratch = tempfile::Builder::new()
            .prefix("tenrankai-video-source-")
            .tempdir_in(&tools.temp_directory)?;
        let input = scratch.path().join("source");
        let mut file = tokio::fs::File::create(&input).await?;
        let mut stream = self.source_storage.read_stream(path).await?;
        let mut length = 0u64;
        loop {
            let chunk = tokio::select! {
                _ = self.shutdown_token.cancelled() => return Err("video preparation cancelled".into()),
                chunk = tokio::time::timeout(Duration::from_secs(60), stream.next()) => chunk.map_err(|_| "video source download timed out")?,
            };
            let Some(chunk) = chunk else {
                break;
            };
            let chunk = chunk?;
            length = length
                .checked_add(chunk.len() as u64)
                .ok_or("video source size overflow")?;
            if length > settings.max_source_bytes || length > original.size {
                return Err("video source changed or exceeds the size limit".into());
            }
            file.write_all(&chunk).await?;
        }
        file.flush().await?;
        drop(file);
        if length != original.size {
            return Err("video source changed during download".into());
        }
        let source = processor.probe(&input, &self.shutdown_token).await?;
        settings.plan(&source)?;
        record.metadata = Some(source);
        self.save_video_record(path, record).await?;
        self.insert_metadata_with_tracking(
            path.to_string(),
            self.video_image_metadata(path, original.last_modified)
                .await,
        )
        .await;
        let prepared = processor
            .prepare(&input, settings, None, &self.shutdown_token)
            .await?;
        let current = self.source_storage.metadata(path).await?;
        if self.video_identity(path, &current, settings) != record.identity {
            return Err("video source changed during preparation".into());
        }
        self.cache_storage
            .write_file(
                &format!("{}/poster.jpg", record.artifact_prefix),
                &prepared.poster_path(),
            )
            .await?;
        self.cache_storage
            .write_file(
                &format!("{}/playback.mp4", record.artifact_prefix),
                &prepared.playback_path(),
            )
            .await?;
        let current = self.source_storage.metadata(path).await?;
        if self.video_identity(path, &current, settings) != record.identity {
            return Err("video source changed before publication".into());
        }
        record.poster_ready = true;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn gallery(
        source: &std::path::Path,
        cache: &std::path::Path,
        settings: VideoSettings,
    ) -> Gallery {
        Gallery::new(
            crate::GallerySystemConfig {
                source_directory: source.display().to_string(),
                cache_directory: cache.display().to_string(),
                video: Some(settings),
                ..Default::default()
            },
            Arc::new(crate::storage::FilesystemStorage::new(source)),
            Arc::new(crate::storage::FilesystemStorage::new(cache)),
        )
    }

    #[test]
    fn identity_supports_pre_epoch_source_timestamps() {
        let source = tempfile::tempdir().unwrap();
        let cache = tempfile::tempdir().unwrap();
        let settings = VideoSettings::default();
        let gallery = gallery(source.path(), cache.path(), settings.clone());
        let mut metadata = ObjectMetadata {
            size: 100,
            last_modified: Some(SystemTime::UNIX_EPOCH - Duration::from_secs(1)),
            content_type: None,
            etag: None,
        };
        let before = gallery.video_identity("clip.mov", &metadata, &settings);
        metadata.last_modified = Some(SystemTime::UNIX_EPOCH + Duration::from_secs(1));
        assert_ne!(
            before,
            gallery.video_identity("clip.mov", &metadata, &settings)
        );
    }

    #[tokio::test]
    async fn video_uses_existing_processing_pool_and_cancels_while_waiting() {
        let source = tempfile::tempdir().unwrap();
        let cache = tempfile::tempdir().unwrap();
        let mut gallery = gallery(source.path(), cache.path(), VideoSettings::default());
        gallery.image_processing_semaphore = Arc::new(tokio::sync::Semaphore::new(1));
        std::fs::write(source.path().join("clip.mov"), b"original").unwrap();
        std::fs::File::open(source.path().join("clip.mov"))
            .unwrap()
            .set_modified(SystemTime::UNIX_EPOCH + Duration::from_secs(1))
            .unwrap();
        let permit = gallery.image_processing_semaphore.acquire().await.unwrap();
        let preparation = gallery.prepare_gallery_video("clip.mov");
        tokio::pin!(preparation);
        assert!(
            tokio::time::timeout(Duration::from_millis(50), &mut preparation)
                .await
                .is_err()
        );
        // No record is marked processing, staged, or encoded without a pool slot.
        assert!(gallery.video_record("clip.mov").await.is_none());
        gallery.shutdown_token.cancel();
        tokio::time::timeout(Duration::from_secs(1), preparation)
            .await
            .unwrap()
            .unwrap();
        drop(permit);
        assert_eq!(gallery.image_processing_semaphore.available_permits(), 1);
    }

    #[tokio::test]
    async fn restart_reuses_artifacts_and_source_changes_discard_old_generation() {
        let source = tempfile::tempdir().unwrap();
        let cache = tempfile::tempdir().unwrap();
        let settings = VideoSettings::default();
        let first = gallery(source.path(), cache.path(), settings.clone());
        first
            .source_storage
            .write("clip.mov", bytes::Bytes::from_static(b"original"))
            .await
            .unwrap();
        let original = first.source_storage.metadata("clip.mov").await.unwrap();
        let identity = first.video_identity("clip.mov", &original, &settings);
        let record = VideoRecord {
            artifact_prefix: format!("video/{}/{identity}", first.video_namespace()),
            identity,
            state: VideoState::Ready,
            attempts: 1,
            retry_after: SystemTime::UNIX_EPOCH,
            metadata: None,
            poster_ready: true,
            error: None,
        };
        for name in ["poster.jpg", "playback.mp4"] {
            first
                .cache_storage
                .write(
                    &format!("{}/{name}", record.artifact_prefix),
                    bytes::Bytes::from_static(b"prepared"),
                )
                .await
                .unwrap();
        }
        first.save_video_record("clip.mov", &record).await.unwrap();
        drop(first);
        let restarted = gallery(source.path(), cache.path(), settings.clone());
        restarted.prepare_gallery_video("clip.mov").await.unwrap();
        let reused = restarted.video_record("clip.mov").await.unwrap();
        assert_eq!(reused.identity, record.identity);
        assert_eq!(reused.attempts, 1);
        assert_eq!(reused.state, VideoState::Ready);
        let mut changed_settings = settings.clone();
        changed_settings.crf += 1;
        assert_ne!(
            restarted.video_identity("clip.mov", &original, &changed_settings),
            record.identity
        );
        restarted
            .source_storage
            .write("clip.mov", bytes::Bytes::from_static(b"replacement source"))
            .await
            .unwrap();
        restarted.prepare_gallery_video("clip.mov").await.unwrap(); // new files settle before processing
        let replaced = restarted.video_record("clip.mov").await.unwrap();
        assert_ne!(replaced.identity, record.identity);
        assert_eq!(replaced.state, VideoState::Pending);
        for name in ["poster.jpg", "playback.mp4"] {
            assert!(
                !restarted
                    .cache_storage
                    .exists(&format!("{}/{name}", record.artifact_prefix))
                    .await
                    .unwrap()
            );
        }
    }
}
