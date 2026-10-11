use crate::{
    CancellationToken, PIPELINE_VERSION, PreparationPlan, Result, StreamAction, VideoError,
    VideoMetadata, VideoSettings, VideoTools,
};
use serde::Serialize;
use std::{
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};
use tempfile::TempDir;
use tokio::{
    io::{AsyncRead, AsyncReadExt},
    process::Command,
    sync::Semaphore,
};

const POSTER_BUDGET: u64 = 16 * 1024 * 1024;
// FFmpeg's -fs stops successfully at the limit. Stop above the accepted budget
// so even a very short, truncated output must fail the final size validation.
const OUTPUT_SIZE_GUARD: u64 = 1024 * 1024;
// Accept the gallery's supported containers, not playlist/concat demuxers that
// can open additional local files referenced by an uploaded document.
const INPUT_FORMATS: &str = "mov,matroska,webm";

#[derive(Debug, Serialize)]
pub struct PrerequisiteReport {
    pub ffmpeg_version: String,
    pub ffprobe_version: String,
    pub supported_versions: bool,
    pub h264_encoder: bool,
    pub aac_encoder: bool,
    pub hdr_tone_mapping: bool,
    pub metadata_filter: bool,
    pub temp_directory: PathBuf,
    pub available_space_bytes: u64,
    pub required_space_bytes: u64,
}

impl PrerequisiteReport {
    pub fn ready(&self) -> bool {
        self.supported_versions
            && self.h264_encoder
            && self.aac_encoder
            && self.hdr_tone_mapping
            && self.metadata_filter
            && self.available_space_bytes >= self.required_space_bytes
    }
}

/// Artifacts have passed validation but have not been published into storage.
/// The caller must retain this value until streaming uploads are complete.
#[derive(Debug)]
pub struct PreparedVideo {
    directory: TempDir,
    pub manifest: PreparationManifest,
}

#[derive(Debug, Serialize)]
pub struct PreparationManifest {
    pub pipeline_version: u32,
    pub settings: VideoSettings,
    pub source: VideoMetadata,
    pub playback: VideoMetadata,
    pub plan: PreparationPlan,
    pub poster_time_seconds: f64,
}

impl PreparedVideo {
    pub fn playback_path(&self) -> PathBuf {
        self.directory.path().join("playback.mp4")
    }
    pub fn poster_path(&self) -> PathBuf {
        self.directory.path().join("poster.jpg")
    }
}

pub struct VideoProcessor {
    tools: VideoTools,
    slots: Semaphore,
}

impl VideoProcessor {
    pub fn new(tools: VideoTools) -> Result<Self> {
        if tools.concurrency == 0
            || tools.concurrency > 16
            || tools.threads == 0
            || tools.threads > 16
            || tools.timeout_seconds == 0
            || tools.min_free_space_bytes > u64::MAX - 2 * 1024 * 1024 * 1024
        {
            return Err(VideoError::Configuration("invalid worker limits".into()));
        }
        Ok(Self {
            slots: Semaphore::new(tools.concurrency),
            tools,
        })
    }

    fn scratch(&self) -> Result<TempDir> {
        // A configured scratch directory must already exist. This catches typos
        // and makes ownership/setup an explicit deployment step.
        Ok(tempfile::Builder::new()
            .prefix("tenrankai-video-")
            .tempdir_in(&self.tools.temp_directory)?)
    }

    fn check_space(&self, additional_bytes: u64) -> Result<u64> {
        let available = fs2::available_space(&self.tools.temp_directory)?;
        let required = additional_bytes
            .checked_add(self.tools.min_free_space_bytes)
            .ok_or_else(|| VideoError::Configuration("disk budget overflow".into()))?;
        if available < required {
            return Err(VideoError::Limit(format!(
                "scratch disk has {available} bytes available; {required} required"
            )));
        }
        Ok(available)
    }

