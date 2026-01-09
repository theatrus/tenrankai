use super::{CameraInfo, ImageMarkdownConfig, ImageMarkdownMetadata, LocationInfo};
use chrono::DateTime;
use quick_xml::events::Event;
use quick_xml::Reader;
use std::path::Path;
use std::time::SystemTime;
use tracing::{debug, trace};

/// Reads XMP metadata from a sidecar file
pub async fn read_xmp_metadata(xmp_path: &Path) -> Option<XmpMetadata> {
    match tokio::fs::read_to_string(xmp_path).await {
        Ok(content) => parse_xmp_content(&content),
        Err(_) => None,
    }
}

#[derive(Debug, Default)]
pub struct XmpMetadata {
    pub title: Option<String>,
    pub description: Option<String>,
    pub camera_make: Option<String>,
    pub camera_model: Option<String>,
    pub lens_model: Option<String>,
    pub iso: Option<u32>,
    pub aperture: Option<String>,
    pub shutter_speed: Option<String>,
    pub focal_length: Option<String>,
    pub capture_date: Option<SystemTime>,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
}

fn parse_xmp_content(content: &str) -> Option<XmpMetadata> {
    let mut reader = Reader::from_str(content);
    reader.config_mut().trim_text(true);
    let mut metadata = XmpMetadata::default();
    let mut in_title = false;
    let mut in_description = false;
    
    let mut buf = Vec::new();
    
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) => {
                let tag_name = e.name();
                let tag_str = String::from_utf8_lossy(tag_name.as_ref());
                
                match tag_str.as_ref() {
                    "dc:title" => {
                        in_title = true;
                    }
                    "dc:description" => {
                        in_description = true;
                    }
                    "exif:Make" => {
                        if let Some(value) = get_attribute_value(&e, b"rdf:resource") {
                            metadata.camera_make = Some(value);
                        }
                    }
                    "exif:Model" => {
                        if let Some(value) = get_attribute_value(&e, b"rdf:resource") {
                            metadata.camera_model = Some(value);
                        }
                    }
                    "exif:ISOSpeedRatings" => {
                        if let Some(value) = get_attribute_value(&e, b"rdf:resource") {
                            metadata.iso = value.parse().ok();
                        }
                    }
                    "exif:FNumber" => {
                        if let Some(value) = get_attribute_value(&e, b"rdf:resource") {
                            metadata.aperture = Some(format!("f/{}", value));
                        }
                    }
                    "exif:ExposureTime" => {
                        if let Some(value) = get_attribute_value(&e, b"rdf:resource") {
                            metadata.shutter_speed = Some(value);
                        }
                    }
                    "exif:FocalLength" => {
                        if let Some(value) = get_attribute_value(&e, b"rdf:resource") {
                            metadata.focal_length = Some(format!("{} mm", value));
                        }
                    }
                    "exif:DateTimeOriginal" => {
                        if let Some(value) = get_attribute_value(&e, b"rdf:resource") {
                            if let Ok(dt) = DateTime::parse_from_rfc3339(&value) {
                                metadata.capture_date = Some(SystemTime::from(dt));
                            }
                        }
                    }
                    _ => {}
                }
            }
            Ok(Event::Text(e)) => {
                if in_title {
                    metadata.title = Some(e.unescape().unwrap_or_default().to_string());
                    in_title = false;
                } else if in_description {
                    metadata.description = Some(e.unescape().unwrap_or_default().to_string());
                    in_description = false;
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => {
                debug!("Error parsing XMP: {}", e);
                return None;
            }
            _ => {}
        }
        buf.clear();
    }
    
    trace!("Parsed XMP metadata: {:?}", metadata);
    Some(metadata)
}

fn get_attribute_value(e: &quick_xml::events::BytesStart, name: &[u8]) -> Option<String> {
    e.attributes()
        .filter_map(|a| a.ok())
        .find(|a| a.key.as_ref() == name)
        .map(|a| String::from_utf8_lossy(&a.value).to_string())
}

/// Reads image markdown metadata file (e.g., image.jpg.md)
pub async fn read_image_markdown_metadata(markdown_path: &Path) -> Option<ImageMarkdownMetadata> {
    match tokio::fs::read_to_string(markdown_path).await {
        Ok(content) => {
            // Check if content starts with TOML front matter
            if content.trim_start().starts_with("+++") {
                // Parse TOML front matter
                let parts: Vec<&str> = content.splitn(3, "+++").collect();
                
                if parts.len() >= 3 {
                    let toml_content = parts[1];
                    let markdown_content = parts[2].trim_start();
                    
                    match toml_edit::de::from_str::<ImageMarkdownConfig>(toml_content) {
                        Ok(config) => {
                            debug!("Successfully parsed image markdown config: {:?}", config);
                            Some(ImageMarkdownMetadata {
                                config,
                                description_markdown: markdown_content.to_string(),
                            })
                        }
                        Err(e) => {
                            debug!("Failed to parse image TOML front matter: {}", e);
                            // Return just the markdown content
                            Some(ImageMarkdownMetadata {
                                config: ImageMarkdownConfig::default(),
                                description_markdown: content,
                            })
                        }
                    }
                } else {
                    // No valid front matter, return the whole content as markdown
                    Some(ImageMarkdownMetadata {
                        config: ImageMarkdownConfig::default(),
                        description_markdown: content,
                    })
                }
            } else {
                // No front matter, just markdown
                Some(ImageMarkdownMetadata {
                    config: ImageMarkdownConfig::default(),
                    description_markdown: content,
                })
            }
        }
        Err(_) => None,
    }
}

