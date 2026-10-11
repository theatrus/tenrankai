//! Real-tool tests. The dedicated video CI job requires FFmpeg 5.1+ with libx264.
//! Other platforms can still run the parsing/policy tests without that package.
use std::path::{Path, PathBuf};
use tempfile::TempDir;
use tenrankai_video::{
    CancellationToken, ReencodePolicy, StreamAction, VideoError, VideoProcessor, VideoSettings,
    VideoTools,
};
use tokio::process::Command;

async fn processor(scratch: &TempDir) -> Option<VideoProcessor> {
    let processor = VideoProcessor::new(VideoTools {
        temp_directory: scratch.path().into(),
        min_free_space_bytes: 0,
        ..Default::default()
    })
    .unwrap();
    let report = processor
        .check_prerequisites(&CancellationToken::new())
        .await;
    if report.as_ref().is_ok_and(|report| report.ready()) {
        return Some(processor);
    }
    assert!(
        std::env::var_os("TENRANKAI_VIDEO_TESTS_REQUIRED").is_none(),
        "required video tools unavailable: {report:?}"
    );
    eprintln!("Skipping real preparation test; FFmpeg 5.1+ with libx264 required: {report:?}");
    None
}

async fn ffmpeg(args: &[&str], output: &Path) {
    let result = Command::new(VideoTools::default().ffmpeg)
        .args([
            "-nostdin",
            "-hide_banner",
            "-loglevel",
            "error",
            "-n",
            "-filter_threads",
            "1",
        ])
        .args(args)
        .arg(output)
        .output()
        .await
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
}

async fn fixture(
    scratch: &TempDir,
    name: &str,
    video_codec: &str,
    audio_codec: Option<&str>,
    extra: &[&str],
) -> PathBuf {
    let path = scratch.path().join(name);
    let mut args = vec!["-f", "lavfi", "-i", "testsrc2=size=160x96:rate=24"];
    if audio_codec.is_some() {
        args.extend(["-f", "lavfi", "-i", "sine=frequency=440:sample_rate=48000"]);
    }
    args.extend([
        "-t",
        "1",
        "-c:v",
        video_codec,
        "-pix_fmt",
        "yuv420p",
        "-threads",
        "1",
    ]);
    if let Some(audio) = audio_codec {
        args.extend(["-c:a", audio]);
    }
    args.extend(extra);
    ffmpeg(&args, &path).await;
    path
}

async fn raw_probe(path: &Path) -> serde_json::Value {
    let result = Command::new(VideoTools::default().ffprobe)
        .args([
            "-v",
            "error",
            "-show_streams",
            "-show_format",
            "-of",
            "json",
        ])
        .arg(path)
        .output()
        .await
        .unwrap();
    assert!(result.status.success());
    serde_json::from_slice(&result.stdout).unwrap()
}

async fn decoded_frame_hashes(path: &Path) -> Vec<String> {
    let result = Command::new(VideoTools::default().ffmpeg)
        .args(["-v", "error", "-i"])
        .arg(path)
        .args(["-map", "0:v:0", "-an", "-f", "framemd5", "-"])
        .output()
        .await
        .unwrap();
    assert!(result.status.success());
    String::from_utf8(result.stdout)
        .unwrap()
        .lines()
        .filter(|line| !line.starts_with('#') && !line.is_empty())
        .map(|line| line.rsplit(',').next().unwrap().trim().to_string())
        .collect()
}

fn box_position(bytes: &[u8], name: &[u8; 4]) -> usize {
    let mut position = 0;
    while position + 8 <= bytes.len() {
        if &bytes[position + 4..position + 8] == name {
            return position;
        }
        let size = u32::from_be_bytes(bytes[position..position + 4].try_into().unwrap()) as usize;
        assert!(size >= 8, "unexpected MP4 box size");
        position += size;
    }
    panic!("missing MP4 box {name:?}")
}