    pub async fn check_prerequisites(
        &self,
        token: &CancellationToken,
    ) -> Result<PrerequisiteReport> {
        let mut command = self.ffmpeg();
        command.arg("-version");
        let ffmpeg_version = first_line(&self.run(&mut command, 20, token).await?);
        let mut command = Command::new(&self.tools.ffprobe);
        command.arg("-version");
        let ffprobe_version = first_line(&self.run(&mut command, 20, token).await?);
        let mut command = self.ffmpeg();
        command.arg("-encoders");
        let encoders =
            String::from_utf8_lossy(&self.run(&mut command, 20, token).await?).into_owned();
        let mut command = self.ffmpeg();
        command.arg("-filters");
        let filters =
            String::from_utf8_lossy(&self.run(&mut command, 20, token).await?).into_owned();
        let mut command = self.ffmpeg();
        command.arg("-bsfs");
        let bitstream_filters =
            String::from_utf8_lossy(&self.run(&mut command, 20, token).await?).into_owned();
        let has = |listing: &str, name: &str| {
            listing
                .lines()
                .any(|line| line.split_whitespace().nth(1) == Some(name))
        };
        // Creating a private directory verifies service-user write access.
        let _scratch = self.scratch()?;
        let settings = VideoSettings::default();
        Ok(PrerequisiteReport {
            supported_versions: supported_version(&ffmpeg_version)
                && supported_version(&ffprobe_version),
            ffmpeg_version,
            ffprobe_version,
            h264_encoder: has(&encoders, "libx264"),
            aac_encoder: has(&encoders, "aac"),
            hdr_tone_mapping: has(&filters, "zscale") && has(&filters, "tonemap"),
            metadata_filter: bitstream_filters
                .lines()
                .any(|line| line.trim() == "filter_units"),
            temp_directory: self.tools.temp_directory.clone(),
            available_space_bytes: fs2::available_space(&self.tools.temp_directory)?,
            required_space_bytes: settings.max_source_bytes
                + settings.max_output_bytes
                + OUTPUT_SIZE_GUARD
                + POSTER_BUDGET
                + self.tools.min_free_space_bytes,
        })
    }

    /// Probe local, seekable files. Remote objects should first be streamed into
    /// bounded scratch storage; piping MP4 fails when its index is at the end.
    pub async fn probe(&self, input: &Path, cancel: &CancellationToken) -> Result<VideoMetadata> {
        let input = tokio::fs::canonicalize(input).await?;
        let file = tokio::fs::metadata(&input).await?;
        if !file.is_file() || file.len() == 0 {
            return Err(VideoError::Unsupported(
                "input must be a nonempty regular file".into(),
            ));
        }
        let mut command = Command::new(&self.tools.ffprobe);
        command
            .args([
                "-v",
                "error",
                "-protocol_whitelist",
                "file,pipe",
                "-format_whitelist",
                INPUT_FORMATS,
                "-max_alloc",
                "268435456",
                "-max_pixels",
                "64000000",
                "-threads",
            ])
            .arg(self.tools.threads.to_string())
            .args(["-show_streams", "-show_format", "-of", "json"])
            .arg(&input);
        let output = self.run(&mut command, 30, cancel).await?;
        VideoMetadata::from_probe(&serde_json::from_slice(&output)?, file.len())
    }