/// Merge metadata from multiple sources with priority:
/// 1. Markdown frontmatter (highest priority)
/// 2. XMP sidecar
/// 3. EXIF data (lowest priority)
pub fn merge_metadata_sources(
    exif_camera: Option<CameraInfo>,
    exif_location: Option<LocationInfo>,
    xmp: Option<XmpMetadata>,
    markdown: Option<ImageMarkdownConfig>,
) -> (Option<CameraInfo>, Option<LocationInfo>) {
    let mut camera_info = exif_camera.unwrap_or_else(|| CameraInfo {
        camera_make: None,
        camera_model: None,
        lens_model: None,
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
    
    // Apply XMP overrides
    if let Some(ref xmp_data) = xmp {
        camera_info.camera_make = xmp_data.camera_make.clone().or(camera_info.camera_make);
        camera_info.camera_model = xmp_data.camera_model.clone().or(camera_info.camera_model);
        camera_info.lens_model = xmp_data.lens_model.clone().or(camera_info.lens_model);
        camera_info.iso = xmp_data.iso.or(camera_info.iso);
        camera_info.aperture = xmp_data.aperture.clone().or(camera_info.aperture);
        camera_info.shutter_speed = xmp_data.shutter_speed.clone().or(camera_info.shutter_speed);
        camera_info.focal_length = xmp_data.focal_length.clone().or(camera_info.focal_length);
    }
    
    // Apply markdown overrides (highest priority)
    if let Some(ref md) = markdown {
        camera_info.camera_make = md.camera_make.clone().or(camera_info.camera_make);
        camera_info.camera_model = md.camera_model.clone().or(camera_info.camera_model);
        camera_info.lens_model = md.lens_model.clone().or(camera_info.lens_model);
        camera_info.iso = md.iso.or(camera_info.iso);
        camera_info.aperture = md.aperture.clone().or(camera_info.aperture);
        camera_info.shutter_speed = md.shutter_speed.clone().or(camera_info.shutter_speed);
        camera_info.focal_length = md.focal_length.clone().or(camera_info.focal_length);
        
        // Astronomical fields (only from markdown)
        camera_info.telescope = md.telescope.clone();
        camera_info.mount = md.mount.clone();
        camera_info.filters = md.filters.clone();
        camera_info.total_exposure_time = md.total_exposure_time;
        camera_info.ra = md.ra.clone();
        camera_info.dec = md.dec.clone();
        camera_info.additional_details = md.additional_details.clone();
    }
    
    // Handle location info
    let mut location_info = exif_location;
    
    // Apply XMP location overrides
    if let Some(ref xmp_data) = xmp {
        if xmp_data.latitude.is_some() && xmp_data.longitude.is_some() {
            let lat = xmp_data.latitude.unwrap();
            let lon = xmp_data.longitude.unwrap();
            location_info = Some(LocationInfo {
                latitude: lat,
                longitude: lon,
                google_maps_url: format!("https://maps.google.com/?q={},{}", lat, lon),
                apple_maps_url: format!("https://maps.apple.com/?ll={},{}", lat, lon),
            });
        }
    }
    
    // Apply markdown location overrides (highest priority)
    if let Some(ref md) = markdown {
        if md.latitude.is_some() && md.longitude.is_some() {
            let lat = md.latitude.unwrap();
            let lon = md.longitude.unwrap();
            location_info = Some(LocationInfo {
                latitude: lat,
                longitude: lon,
                google_maps_url: format!("https://maps.google.com/?q={},{}", lat, lon),
                apple_maps_url: format!("https://maps.apple.com/?ll={},{}", lat, lon),
            });
        }
    }
    
    // Only return camera_info if it has at least one field set
    let has_camera_info = camera_info.camera_make.is_some()
        || camera_info.camera_model.is_some()
        || camera_info.lens_model.is_some()
        || camera_info.iso.is_some()
        || camera_info.aperture.is_some()
        || camera_info.shutter_speed.is_some()
        || camera_info.focal_length.is_some()
        || camera_info.telescope.is_some()
        || camera_info.mount.is_some()
        || camera_info.filters.is_some()
        || camera_info.total_exposure_time.is_some()
        || camera_info.ra.is_some()
        || camera_info.dec.is_some()
        || camera_info.additional_details.is_some();
    
    let final_camera_info = if has_camera_info {
        Some(camera_info)
    } else {
        None
    };
    
    (final_camera_info, location_info)
}