#[tokio::test]
async fn probes_actual_files_without_an_h264_encoder() {
    // Native MPEG-4 generation needs no optional H.264 encoder.
    if Command::new(VideoTools::default().ffmpeg)
        .arg("-version")
        .output()
        .await
        .is_err()
    {
        assert!(std::env::var_os("TENRANKAI_VIDEO_TESTS_REQUIRED").is_none());
        return;
    }
    let scratch = tempfile::tempdir().unwrap();
    let input = fixture(&scratch, "probe.mp4", "mpeg4", None, &[]).await;
    let processor = VideoProcessor::new(VideoTools::default()).unwrap();
    let metadata = processor
        .probe(&input, &CancellationToken::new())
        .await
        .unwrap();
    assert_eq!(metadata.video.codec, "mpeg4");
    assert_eq!(metadata.video.display_dimensions, (160, 96));
    assert!(metadata.audio.is_none());
    assert!((metadata.duration_seconds - 1.0).abs() < 0.01);
}

#[tokio::test]
async fn prepares_real_video_and_cleans_up_without_modifying_original() {
    let scratch = tempfile::tempdir().unwrap();
    let Some(processor) = processor(&scratch).await else {
        return;
    };
    let input = fixture(&scratch, "source.mov", "mpeg4", Some("pcm_s16le"), &[]).await;
    let original = tokio::fs::read(&input).await.unwrap();
    let prepared = processor
        .prepare(
            &input,
            &VideoSettings::default(),
            None,
            &CancellationToken::new(),
        )
        .await
        .unwrap();
    assert_eq!(prepared.manifest.plan.video, StreamAction::Encode);
    assert_eq!(prepared.manifest.plan.audio, Some(StreamAction::Encode));
    assert_eq!(prepared.manifest.playback.video.codec, "h264");
    assert!(prepared.poster_path().metadata().unwrap().len() > 0);
    assert_eq!(original, tokio::fs::read(&input).await.unwrap());
    let playback = prepared.playback_path();
    let bytes = tokio::fs::read(&playback).await.unwrap();
    assert!(box_position(&bytes, b"moov") < box_position(&bytes, b"mdat"));
    drop(prepared);
    assert!(!playback.exists());
}

#[tokio::test]
async fn remuxes_compatible_video_without_quality_loss_and_removes_private_tags() {
    let scratch = tempfile::tempdir().unwrap();
    let Some(processor) = processor(&scratch).await else {
        return;
    };
    let input = fixture(
        &scratch,
        "tagged.mp4",
        "libx264",
        Some("aac"),
        &[
            "-movflags",
            "use_metadata_tags",
            "-metadata",
            "title=PRIVATE TITLE",
            "-metadata",
            "creation_time=2026-09-19T19:30:00Z",
            "-metadata",
            "location=+37.7749-122.4194/",
            "-metadata",
            "com.apple.quicktime.model=PRIVATE CAMERA",
            "-metadata:s:v:0",
            "handler_name=PRIVATE VIDEO",
            "-metadata:s:a:0",
            "handler_name=PRIVATE AUDIO",
        ],
    )
    .await;
    let source = processor
        .probe(&input, &CancellationToken::new())
        .await
        .unwrap();
    assert!(source.location.is_some());
    assert!(source.capture_time.is_some());
    let settings = VideoSettings {
        reencode: ReencodePolicy::Never,
        ..Default::default()
    };
    let prepared = processor
        .prepare(&input, &settings, None, &CancellationToken::new())
        .await
        .unwrap();
    assert_eq!(prepared.manifest.plan.video, StreamAction::Copy);
    assert_eq!(prepared.manifest.plan.audio, Some(StreamAction::Copy));
    assert_eq!(
        decoded_frame_hashes(&input).await,
        decoded_frame_hashes(&prepared.playback_path()).await
    );
    let public_metadata = raw_probe(&prepared.playback_path()).await.to_string();
    assert!(!public_metadata.contains("PRIVATE"));
    assert!(!public_metadata.contains("37.7749"));
    assert!(!public_metadata.contains("2026-09-19"));
}

#[tokio::test]
async fn converts_audio_without_reencoding_compatible_video() {
    let scratch = tempfile::tempdir().unwrap();
    let Some(processor) = processor(&scratch).await else {
        return;
    };
    let input = fixture(&scratch, "pcm.mov", "libx264", Some("pcm_s16le"), &[]).await;
    let prepared = processor
        .prepare(
            &input,
            &VideoSettings::default(),
            None,
            &CancellationToken::new(),
        )
        .await
        .unwrap();
    assert_eq!(prepared.manifest.plan.video, StreamAction::Copy);
    assert_eq!(prepared.manifest.plan.audio, Some(StreamAction::Encode));
    assert_eq!(
        decoded_frame_hashes(&input).await,
        decoded_frame_hashes(&prepared.playback_path()).await
    );
}