    pub async fn prepare(
        &self,
        input: &Path,
        settings: &VideoSettings,
        poster_time: Option<f64>,
        cancel: &CancellationToken,
    ) -> Result<PreparedVideo> {
        settings.validate()?;
        let _permit = tokio::select! {
            biased;
            _ = cancel.cancelled() => return Err(VideoError::Cancelled),
            permit = self.slots.acquire() => permit.map_err(|_| VideoError::Cancelled)?,
        };
        let input = tokio::fs::canonicalize(input).await?;
        let original_file = tokio::fs::metadata(&input).await?;
        if original_file.len() > settings.max_source_bytes {
            return Err(VideoError::Limit("source file is too large".into()));
        }
        let output_stop_bytes = settings
            .max_output_bytes
            .checked_add(OUTPUT_SIZE_GUARD)
            .ok_or_else(|| VideoError::Configuration("disk budget overflow".into()))?;
        self.check_space(
            output_stop_bytes
                .checked_add(POSTER_BUDGET)
                .ok_or_else(|| VideoError::Configuration("disk budget overflow".into()))?,
        )?;
        let source = self.probe(&input, cancel).await?;
        let plan = settings.plan(&source)?;
        let requested_poster_time = poster_time.unwrap_or_else(|| {
            if source.duration_seconds < 0.25 {
                0.0
            } else {
                (source.duration_seconds * 0.1).min(3.0)
            }
        });
        if !requested_poster_time.is_finite()
            || requested_poster_time < 0.0
            || requested_poster_time >= source.duration_seconds
        {
            return Err(VideoError::Configuration(
                "poster time must be within the clip".into(),
            ));
        }
        let directory = self.scratch()?;
        let poster = directory.path().join("poster.jpg");
        // Generate the poster before encoding so callers can later publish this
        // stage independently. Both outputs use the same SDR/color policy.
        self.create_poster(&input, &source, requested_poster_time, &poster, cancel)
            .await?;
        let poster_time_seconds = if !poster.exists() && poster_time.is_none() {
            self.create_poster(&input, &source, 0.0, &poster, cancel)
                .await?;
            0.0
        } else {
            requested_poster_time
        };
        let poster_size = tokio::fs::metadata(&poster).await?.len();
        if poster_size == 0 || poster_size > POSTER_BUDGET {
            return Err(VideoError::InvalidOutput(
                "missing or oversized poster".into(),
            ));
        }
        let output = directory.path().join("playback.mp4");
        let mut command = self.ffmpeg_input(&input);
        command.arg("-map").arg(format!("0:{}", source.video.index));
        if let Some(audio) = &source.audio {
            command.arg("-map").arg(format!("0:{}", audio.index));
        }
        strip_metadata(&mut command);
        if plan.video == StreamAction::Copy {
            command.args(["-c:v", "copy", "-bsf:v", "filter_units=remove_types=6"]);
        } else {
            let (width, height) = plan.output_dimensions;
            let filter = format!(
                "select='isnan(prev_selected_t)+gte(t-prev_selected_t,1/{}-0.000001)',{}scale={width}:{height}:flags=lanczos,setsar=1,format=yuv420p,sidedata=mode=delete",
                settings.max_frame_rate,
                tone_map(&source)
            );
            command
                .args(["-c:v", "libx264", "-preset", "medium", "-crf"])
                .arg(settings.crf.to_string())
                .args(["-profile:v", "high", "-pix_fmt", "yuv420p", "-vf"])
                .arg(filter)
                .arg("-maxrate")
                .arg(settings.max_video_bitrate.to_string())
                .arg("-bufsize")
                .arg((settings.max_video_bitrate * 2).to_string())
                .args(["-fps_mode", "vfr", "-metadata:s:v:0", "rotate=0"])
                .arg("-threads")
                .arg(self.tools.threads.to_string());
            if width.max(height) <= 1920
                && width.min(height) <= 1080
                && settings.max_frame_rate <= 60
            {
                command.args(["-level:v", "4.2"]);
            }
            if source.video.is_hdr {
                command.args([
                    "-color_primaries",
                    "bt709",
                    "-color_trc",
                    "bt709",
                    "-colorspace",
                    "bt709",
                    "-color_range",
                    "tv",
                ]);
            }
        }
        match plan.audio {
            Some(StreamAction::Copy) => {
                command.args(["-c:a", "copy"]);
            }
            Some(StreamAction::Encode) => {
                command.args(["-c:a", "aac", "-b:a", "128k", "-ac", "2", "-ar", "48000"]);
            }
            None => {
                command.arg("-an");
            }
        }
        command
            .args(["-movflags", "+faststart", "-fs"])
            .arg(output_stop_bytes.to_string())
            .arg(&output);
        self.run(&mut command, self.tools.timeout_seconds, cancel)
            .await?;
        let playback = self.probe(&output, cancel).await?;
        validate_output(&source, &playback, &plan, settings)?;
        let current_file = tokio::fs::metadata(&input).await?;
        if current_file.len() != original_file.len()
            || current_file.modified()? != original_file.modified()?
        {
            return Err(VideoError::SourceChanged);
        }
        Ok(PreparedVideo {
            directory,
            manifest: PreparationManifest {
                pipeline_version: PIPELINE_VERSION,
                settings: settings.clone(),
                source,
                playback,
                plan,
                poster_time_seconds,
            },
        })
    }

