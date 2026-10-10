use crate::{Result, VideoError};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoMetadata {
    pub container: String,
    pub duration_seconds: f64,
    pub file_size: u64,
    pub bitrate: Option<u64>,
    pub video: VideoStream,
    pub audio: Option<AudioStream>,
    pub capture_time: Option<CaptureTime>,
    pub camera_make: Option<String>,
    pub camera_model: Option<String>,
    pub lens_model: Option<String>,
    pub location: Option<Location>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoStream {
    pub index: u32,
    pub codec: String,
    pub profile: Option<String>,
    pub level: Option<u32>,
    pub pixel_format: Option<String>,
    pub encoded_dimensions: (u32, u32),
    pub display_dimensions: (u32, u32),
    pub rotation_degrees: i32,
    pub sample_aspect_ratio: (u32, u32),
    /// Original rational rate, retained instead of replacing it with a rounded FPS.
    pub average_frame_rate: Option<String>,
    pub frame_rate: Option<f64>,
    pub bitrate: Option<u64>,
    pub color_primaries: Option<String>,
    pub color_transfer: Option<String>,
    pub color_space: Option<String>,
    pub color_range: Option<String>,
    pub is_hdr: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioStream {
    pub index: u32,
    pub codec: String,
    pub profile: Option<String>,
    pub channels: Option<u32>,
    pub sample_rate: Option<u32>,
    pub bitrate: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaptureTime {
    pub raw: String,
    pub tag: String,
    /// None when the source omits a timezone or has an unrecognized date.
    pub utc: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Location {
    pub latitude: f64,
    pub longitude: f64,
    pub altitude_meters: Option<f64>,
}

fn string(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)?
        .as_str()
        .filter(|s| !s.is_empty() && *s != "unknown" && *s != "N/A")
        .map(str::to_owned)
}

fn number(value: &Value, key: &str) -> Option<f64> {
    let value = value.get(key)?;
    value
        .as_f64()
        .or_else(|| value.as_str()?.parse().ok())
        .filter(|n| n.is_finite())
}

fn unsigned(value: &Value, key: &str) -> Option<u64> {
    let value = value.get(key)?;
    value.as_u64().or_else(|| value.as_str()?.parse().ok())
}

fn rational(value: &str, separator: char) -> Option<(u32, u32)> {
    let (num, den) = value.split_once(separator)?;
    let pair = (num.parse().ok()?, den.parse().ok()?);
    (pair.0 > 0 && pair.1 > 0).then_some(pair)
}

fn tag<'a>(format: &'a Value, stream: &'a Value, keys: &[&str]) -> Option<(String, &'a str)> {
    for key in keys {
        for section in [format, stream] {
            if let Some(tags) = section.get("tags").and_then(Value::as_object)
                && let Some((name, value)) =
                    tags.iter().find(|(name, _)| name.eq_ignore_ascii_case(key))
                && let Some(text) = value.as_str().filter(|s| !s.is_empty())
            {
                return Some((name.clone(), text));
            }
        }
    }
    None
}

fn parse_location(text: &str) -> Option<Location> {
    let text = text.trim_end_matches('/');
    if !text.starts_with(['+', '-']) {
        return None;
    }
    let mut positions: Vec<usize> = text
        .char_indices()
        .skip(1)
        .filter(|(_, c)| matches!(c, '+' | '-'))
        .map(|(i, _)| i)
        .collect();
    positions.push(text.len());
    let split = *positions.first()?;
    let latitude: f64 = text[..split].parse().ok()?;
    let longitude: f64 = text[split..*positions.get(1)?].parse().ok()?;
    let altitude_meters = if positions.len() > 2 {
        Some(text[positions[1]..positions[2]].parse::<f64>().ok()?)
    } else {
        None
    };
    (latitude.is_finite()
        && longitude.is_finite()
        && (-90.0..=90.0).contains(&latitude)
        && (-180.0..=180.0).contains(&longitude)
        && altitude_meters.is_none_or(f64::is_finite))
    .then_some(Location {
        latitude,
        longitude,
        altitude_meters,
    })
}

impl VideoMetadata {
    pub(crate) fn from_probe(value: &Value, file_size: u64) -> Result<Self> {
        let streams = value
            .get("streams")
            .and_then(Value::as_array)
            .ok_or_else(|| VideoError::Unsupported("missing streams".into()))?;
        let format = &value["format"];
        let stream = streams
            .iter()
            .filter(|s| {
                s["codec_type"] == "video"
                    && s["disposition"]["attached_pic"] != 1
                    && unsigned(s, "width").is_some_and(|w| w > 0)
                    && unsigned(s, "height").is_some_and(|h| h > 0)
            })
            .max_by_key(|s| s["disposition"]["default"].as_u64().unwrap_or(0))
            .ok_or_else(|| VideoError::Unsupported("no usable video stream".into()))?;
        // Audio can outlast the video stream. Apply the clip limit to the whole
        // container as well, rather than admitting a short video with long audio.
        let duration_seconds = [number(stream, "duration"), number(format, "duration")]
            .into_iter()
            .flatten()
            .filter(|duration| *duration > 0.0)
            .reduce(f64::max)
            .ok_or_else(|| VideoError::Unsupported("missing or invalid duration".into()))?;
        let width = unsigned(stream, "width").unwrap();
        let height = unsigned(stream, "height").unwrap();
        if width > 32768 || height > 32768 || width * height > 64_000_000 {
            return Err(VideoError::Unsupported(
                "source exceeds the 64 megapixel decode limit".into(),
            ));
        }
        let encoded_dimensions = (width as u32, height as u32);
        let side_data = stream.get("side_data_list").and_then(Value::as_array);
        let rotation = side_data
            .and_then(|entries| entries.iter().find_map(|s| number(s, "rotation")))
            .or_else(|| number(&stream["tags"], "rotate"))
            .unwrap_or(0.0);
        if rotation.fract().abs() > 0.01 || (rotation % 90.0).abs() > 0.01 {
            return Err(VideoError::Unsupported(
                "non-orthogonal video rotation".into(),
            ));
        }
        let rotation_degrees = rotation.rem_euclid(360.0) as i32;
        let sample_aspect_ratio = string(stream, "sample_aspect_ratio")
            .and_then(|s| rational(&s, ':'))
            .unwrap_or((1, 1));
        let display_width =
            (width as f64 * sample_aspect_ratio.0 as f64 / sample_aspect_ratio.1 as f64).round();
        if !(1.0..=32768.0).contains(&display_width) {
            return Err(VideoError::Unsupported(
                "invalid display aspect ratio".into(),
            ));
        }
        let mut display_dimensions = (display_width as u32, height as u32);
        if rotation_degrees == 90 || rotation_degrees == 270 {
            display_dimensions = (display_dimensions.1, display_dimensions.0);
        }
        let average_frame_rate = string(stream, "avg_frame_rate");
        let frame_rate = average_frame_rate
            .as_deref()
            .and_then(|s| rational(s, '/'))
            .or_else(|| string(stream, "r_frame_rate").and_then(|s| rational(&s, '/')))
            .map(|(num, den)| num as f64 / den as f64);
        let color_transfer = string(stream, "color_transfer");
        let is_hdr = matches!(
            color_transfer.as_deref(),
            Some("smpte2084" | "arib-std-b67")
        ) || side_data.is_some_and(|entries| {
            entries
                .iter()
                .any(|s| string(s, "side_data_type").is_some_and(|kind| kind.contains("DOVI")))
        });
        let audio = streams
            .iter()
            .filter(|s| s["codec_type"] == "audio")
            .max_by_key(|s| s["disposition"]["default"].as_u64().unwrap_or(0))
            .map(|s| AudioStream {
                index: unsigned(s, "index").unwrap_or(0) as u32,
                codec: string(s, "codec_name").unwrap_or_else(|| "unknown".into()),
                profile: string(s, "profile"),
                channels: unsigned(s, "channels").and_then(|n| n.try_into().ok()),
                sample_rate: unsigned(s, "sample_rate").and_then(|n| n.try_into().ok()),
                bitrate: unsigned(s, "bit_rate"),
            });
        let capture_time = tag(
            format,
            stream,
            &[
                "com.apple.quicktime.creationdate",
                "date_time_original",
                "creation_time",
            ],
        )
        .map(|(tag, raw)| CaptureTime {
            raw: raw.into(),
            tag,
            utc: DateTime::parse_from_rfc3339(raw)
                .ok()
                .map(|date| date.with_timezone(&Utc)),
        });
        let camera_tag =
            |keys: &[&str]| tag(format, stream, keys).map(|(_, value)| value.to_string());
        Ok(Self {
            container: string(format, "format_name").unwrap_or_default(),
            duration_seconds,
            file_size,
            bitrate: unsigned(format, "bit_rate").filter(|n| *n > 0),
            video: VideoStream {
                index: unsigned(stream, "index").unwrap_or(0) as u32,
                codec: string(stream, "codec_name").unwrap_or_else(|| "unknown".into()),
                profile: string(stream, "profile"),
                level: unsigned(stream, "level").and_then(|n| n.try_into().ok()),
                pixel_format: string(stream, "pix_fmt"),
                encoded_dimensions,
                display_dimensions,
                rotation_degrees,
                sample_aspect_ratio,
                average_frame_rate,
                frame_rate,
                bitrate: unsigned(stream, "bit_rate").filter(|n| *n > 0),
                color_primaries: string(stream, "color_primaries"),
                color_transfer,
                color_space: string(stream, "color_space"),
                color_range: string(stream, "color_range"),
                is_hdr,
            },
            audio,
            capture_time,
            camera_make: camera_tag(&["com.apple.quicktime.make", "make"]),
            camera_model: camera_tag(&["com.apple.quicktime.model", "model"]),
            lens_model: camera_tag(&["lens_model", "lens"]),
            location: tag(
                format,
                stream,
                &[
                    "com.apple.quicktime.location.iso6709",
                    "location",
                    "location-eng",
                ],
            )
            .and_then(|(_, value)| parse_location(value)),
        })
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use serde_json::json;

    pub(crate) fn compatible_probe() -> Value {
        json!({
            "streams": [
                {"index": 2, "codec_type": "video", "codec_name": "h264", "profile": "High", "level": 40,
                 "width": 1920, "height": 1080, "pix_fmt": "yuv420p", "sample_aspect_ratio": "1:1",
                 "avg_frame_rate": "30000/1001", "bit_rate": "4000000", "disposition": {"default": 1}},
                {"index": 3, "codec_type": "audio", "codec_name": "aac", "profile": "LC", "channels": 2, "sample_rate": "48000"}
            ],
            "format": {"format_name": "mov,mp4,m4a,3gp,3g2,mj2", "duration": "12.5", "bit_rate": "4128000"}
        })
    }

    #[test]
    fn uses_the_longer_valid_stream_or_container_duration() {
        let mut probe = compatible_probe();
        probe["streams"][0]["duration"] = json!("2.0");
        assert_eq!(
            VideoMetadata::from_probe(&probe, 1000)
                .unwrap()
                .duration_seconds,
            12.5
        );
        probe["streams"][0]["duration"] = json!("0");
        assert!(VideoMetadata::from_probe(&probe, 1000).is_ok());
        probe["streams"][0]["duration"] = json!("15.0");
        assert_eq!(
            VideoMetadata::from_probe(&probe, 1000)
                .unwrap()
                .duration_seconds,
            15.0
        );
    }

    #[test]
    fn normalizes_phone_metadata_without_losing_provenance() {
        let mut probe = compatible_probe();
        probe["streams"][0]["side_data_list"] =
            json!([{ "side_data_type": "Display Matrix", "rotation": -90 }]);
        probe["format"]["tags"] = json!({
            "creation_time": "2026-09-20T16:00:00Z",
            "com.apple.quicktime.creationdate": "2026-09-19T12:30:00-07:00",
            "com.apple.quicktime.make": "Apple", "com.apple.quicktime.model": "Phone",
            "com.apple.quicktime.location.ISO6709": "+37.7749-122.4194+015.0/"
        });
        let metadata = VideoMetadata::from_probe(&probe, 1000).unwrap();
        assert_eq!(metadata.video.index, 2);
        assert_eq!(metadata.audio.unwrap().index, 3);
        assert_eq!(metadata.video.display_dimensions, (1080, 1920));
        assert_eq!(metadata.video.rotation_degrees, 270);
        assert_eq!(
            metadata.video.average_frame_rate.as_deref(),
            Some("30000/1001")
        );
        assert!((metadata.video.frame_rate.unwrap() - 29.97003).abs() < 0.00001);
        assert_eq!(metadata.camera_make.as_deref(), Some("Apple"));
        let capture = metadata.capture_time.unwrap();
        assert_eq!(capture.tag, "com.apple.quicktime.creationdate");
        assert_eq!(
            capture.utc.unwrap().to_rfc3339(),
            "2026-09-19T19:30:00+00:00"
        );
        let location = metadata.location.unwrap();
        assert_eq!(location.longitude, -122.4194);
        assert_eq!(location.altitude_meters, Some(15.0));
    }

    #[test]
    fn retains_unknown_timezone_and_rejects_invalid_locations() {
        let mut probe = compatible_probe();
        probe["format"]["tags"] =
            json!({"creation_time": "2026-09-19T12:30:00", "location": "+91.0-122.4/"});
        let metadata = VideoMetadata::from_probe(&probe, 1000).unwrap();
        assert!(metadata.capture_time.unwrap().utc.is_none());
        assert!(metadata.location.is_none());
        for text in ["garbage", "+NaN-122/", "+37-181/", "+37-122+NaN/", "+", ""] {
            assert!(parse_location(text).is_none(), "{text}");
        }
        assert!(parse_location("+37.0-122.0/").is_some());
    }

    #[test]
    fn excludes_cover_art_and_rejects_audio_only_or_unknown_duration() {
        let mut probe = compatible_probe();
        probe["streams"][0]["disposition"]["attached_pic"] = json!(1);
        assert!(matches!(
            VideoMetadata::from_probe(&probe, 1000),
            Err(VideoError::Unsupported(_))
        ));
        probe["streams"][0]["disposition"]["attached_pic"] = json!(0);
        for duration in ["N/A", "NaN", "inf", "0", "-1"] {
            probe["format"]["duration"] = json!(duration);
            assert!(
                VideoMetadata::from_probe(&probe, 1000).is_err(),
                "{duration}"
            );
        }
    }

    #[test]
    fn accounts_for_anamorphic_pixels_and_hdr() {
        let mut probe = compatible_probe();
        probe["streams"][0]["width"] = json!(1440);
        probe["streams"][0]["sample_aspect_ratio"] = json!("4:3");
        probe["streams"][0]["color_transfer"] = json!("arib-std-b67");
        let metadata = VideoMetadata::from_probe(&probe, 1000).unwrap();
        assert_eq!(metadata.video.display_dimensions, (1920, 1080));
        assert!(metadata.video.is_hdr);
        probe["streams"][0]["width"] = json!(32768);
        probe["streams"][0]["height"] = json!(32768);
        assert!(VideoMetadata::from_probe(&probe, 1000).is_err());
    }
}