#[tokio::test]
async fn applies_portrait_rotation_to_playback_and_poster() {
    let scratch = tempfile::tempdir().unwrap();
    let Some(processor) = processor(&scratch).await else {
        return;
    };
    let base = fixture(&scratch, "base.mp4", "libx264", None, &[]).await;
    let input = scratch.path().join("portrait.mov");
    let report = processor
        .check_prerequisites(&CancellationToken::new())
        .await
        .unwrap();
    if report
        .ffmpeg_version
        .split_whitespace()
        .nth(2)
        .unwrap()
        .starts_with('5')
    {
        ffmpeg(
            &[
                "-i",
                base.to_str().unwrap(),
                "-c",
                "copy",
                "-metadata:s:v:0",
                "rotate=90",
            ],
            &input,
        )
        .await;
    } else {
        ffmpeg(
            &[
                "-display_rotation",
                "90",
                "-i",
                base.to_str().unwrap(),
                "-c",
                "copy",
            ],
            &input,
        )
        .await;
    }
    let prepared = processor
        .prepare(
            &input,
            &VideoSettings::default(),
            None,
            &CancellationToken::new(),
        )
        .await
        .unwrap();
    assert_eq!(prepared.manifest.source.video.display_dimensions, (96, 160));
    assert_eq!(
        prepared.manifest.playback.video.encoded_dimensions,
        (96, 160)
    );
    assert_eq!(prepared.manifest.playback.video.rotation_degrees, 0);
    let poster = raw_probe(&prepared.poster_path()).await;
    assert_eq!(poster["streams"][0]["width"], 96);
    assert_eq!(poster["streams"][0]["height"], 160);
}

#[tokio::test]
async fn tone_maps_hdr_and_handles_silent_and_tiny_clips() {
    let scratch = tempfile::tempdir().unwrap();
    let Some(processor) = processor(&scratch).await else {
        return;
    };
    let input = fixture(
        &scratch,
        "hdr.mkv",
        "libx264",
        None,
        &[
            "-pix_fmt",
            "yuv420p10le",
            "-vf",
            "setparams=color_primaries=bt2020:color_trc=arib-std-b67:colorspace=bt2020nc",
            "-x264-params",
            "colorprim=bt2020:transfer=arib-std-b67:colormatrix=bt2020nc",
            "-color_primaries",
            "bt2020",
            "-color_trc",
            "arib-std-b67",
            "-colorspace",
            "bt2020nc",
        ],
    )
    .await;
    let prepared = processor
        .prepare(
            &input,
            &VideoSettings::default(),
            None,
            &CancellationToken::new(),
        )
        .await
        .unwrap();
    assert!(
        prepared.manifest.source.video.is_hdr,
        "{}",
        raw_probe(&input).await
    );
    assert!(!prepared.manifest.playback.video.is_hdr);
    assert!(prepared.manifest.playback.audio.is_none());
    assert_eq!(
        prepared.manifest.playback.video.color_transfer.as_deref(),
        Some("bt709")
    );
    let tiny = scratch.path().join("tiny.mp4");
    ffmpeg(
        &[
            "-f",
            "lavfi",
            "-i",
            "color=size=32x32:rate=24",
            "-frames:v",
            "1",
            "-c:v",
            "libx264",
            "-threads",
            "1",
        ],
        &tiny,
    )
    .await;
    let prepared = processor
        .prepare(
            &tiny,
            &VideoSettings::default(),
            None,
            &CancellationToken::new(),
        )
        .await
        .unwrap();
    assert_eq!(prepared.manifest.poster_time_seconds, 0.0);
    assert!(prepared.poster_path().exists());
    // Duration rounding tolerance must not let an oversized tiny clip through.
    let settings = VideoSettings {
        max_output_bytes: 1000,
        ..Default::default()
    };
    assert!(matches!(
        processor
            .prepare(&tiny, &settings, None, &CancellationToken::new())
            .await,
        Err(VideoError::InvalidOutput(_))
    ));
}