    /// Exercise decoding, H.264/AAC encoding, metadata probing, and poster
    /// generation without publishing any gallery files.
    pub async fn smoke_test(&self, cancel: &CancellationToken) -> Result<()> {
        let scratch = self.scratch()?;
        let input = scratch.path().join("source.mp4");
        let mut command = self.ffmpeg();
        command
            .args([
                "-f",
                "lavfi",
                "-i",
                "testsrc2=size=64x48:rate=12",
                "-f",
                "lavfi",
                "-i",
                "sine=frequency=440:sample_rate=48000",
                "-t",
                "0.5",
                "-c:v",
                "mpeg4",
                "-c:a",
                "aac",
                "-threads",
                "1",
            ])
            .arg(&input);
        self.run(&mut command, 30, cancel).await?;
        self.prepare(&input, &VideoSettings::default(), None, cancel)
            .await?;
        Ok(())
    }

    async fn create_poster(
        &self,
        input: &Path,
        source: &VideoMetadata,
        time: f64,
        output: &Path,
        cancel: &CancellationToken,
    ) -> Result<()> {
        let (width, height) = source.video.display_dimensions;
        let scale = (1200.0 / width.max(height) as f64).min(1.0);
        let width = ((width as f64 * scale).floor() as u32 / 2 * 2).max(2);
        let height = ((height as f64 * scale).floor() as u32 / 2 * 2).max(2);
        let mut command = self.ffmpeg();
        command
            .args([
                "-protocol_whitelist",
                "file,pipe",
                "-format_whitelist",
                INPUT_FORMATS,
                "-max_alloc",
                "268435456",
                "-max_pixels",
                "64000000",
                "-threads",
            ])
            .arg(self.tools.threads.to_string())
            .arg("-ss")
            .arg(format!("{time:.6}"))
            .arg("-i")
            .arg(input)
            .arg("-map")
            .arg(format!("0:{}", source.video.index))
            .args(["-frames:v", "1", "-an", "-vf"])
            .arg(format!(
                "{}scale={width}:{height}:flags=lanczos,setsar=1",
                tone_map(source)
            ))
            .args([
                "-c:v", "mjpeg", "-q:v", "3", "-threads", "1", "-update", "1", "-fs",
            ])
            .arg(POSTER_BUDGET.to_string());
        strip_metadata(&mut command);
        command.arg(output);
        self.run(&mut command, 60, cancel).await?;
        Ok(())
    }

    fn ffmpeg(&self) -> Command {
        let mut command = Command::new(&self.tools.ffmpeg);
        command.args([
            "-nostdin",
            "-hide_banner",
            "-loglevel",
            "error",
            "-nostats",
            "-n",
            "-filter_threads",
            "1",
        ]);
        command
    }

    fn ffmpeg_input(&self, input: &Path) -> Command {
        let mut command = self.ffmpeg();
        command
            .args([
                "-protocol_whitelist",
                "file,pipe",
                "-format_whitelist",
                INPUT_FORMATS,
                "-max_alloc",
                "268435456",
                "-max_pixels",
                "64000000",
                "-threads",
            ])
            .arg(self.tools.threads.to_string())
            .arg("-i")
            .arg(input);
        command
    }

