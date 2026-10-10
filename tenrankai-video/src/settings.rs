use crate::{Result, VideoError, VideoMetadata};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReencodePolicy {
    /// Lossless repackaging only; fail if any selected stream requires encoding.
    Never,
    #[default]
    Auto,
    Always,
}

/// Delivery settings are also suitable for serialization into a cache manifest.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct VideoSettings {
    pub reencode: ReencodePolicy,
    pub max_duration_seconds: f64,
    pub max_source_bytes: u64,
    pub max_output_bytes: u64,
    pub max_long_edge: u32,
    pub max_short_edge: u32,
    pub max_frame_rate: u32,
    pub max_video_bitrate: u64,
    pub crf: u8,
}

impl Default for VideoSettings {
    fn default() -> Self {
        Self {
            reencode: ReencodePolicy::Auto,
            max_duration_seconds: 300.0,
            max_source_bytes: 512 * 1024 * 1024,
            max_output_bytes: 256 * 1024 * 1024,
            max_long_edge: 1920,
            max_short_edge: 1080,
            max_frame_rate: 60,
            max_video_bitrate: 6_000_000,
            crf: 23,
        }
    }
}

impl VideoSettings {
    pub fn validate(&self) -> Result<()> {
        if !self.max_duration_seconds.is_finite()
            || self.max_duration_seconds <= 0.0
            || self.max_source_bytes == 0
            || self.max_output_bytes == 0
            || self.max_short_edge < 2
            || self.max_long_edge < self.max_short_edge
            || self.max_long_edge > 8192
            || self.max_frame_rate == 0
            || self.max_frame_rate > 120
            || self.max_video_bitrate == 0
            || self.max_video_bitrate > u64::MAX / 2
            || self.crf > 51
        {
            return Err(VideoError::Configuration("invalid delivery limits".into()));
        }
        Ok(())
    }

    /// Fit in a landscape or portrait delivery box, with square pixels and even
    /// dimensions for H.264 4:2:0. Never upscale.
    pub fn output_dimensions(&self, source: &VideoMetadata) -> (u32, u32) {
        let (width, height) = source.video.display_dimensions;
        let scale = (self.max_long_edge as f64 / width.max(height) as f64)
            .min(self.max_short_edge as f64 / width.min(height) as f64)
            .min(1.0);
        let even = |value: u32| ((value as f64 * scale).floor() as u32 / 2 * 2).max(2);
        (even(width), even(height))
    }

    pub fn plan(&self, source: &VideoMetadata) -> Result<PreparationPlan> {
        self.validate()?;
        if source.duration_seconds > self.max_duration_seconds {
            return Err(VideoError::Limit(format!(
                "duration {:.2}s exceeds {:.2}s",
                source.duration_seconds, self.max_duration_seconds
            )));
        }
        if source.file_size > self.max_source_bytes {
            return Err(VideoError::Limit("source file is too large".into()));
        }
        if source.video.is_hdr
            && !matches!(
                source.video.color_transfer.as_deref(),
                Some("smpte2084" | "arib-std-b67")
            )
        {
            return Err(VideoError::Unsupported(
                "HDR without a supported PQ/HLG base layer".into(),
            ));
        }
        let mut reasons = Vec::new();
        let video = &source.video;
        if video.codec != "h264" || video.pixel_format.as_deref() != Some("yuv420p") {
            reasons.push("video codec or pixel format".to_string());
        }
        if !matches!(
            video.profile.as_deref(),
            Some("Constrained Baseline" | "Baseline" | "Main" | "High")
        ) || video.level.is_none_or(|level| level > 42)
        {
            reasons.push("video profile or level".to_string());
        }
        if video.is_hdr {
            reasons.push("HDR to SDR conversion".to_string());
        }
        if video.rotation_degrees != 0 || video.sample_aspect_ratio != (1, 1) {
            reasons.push("display orientation or pixel aspect ratio".to_string());
        }
        let output_dimensions = self.output_dimensions(source);
        if output_dimensions != video.encoded_dimensions {
            reasons.push("delivery dimensions".to_string());
        }
        if video
            .frame_rate
            .is_none_or(|fps| fps > self.max_frame_rate as f64 + 0.01)
        {
            reasons.push("frame rate".to_string());
        }
        // Container bitrate is a conservative fallback when stream bitrate is
        // absent. Unknown bitrate requires preparation rather than a blind copy.
        let bitrate = video.bitrate.or(source.bitrate);
        if bitrate.is_none_or(|rate| rate > self.max_video_bitrate) {
            reasons.push("delivery bitrate".to_string());
        }
        if source.file_size > self.max_output_bytes {
            reasons.push("delivery file size".to_string());
        }
        if self.reencode == ReencodePolicy::Always {
            reasons.push("always re-encode policy".to_string());
        }
        let video_action = if reasons.is_empty() {
            StreamAction::Copy
        } else {
            StreamAction::Encode
        };
        let audio_action = source.audio.as_ref().map(|audio| {
            if self.reencode != ReencodePolicy::Always
                && audio.codec == "aac"
                && audio.profile.as_deref() == Some("LC")
                && audio
                    .channels
                    .is_some_and(|channels| (1..=2).contains(&channels))
                && audio
                    .sample_rate
                    .is_some_and(|rate| (8000..=48_000).contains(&rate))
            {
                StreamAction::Copy
            } else {
                StreamAction::Encode
            }
        });
        if self.reencode == ReencodePolicy::Never
            && (video_action == StreamAction::Encode || audio_action == Some(StreamAction::Encode))
        {
            return Err(VideoError::Unsupported(
                "delivery settings require encoding, but reencode = never".into(),
            ));
        }
        Ok(PreparationPlan {
            video: video_action,
            audio: audio_action,
            output_dimensions,
            reasons,
        })
    }
}