#[tokio::test]
async fn preserves_variable_frame_timing_when_encoding() {
    let scratch = tempfile::tempdir().unwrap();
    let Some(processor) = processor(&scratch).await else {
        return;
    };
    let input = scratch.path().join("vfr.mp4");
    ffmpeg(
        &[
            "-f",
            "lavfi",
            "-i",
            "testsrc2=size=64x48:rate=30:duration=1",
            "-vf",
            "select='not(mod(n,3))+eq(n,1)'",
            "-fps_mode",
            "vfr",
            "-c:v",
            "libx264",
            "-threads",
            "1",
        ],
        &input,
    )
    .await;
    let settings = VideoSettings {
        reencode: ReencodePolicy::Always,
        ..Default::default()
    };
    let prepared = processor
        .prepare(&input, &settings, None, &CancellationToken::new())
        .await
        .unwrap();
    async fn times(path: &Path) -> Vec<f64> {
        let output = Command::new(VideoTools::default().ffprobe)
            .args([
                "-v",
                "error",
                "-select_streams",
                "v:0",
                "-show_entries",
                "frame=best_effort_timestamp_time",
                "-of",
                "json",
            ])
            .arg(path)
            .output()
            .await
            .unwrap();
        let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        value["frames"]
            .as_array()
            .unwrap()
            .iter()
            .map(|frame| {
                frame["best_effort_timestamp_time"]
                    .as_str()
                    .unwrap()
                    .parse()
                    .unwrap()
            })
            .collect()
    }
    let source = times(&input).await;
    assert!((source[1] - source[0] - (source[2] - source[1])).abs() > 0.01);
    let playback = times(&prepared.playback_path()).await;
    assert_eq!(source.len(), playback.len());
    for (source, playback) in source.iter().zip(playback) {
        assert!((source - playback).abs() < 0.001);
    }
}

#[tokio::test]
async fn rejects_limits_invalid_poster_and_cancellation_without_artifacts() {
    let scratch = tempfile::tempdir().unwrap();
    let Some(processor) = processor(&scratch).await else {
        return;
    };
    let input = fixture(&scratch, "source.mp4", "mpeg4", None, &[]).await;
    let cancel = CancellationToken::new();
    let settings = VideoSettings {
        max_source_bytes: 1,
        ..Default::default()
    };
    assert!(matches!(
        processor.prepare(&input, &settings, None, &cancel).await,
        Err(VideoError::Limit(_))
    ));
    let settings = VideoSettings {
        max_duration_seconds: 0.1,
        ..Default::default()
    };
    assert!(matches!(
        processor.prepare(&input, &settings, None, &cancel).await,
        Err(VideoError::Limit(_))
    ));
    assert!(matches!(
        processor
            .prepare(&input, &VideoSettings::default(), Some(2.0), &cancel)
            .await,
        Err(VideoError::Configuration(_))
    ));
    let settings = VideoSettings {
        max_output_bytes: 1000,
        ..Default::default()
    };
    assert!(
        processor
            .prepare(&input, &settings, None, &cancel)
            .await
            .is_err()
    );
    cancel.cancel();
    assert!(matches!(
        processor
            .prepare(&input, &VideoSettings::default(), None, &cancel)
            .await,
        Err(VideoError::Cancelled)
    ));
    let files: Vec<_> = std::fs::read_dir(scratch.path()).unwrap().collect();
    assert_eq!(
        files.len(),
        1,
        "failed preparation leaked scratch artifacts"
    );
}

#[tokio::test]
async fn rejects_playlists_disguised_as_video_files() {
    let scratch = tempfile::tempdir().unwrap();
    let Some(processor) = processor(&scratch).await else {
        return;
    };
    fixture(&scratch, "source.mp4", "mpeg4", None, &[]).await;
    let input = scratch.path().join("playlist.mov");
    std::fs::write(&input, "ffconcat version 1.0\nfile source.mp4\n").unwrap();
    assert!(matches!(
        processor.probe(&input, &CancellationToken::new()).await,
        Err(VideoError::Tool { .. })
    ));
}