    async fn run(
        &self,
        command: &mut Command,
        timeout_seconds: u64,
        cancel: &CancellationToken,
    ) -> Result<Vec<u8>> {
        if cancel.is_cancelled() {
            return Err(VideoError::Cancelled);
        }
        let tool = command
            .as_std()
            .get_program()
            .to_string_lossy()
            .into_owned();
        let mut child = command
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .map_err(|error| VideoError::Tool {
                tool: tool.clone(),
                message: error.to_string(),
            })?;
        let stdout = child.stdout.take().unwrap();
        let stderr = child.stderr.take().unwrap();
        let result = tokio::select! {
            biased;
            _ = cancel.cancelled() => Err(VideoError::Cancelled),
            result = tokio::time::timeout(Duration::from_secs(timeout_seconds), async {
                let (stdout, stderr, status) = tokio::try_join!(
                    read_bounded(stdout, 8 * 1024 * 1024), read_bounded(stderr, 64 * 1024), child.wait()
                )?;
                if !status.success() {
                    return Err(VideoError::Tool { tool: tool.clone(), message: format!("{status}: {}", String::from_utf8_lossy(&stderr.0)) });
                }
                if stdout.1 {
                    return Err(VideoError::Tool { tool: tool.clone(), message: "tool output exceeds 8 MiB".into() });
                }
                Ok(stdout.0)
            }) => result.unwrap_or(Err(VideoError::Timeout)),
        };
        if result.is_err() {
            // Reap cancelled/timed-out processes before releasing the job slot.
            let _ = child.kill().await;
            let _ = child.wait().await;
        }
        result
    }
}

fn strip_metadata(command: &mut Command) {
    // Global and per-stream mappings are separate in FFmpeg. Disable both, and
    // exclude chapters/data/subtitle streams by mapping selected AV streams only.
    command.args([
        "-map_metadata",
        "-1",
        "-map_metadata:s",
        "-1",
        "-map_chapters",
        "-1",
    ]);
}

fn tone_map(source: &VideoMetadata) -> &'static str {
    if source.video.is_hdr {
        "zscale=t=linear:npl=100,format=gbrpf32le,zscale=p=bt709,tonemap=tonemap=hable:desat=0,zscale=t=bt709:m=bt709:r=tv,format=yuv420p,"
    } else {
        ""
    }
}

fn first_line(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes)
        .lines()
        .next()
        .unwrap_or_default()
        .to_string()
}

fn supported_version(line: &str) -> bool {
    let Some(version) = line.split_whitespace().nth(2) else {
        return false;
    };
    let mut parts = version.trim_start_matches(['n', 'v']).split('.');
    let major = parts
        .next()
        .and_then(|part| part.parse::<u32>().ok())
        .unwrap_or(0);
    let minor = parts
        .next()
        .and_then(|part| part.parse::<u32>().ok())
        .unwrap_or(0);
    major >= 6 || (major == 5 && minor >= 1)
}

async fn read_bounded(
    mut reader: impl AsyncRead + Unpin,
    limit: usize,
) -> std::io::Result<(Vec<u8>, bool)> {
    let mut captured = Vec::new();
    let mut truncated = false;
    let mut buffer = [0; 8192];
    loop {
        let count = reader.read(&mut buffer).await?;
        if count == 0 {
            break;
        }
        let keep = count.min(limit.saturating_sub(captured.len()));
        captured.extend_from_slice(&buffer[..keep]);
        truncated |= keep < count;
    }
    Ok((captured, truncated))
}

