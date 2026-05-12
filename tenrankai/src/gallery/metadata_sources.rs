use super::{CameraInfo, LocationInfo};
use crate::metadata_storage::ImageUserMetadata;
use crate::storage::DynStorage;
use chrono::DateTime;
use quick_xml::Reader;
use quick_xml::events::Event;
use std::time::SystemTime;
use tracing::{debug, trace};

/// Reads XMP metadata from a sidecar file using storage abstraction
pub async fn read_xmp_metadata_from_storage(
    storage: &DynStorage,
    xmp_path: &str,
) -> Option<XmpMetadata> {
    match storage.read_to_string(xmp_path).await {
        Ok(content) => parse_xmp_content(&content),
        Err(_) => None,
    }
}

#[derive(Debug, Default, Clone)]
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
                    "exif:LensModel" => {
                        if let Some(value) = get_attribute_value(&e, b"rdf:resource") {
                            metadata.lens_model = Some(value);
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
                        if let Some(value) = get_attribute_value(&e, b"rdf:resource")
                            && let Ok(dt) = DateTime::parse_from_rfc3339(&value)
                        {
                            metadata.capture_date = Some(SystemTime::from(dt));
                        }
                    }
                    _ => {}
                }
            }
            Ok(Event::Text(e)) => {
                if in_title {
                    metadata.title = Some(e.decode().unwrap_or_default().to_string());
                    in_title = false;
                } else if in_description {
                    metadata.description = Some(e.decode().unwrap_or_default().to_string());
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

/// Merge metadata from multiple sources with priority:
/// 1. User metadata from .md file (highest priority)
/// 2. XMP sidecar
/// 3. EXIF data (lowest priority)
pub fn merge_metadata_sources(
    exif_camera: Option<CameraInfo>,
    exif_location: Option<LocationInfo>,
    xmp: Option<XmpMetadata>,
    user_metadata: Option<&ImageUserMetadata>,
) -> (Option<CameraInfo>, Option<LocationInfo>) {
    let mut camera_info = exif_camera.unwrap_or(CameraInfo {
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

    // Apply user metadata overrides (highest priority)
    if let Some(um) = user_metadata {
        camera_info.camera_make = um.camera_make.clone().or(camera_info.camera_make);
        camera_info.camera_model = um.camera_model.clone().or(camera_info.camera_model);
        camera_info.lens_model = um.lens_model.clone().or(camera_info.lens_model);
        camera_info.iso = um.iso.or(camera_info.iso);
        camera_info.aperture = um.aperture.clone().or(camera_info.aperture);
        camera_info.shutter_speed = um.shutter_speed.clone().or(camera_info.shutter_speed);
        camera_info.focal_length = um.focal_length.clone().or(camera_info.focal_length);

        // Astronomical fields (only from user metadata)
        camera_info.telescope = um.telescope.clone();
        camera_info.mount = um.mount.clone();
        camera_info.filters = um.filters.clone();
        camera_info.total_exposure_time = um.total_exposure_time;
        camera_info.ra = um.ra.clone();
        camera_info.dec = um.dec.clone();
        camera_info.additional_details = um.additional_details.clone();
    }

    // Handle location info
    let mut location_info = exif_location;

    // Apply XMP location overrides
    if let Some(ref xmp_data) = xmp
        && xmp_data.latitude.is_some()
        && xmp_data.longitude.is_some()
    {
        let lat = xmp_data.latitude.unwrap();
        let lon = xmp_data.longitude.unwrap();
        location_info = Some(LocationInfo {
            latitude: lat,
            longitude: lon,
            google_maps_url: format!("https://maps.google.com/?q={},{}", lat, lon),
            apple_maps_url: format!("https://maps.apple.com/?ll={},{}", lat, lon),
        });
    }

    // Apply user metadata location overrides (highest priority)
    if let Some(um) = user_metadata
        && um.latitude.is_some()
        && um.longitude.is_some()
    {
        let lat = um.latitude.unwrap();
        let lon = um.longitude.unwrap();
        location_info = Some(LocationInfo {
            latitude: lat,
            longitude: lon,
            google_maps_url: format!("https://maps.google.com/?q={},{}", lat, lon),
            apple_maps_url: format!("https://maps.apple.com/?ll={},{}", lat, lon),
        });
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_xmp_parsing_basic() {
        let xmp_content = r#"<?xml version="1.0" encoding="UTF-8"?>
<x:xmpmeta xmlns:x="adobe:ns:meta/" x:xmptk="XMP Core 5.4.0">
    <rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
        <rdf:Description rdf:about=""
            xmlns:dc="http://purl.org/dc/elements/1.1/"
            xmlns:exif="http://ns.adobe.com/exif/1.0/">
            <dc:title>
                <rdf:Alt>
                    <rdf:li xml:lang="x-default">Test Image Title</rdf:li>
                </rdf:Alt>
            </dc:title>
            <dc:description>
                <rdf:Alt>
                    <rdf:li xml:lang="x-default">Test image description</rdf:li>
                </rdf:Alt>
            </dc:description>
            <exif:Make rdf:resource="Canon"/>
            <exif:Model rdf:resource="Canon EOS R5"/>
            <exif:ISOSpeedRatings rdf:resource="400"/>
            <exif:FNumber rdf:resource="2.8"/>
            <exif:ExposureTime rdf:resource="1/200"/>
            <exif:FocalLength rdf:resource="85"/>
            <exif:LensModel rdf:resource="Canon RF 85mm f/1.2L USM"/>
        </rdf:Description>
    </rdf:RDF>
</x:xmpmeta>"#;

        let metadata = parse_xmp_content(xmp_content).unwrap();

        assert_eq!(metadata.title, Some("Test Image Title".to_string()));
        assert_eq!(
            metadata.description,
            Some("Test image description".to_string())
        );
        assert_eq!(metadata.camera_make, Some("Canon".to_string()));
        assert_eq!(metadata.camera_model, Some("Canon EOS R5".to_string()));
        assert_eq!(
            metadata.lens_model,
            Some("Canon RF 85mm f/1.2L USM".to_string())
        );
        assert_eq!(metadata.iso, Some(400));
        assert_eq!(metadata.aperture, Some("f/2.8".to_string()));
        assert_eq!(metadata.shutter_speed, Some("1/200".to_string()));
        assert_eq!(metadata.focal_length, Some("85 mm".to_string()));
    }

    #[test]
    fn test_xmp_parsing_with_datetime() {
        let xmp_content = r#"<?xml version="1.0" encoding="UTF-8"?>
<x:xmpmeta xmlns:x="adobe:ns:meta/">
    <rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
        <rdf:Description rdf:about=""
            xmlns:exif="http://ns.adobe.com/exif/1.0/">
            <exif:DateTimeOriginal rdf:resource="2024-01-15T14:30:00+00:00"/>
        </rdf:Description>
    </rdf:RDF>
</x:xmpmeta>"#;

        let metadata = parse_xmp_content(xmp_content).unwrap();
        assert!(metadata.capture_date.is_some());
    }

    #[test]
    fn test_metadata_merge_priority() {
        // Create test data
        let exif_camera = Some(CameraInfo {
            camera_make: Some("Canon".to_string()),
            camera_model: Some("EOS R5".to_string()),
            lens_model: None,
            iso: Some(200),
            aperture: Some("f/4".to_string()),
            shutter_speed: Some("1/100".to_string()),
            focal_length: Some("50mm".to_string()),
            telescope: None,
            mount: None,
            filters: None,
            total_exposure_time: None,
            ra: None,
            dec: None,
            additional_details: None,
        });

        let xmp = Some(XmpMetadata {
            title: Some("XMP Title".to_string()),
            description: Some("XMP Description".to_string()),
            camera_make: Some("Canon Updated".to_string()), // Should override EXIF
            camera_model: None,
            lens_model: Some("Canon RF 50mm f/1.2L USM".to_string()),
            iso: Some(400), // Should override EXIF
            aperture: None,
            shutter_speed: None,
            focal_length: None,
            capture_date: None,
            latitude: Some(37.7749),
            longitude: Some(-122.4194),
        });

        let user_metadata = ImageUserMetadata {
            title: Some("User Metadata Title".to_string()), // Should override XMP
            camera_make: None,
            camera_model: Some("EOS R5 Mark II".to_string()), // Should override all
            lens_model: None,
            iso: Some(800),                      // Should override all
            aperture: Some("f/2.8".to_string()), // Should override EXIF
            shutter_speed: None,
            focal_length: None,
            telescope: Some("RedCat 51".to_string()),
            mount: Some("EQ6-R Pro".to_string()),
            filters: Some("L-eXtreme".to_string()),
            total_exposure_time: Some(3.5),
            ra: Some("00h 42m".to_string()),
            dec: Some("+41° 16'".to_string()),
            additional_details: Some("Test details".to_string()),
            latitude: Some(40.7128),
            longitude: Some(-74.0060),
            capture_date: None,
            ..Default::default()
        };

        let exif_location = Some(LocationInfo {
            latitude: 34.0522,
            longitude: -118.2437,
            google_maps_url: "old_url".to_string(),
            apple_maps_url: "old_url".to_string(),
        });

        let (camera, location) = merge_metadata_sources(
            exif_camera,
            exif_location,
            xmp.clone(),
            Some(&user_metadata),
        );

        let camera = camera.unwrap();

        // Check priority: User metadata > XMP > EXIF
        assert_eq!(camera.camera_make, Some("Canon Updated".to_string())); // From XMP
        assert_eq!(camera.camera_model, Some("EOS R5 Mark II".to_string())); // From user metadata
        assert_eq!(
            camera.lens_model,
            Some("Canon RF 50mm f/1.2L USM".to_string())
        ); // From XMP
        assert_eq!(camera.iso, Some(800)); // From user metadata
        assert_eq!(camera.aperture, Some("f/2.8".to_string())); // From user metadata
        assert_eq!(camera.shutter_speed, Some("1/100".to_string())); // From EXIF
        assert_eq!(camera.focal_length, Some("50mm".to_string())); // From EXIF

        // Astronomical fields only from user metadata
        assert_eq!(camera.telescope, Some("RedCat 51".to_string()));
        assert_eq!(camera.mount, Some("EQ6-R Pro".to_string()));
        assert_eq!(camera.filters, Some("L-eXtreme".to_string()));
        assert_eq!(camera.total_exposure_time, Some(3.5));
        assert_eq!(camera.ra, Some("00h 42m".to_string()));
        assert_eq!(camera.dec, Some("+41° 16'".to_string()));
        assert_eq!(camera.additional_details, Some("Test details".to_string()));

        // Location priority: User metadata > XMP > EXIF
        let location = location.unwrap();
        assert_eq!(location.latitude, 40.7128); // From user metadata
        assert_eq!(location.longitude, -74.0060); // From user metadata
    }

    #[test]
    fn test_metadata_merge_empty_exif() {
        let xmp = Some(XmpMetadata {
            title: Some("XMP Title".to_string()),
            description: None,
            camera_make: Some("Sony".to_string()),
            camera_model: Some("A7R V".to_string()),
            lens_model: None,
            iso: Some(100),
            aperture: Some("f/1.4".to_string()),
            shutter_speed: None,
            focal_length: None,
            capture_date: None,
            latitude: None,
            longitude: None,
        });

        let (camera, location) = merge_metadata_sources(None, None, xmp, None);

        let camera = camera.unwrap();
        assert_eq!(camera.camera_make, Some("Sony".to_string()));
        assert_eq!(camera.camera_model, Some("A7R V".to_string()));
        assert_eq!(camera.iso, Some(100));
        assert_eq!(camera.aperture, Some("f/1.4".to_string()));

        assert!(location.is_none());
    }

    #[test]
    fn test_get_attribute_value() {
        use quick_xml::events::Event;

        let xml = r#"<exif:Make rdf:resource="Canon" other:attr="value"/>"#;
        let mut reader = Reader::from_str(xml);

        let mut buf = Vec::new();
        if let Ok(Event::Empty(e)) = reader.read_event_into(&mut buf) {
            assert_eq!(
                get_attribute_value(&e, b"rdf:resource"),
                Some("Canon".to_string())
            );
            assert_eq!(
                get_attribute_value(&e, b"other:attr"),
                Some("value".to_string())
            );
            assert_eq!(get_attribute_value(&e, b"nonexistent"), None);
        }
    }
}
