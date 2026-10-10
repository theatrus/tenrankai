use clap::{Args, Subcommand};
use std::path::PathBuf;
use tenrankai_video::{
    CancellationToken, ReencodePolicy, VideoProcessor, VideoSettings, VideoTools,
};

#[derive(Debug, Args)]
pub struct ToolArgs {
    /// FFmpeg executable (also TENRANKAI_FFMPEG)
    #[arg(long)]
    ffmpeg: Option<PathBuf>,
    /// FFprobe executable (also TENRANKAI_FFPROBE)
    #[arg(long)]
    ffprobe: Option<PathBuf>,
    /// Existing writable scratch directory (also TENRANKAI_VIDEO_TEMP_DIR)
    #[arg(long)]
    temp_directory: Option<PathBuf>,
}

impl ToolArgs {
    fn processor(self) -> tenrankai_video::Result<VideoProcessor> {
        let mut tools = VideoTools::default();
        if let Some(path) = self.ffmpeg {
            tools.ffmpeg = path;
        }
        if let Some(path) = self.ffprobe {
            tools.ffprobe = path;
        }
        if let Some(path) = self.temp_directory {
            tools.temp_directory = path;
        }
        VideoProcessor::new(tools)
    }
}

#[derive(Debug, Subcommand)]
pub enum VideoCommands {
    /// Check encoders, HDR filters, scratch permissions and free space
    Doctor {
        #[command(flatten)]
        tools: ToolArgs,
        /// Also decode, encode, extract a poster, and validate a tiny test clip
        #[arg(long)]
        smoke_test: bool,
    },
    /// Print normalized metadata from a local video file
    Probe {
        input: PathBuf,
        #[command(flatten)]
        tools: ToolArgs,
    },
    /// Prepare a local clip without modifying the original or publishing it
    Prepare {
        input: PathBuf,
        /// New directory for playback.mp4, poster.jpg, and manifest.json
        #[arg(long)]
        output: PathBuf,
        #[arg(long, default_value = "auto", value_parser = ["never", "auto", "always"])]
        reencode: String,
        #[arg(long)]
        poster_time: Option<f64>,
        #[command(flatten)]
        tools: ToolArgs,
    },
}

pub async fn handle(command: VideoCommands) -> Result<(), Box<dyn std::error::Error>> {
    let cancel = CancellationToken::new();
    let signal_token = cancel.clone();
    let signal = tokio::spawn(async move {
        if tokio::signal::ctrl_c().await.is_ok() {
            signal_token.cancel();
        }
    });
    let result = execute(command, &cancel).await;
    signal.abort();
    result
}

async fn execute(
    command: VideoCommands,
    cancel: &CancellationToken,
) -> Result<(), Box<dyn std::error::Error>> {
    match command {
        VideoCommands::Doctor { tools, smoke_test } => {
            let processor = tools.processor()?;
            let report = processor.check_prerequisites(cancel).await?;
            println!("{}", serde_json::to_string_pretty(&report)?);
            if !report.ready() {
                return Err("video prerequisites are incomplete (see report)".into());
            }
            if smoke_test {
                processor.smoke_test(cancel).await?;
                println!("Video processing smoke test passed.");
            }
        }
        VideoCommands::Probe { input, tools } => {
            let metadata = tools.processor()?.probe(&input, cancel).await?;
            println!("{}", serde_json::to_string_pretty(&metadata)?);
        }
        VideoCommands::Prepare {
            input,
            output,
            reencode,
            poster_time,
            tools,
        } => {
            if output.exists() {
                return Err("output directory must not already exist".into());
            }
            let settings = VideoSettings {
                reencode: match reencode.as_str() {
                    "never" => ReencodePolicy::Never,
                    "always" => ReencodePolicy::Always,
                    _ => ReencodePolicy::Auto,
                },
                ..Default::default()
            };
            let prepared = tools
                .processor()?
                .prepare(&input, &settings, poster_time, cancel)
                .await?;
            let manifest = serde_json::to_vec_pretty(&prepared.manifest)?;
            // Use a private staging directory on the destination filesystem.
            // The CLI output is for operator inspection, not public serving.
            let parent = output
                .parent()
                .filter(|path| !path.as_os_str().is_empty())
                .unwrap_or_else(|| std::path::Path::new("."));
            let staging = tempfile::Builder::new()
                .prefix(".tenrankai-video-")
                .tempdir_in(parent)?;
            tokio::fs::copy(
                prepared.playback_path(),
                staging.path().join("playback.mp4"),
            )
            .await?;
            tokio::fs::copy(prepared.poster_path(), staging.path().join("poster.jpg")).await?;
            tokio::fs::write(staging.path().join("manifest.json"), &manifest).await?;
            // Reserve the destination exclusively to avoid overwriting user data.
            // Source metadata in the manifest must stay private to the operator.
            let destination = std::fs::DirBuilder::new();
            #[cfg(unix)]
            let destination = {
                use std::os::unix::fs::DirBuilderExt;
                let mut destination = destination;
                destination.mode(0o700);
                destination
            };
            destination.create(&output)?;
            for name in ["playback.mp4", "poster.jpg", "manifest.json"] {
                tokio::fs::rename(staging.path().join(name), output.join(name)).await?;
            }
            println!("{}", String::from_utf8(manifest)?);
        }
    }
    Ok(())
}