fn validate_output(
    source: &VideoMetadata,
    output: &VideoMetadata,
    plan: &PreparationPlan,
    settings: &VideoSettings,
) -> Result<()> {
    if output.video.codec != "h264"
        || output.video.pixel_format.as_deref() != Some("yuv420p")
        || output.video.display_dimensions != plan.output_dimensions
        || output.video.rotation_degrees != 0
        || output.video.sample_aspect_ratio != (1, 1)
        || output.video.is_hdr
        || output
            .video
            .frame_rate
            .is_some_and(|fps| fps > settings.max_frame_rate as f64 + 0.1)
        || source.audio.is_some() != output.audio.is_some()
        || output
            .audio
            .as_ref()
            .is_some_and(|audio| audio.codec != "aac")
        || output.capture_time.is_some()
        || output.location.is_some()
        || output.camera_make.is_some()
        || output.camera_model.is_some()
        || output.lens_model.is_some()
    {
        return Err(VideoError::InvalidOutput(
            "playback does not match delivery policy".into(),
        ));
    }
    // -fs is set above the accepted budget: any size-limited truncation fails
    // regardless of the duration tolerance needed for codec/container rounding.
    if (source.duration_seconds - output.duration_seconds).abs() > 0.5
        || output.file_size > settings.max_output_bytes
    {
        return Err(VideoError::InvalidOutput(
            "truncated or oversized playback".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn drains_oversized_process_output_without_unbounded_capture() {
        let (mut writer, reader) = tokio::io::duplex(1024);
        let producer = tokio::spawn(async move {
            use tokio::io::AsyncWriteExt;
            writer.write_all(&vec![b'x'; 128 * 1024]).await.unwrap();
        });
        let (bytes, truncated) = read_bounded(reader, 1024).await.unwrap();
        producer.await.unwrap();
        assert_eq!(bytes.len(), 1024);
        assert!(truncated);
    }

    #[test]
    fn checks_tool_versions_and_invalid_worker_limits() {
        assert!(supported_version("ffmpeg version 6.1.1-3ubuntu5 Copyright"));
        assert!(supported_version("ffprobe version n8.0 Copyright"));
        assert!(supported_version(
            "ffmpeg version 5.1.9-0+deb12u1 Copyright"
        ));
        assert!(!supported_version("ffmpeg version 5.0.3 Copyright"));
        assert!(!supported_version("ffmpeg version 4.4.2 Copyright"));
        assert!(!supported_version("unexpected output"));
        assert!(
            VideoProcessor::new(VideoTools {
                concurrency: 0,
                ..Default::default()
            })
            .is_err()
        );
        assert!(
            VideoProcessor::new(VideoTools {
                min_free_space_bytes: u64::MAX,
                ..Default::default()
            })
            .is_err()
        );
    }

    #[cfg(unix)]
    async fn sleeper(scratch: &TempDir) -> Command {
        let executable = scratch.path().join("sleep-tool");
        tokio::fs::write(
            &executable,
            b"#!/bin/sh\nprintf '%s' \"$$\" > \"$0.pid\"\nexec sleep 30\n",
        )
        .await
        .unwrap();
        // Read the freshly written fixture through the shell instead of
        // executing the temporary file, which can race filesystem write locks.
        let mut command = Command::new("/bin/sh");
        command.arg(executable);
        command
    }

    #[cfg(unix)]
    async fn assert_reaped(scratch: &TempDir) {
        let pid = tokio::fs::read_to_string(scratch.path().join("sleep-tool.pid"))
            .await
            .unwrap();
        // Minimal Debian images provide kill as a shell builtin.
        let status = Command::new("/bin/sh")
            .args(["-c", "kill -0 \"$1\"", "reap-check", pid.trim()])
            .stderr(Stdio::null())
            .status()
            .await
            .unwrap();
        assert!(!status.success(), "cancelled tool process is still alive");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn cancels_and_reaps_running_process() {
        let scratch = tempfile::tempdir().unwrap();
        let processor = VideoProcessor::new(VideoTools::default()).unwrap();
        let mut command = sleeper(&scratch).await;
        let token = CancellationToken::new();
        let signal_token = token.clone();
        let pid_path = scratch.path().join("sleep-tool.pid");
        let trigger = tokio::spawn(async move {
            for _ in 0..200 {
                if pid_path.exists() {
                    signal_token.cancel();
                    return;
                }
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
            panic!("test process did not start");
        });
        let result = processor.run(&mut command, 10, &token).await;
        assert!(matches!(result, Err(VideoError::Cancelled)), "{result:?}");
        trigger.await.unwrap();
        assert_reaped(&scratch).await;
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn times_out_and_reaps_running_process() {
        let scratch = tempfile::tempdir().unwrap();
        let processor = VideoProcessor::new(VideoTools::default()).unwrap();
        let mut command = sleeper(&scratch).await;
        let result = processor
            .run(&mut command, 1, &CancellationToken::new())
            .await;
        assert!(matches!(result, Err(VideoError::Timeout)), "{result:?}");
        assert_reaped(&scratch).await;
    }
}