/// Runtime paths and resource limits. Share one processor across galleries.
#[derive(Debug, Clone)]
pub struct VideoTools {
    pub ffmpeg: PathBuf,
    pub ffprobe: PathBuf,
    pub temp_directory: PathBuf,
    pub concurrency: usize,
    pub threads: u32,
    pub timeout_seconds: u64,
    pub min_free_space_bytes: u64,
}

impl Default for VideoTools {
    fn default() -> Self {
        Self {
            ffmpeg: std::env::var_os("TENRANKAI_FFMPEG")
                .map(PathBuf::from)
                .unwrap_or_else(|| "ffmpeg".into()),
            ffprobe: std::env::var_os("TENRANKAI_FFPROBE")
                .map(PathBuf::from)
                .unwrap_or_else(|| "ffprobe".into()),
            temp_directory: std::env::var_os("TENRANKAI_VIDEO_TEMP_DIR")
                .map(PathBuf::from)
                .unwrap_or_else(std::env::temp_dir),
            concurrency: 1,
            threads: 2,
            timeout_seconds: 1800,
            min_free_space_bytes: 1024 * 1024 * 1024,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StreamAction {
    Copy,
    Encode,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreparationPlan {
    pub video: StreamAction,
    pub audio: Option<StreamAction>,
    pub output_dimensions: (u32, u32),
    pub reasons: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metadata::tests::compatible_probe;

    fn source() -> VideoMetadata {
        VideoMetadata::from_probe(&compatible_probe(), 1000).unwrap()
    }

    #[test]
    fn copies_compatible_streams_and_encodes_only_unsupported_audio() {
        let mut source = source();
        let settings = VideoSettings::default();
        let plan = settings.plan(&source).unwrap();
        assert_eq!(plan.video, StreamAction::Copy);
        assert_eq!(plan.audio, Some(StreamAction::Copy));
        source.audio.as_mut().unwrap().codec = "pcm_s16le".into();
        let plan = settings.plan(&source).unwrap();
        assert_eq!(plan.video, StreamAction::Copy);
        assert_eq!(plan.audio, Some(StreamAction::Encode));
        source.audio = None;
        assert!(settings.plan(&source).unwrap().audio.is_none());
    }

    #[test]
    fn honors_never_and_always_policies() {
        let mut source = source();
        let mut settings = VideoSettings {
            reencode: ReencodePolicy::Never,
            ..Default::default()
        };
        assert!(settings.plan(&source).is_ok());
        source.video.codec = "hevc".into();
        assert!(matches!(
            settings.plan(&source),
            Err(VideoError::Unsupported(_))
        ));
        settings.reencode = ReencodePolicy::Always;
        let plan = settings.plan(&source).unwrap();
        assert_eq!(plan.video, StreamAction::Encode);
        assert_eq!(plan.audio, Some(StreamAction::Encode));
    }

    #[test]
    fn fits_portrait_square_and_anamorphic_sources() {
        let mut source = source();
        let settings = VideoSettings::default();
        source.video.encoded_dimensions = (3840, 2160);
        source.video.display_dimensions = (2160, 3840);
        source.video.rotation_degrees = 90;
        assert_eq!(
            settings.plan(&source).unwrap().output_dimensions,
            (1080, 1920)
        );
        source.video.display_dimensions = (2000, 2000);
        assert_eq!(settings.output_dimensions(&source), (1080, 1080));
        source.video.display_dimensions = (641, 479);
        assert_eq!(settings.output_dimensions(&source), (640, 478));
    }

    #[test]
    fn enforces_duration_size_and_hdr_limits() {
        let mut source = source();
        let settings = VideoSettings::default();
        source.duration_seconds = 300.01;
        assert!(matches!(settings.plan(&source), Err(VideoError::Limit(_))));
        source.duration_seconds = 300.0;
        source.file_size = settings.max_source_bytes + 1;
        assert!(matches!(settings.plan(&source), Err(VideoError::Limit(_))));
        source.file_size = 1000;
        source.video.is_hdr = true;
        assert!(matches!(
            settings.plan(&source),
            Err(VideoError::Unsupported(_))
        ));
        source.video.color_transfer = Some("smpte2084".into());
        assert_eq!(settings.plan(&source).unwrap().video, StreamAction::Encode);
        let invalid = VideoSettings {
            max_frame_rate: 0,
            ..settings
        };
        assert!(invalid.validate().is_err());
    }
}
