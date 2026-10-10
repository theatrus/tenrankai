//! Video preparation for short gallery clips. Originals are never modified.
//!
//! A processor bounds concurrent work across all callers sharing it. Preparation
//! returns validated, private temporary artifacts for the caller to publish into
//! storage. Dropping the result cleans them up. FFmpeg is a runtime dependency;
//! none of its libraries are linked into Tenrankai.

mod metadata;
mod process;
mod settings;

pub use metadata::{AudioStream, CaptureTime, Location, VideoMetadata, VideoStream};
pub use process::{PreparationManifest, PreparedVideo, PrerequisiteReport, VideoProcessor};
pub use settings::{PreparationPlan, ReencodePolicy, StreamAction, VideoSettings, VideoTools};
pub use tokio_util::sync::CancellationToken;

/// Increment when changes to preparation require regenerating derivatives.
pub const PIPELINE_VERSION: u32 = 1;

#[derive(Debug, thiserror::Error)]
pub enum VideoError {
    #[error("invalid video configuration: {0}")]
    Configuration(String),
    #[error("unsupported video: {0}")]
    Unsupported(String),
    #[error("video exceeds processing limits: {0}")]
    Limit(String),
    #[error("{tool} failed: {message}")]
    Tool { tool: String, message: String },
    #[error("video processing timed out")]
    Timeout,
    #[error("video processing cancelled")]
    Cancelled,
    #[error("source changed during video preparation")]
    SourceChanged,
    #[error("invalid prepared video: {0}")]
    InvalidOutput(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, VideoError>;

/// Extensions are discovery hints. A successful probe determines actual support.
pub fn is_video_path(path: &str) -> bool {
    path.rsplit_once('.').is_some_and(|(_, extension)| {
        ["mp4", "mov", "m4v", "webm", "mkv"]
            .iter()
            .any(|candidate| extension.eq_ignore_ascii_case(candidate))
    })
}
