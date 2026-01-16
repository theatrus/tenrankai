use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// User-editable metadata for images
///
/// This struct combines data from two sources:
/// - `.md` sidecar files: title, description, location, and technical overrides
/// - `.toml` sidecar files: picks, comments, tags, AI analysis
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ImageUserMetadata {
    // === From .md sidecar (human-editable description) ===
    /// Image title (from .md frontmatter)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,

    /// Markdown description (body of .md file)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Human-readable location name
    #[serde(skip_serializing_if = "Option::is_none")]
    pub location: Option<String>,

    // === Technical overrides from .md frontmatter ===
    /// Camera make override
    #[serde(skip_serializing_if = "Option::is_none")]
    pub camera_make: Option<String>,

    /// Camera model override
    #[serde(skip_serializing_if = "Option::is_none")]
    pub camera_model: Option<String>,

    /// Lens model override
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lens_model: Option<String>,

    /// ISO override
    #[serde(skip_serializing_if = "Option::is_none")]
    pub iso: Option<u32>,

    /// Aperture override (e.g., "f/2.8")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub aperture: Option<String>,

    /// Shutter speed override (e.g., "1/200")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shutter_speed: Option<String>,

    /// Focal length override (e.g., "85mm")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub focal_length: Option<String>,

    /// Capture date override (ISO 8601 format)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capture_date: Option<String>,

    // === Astronomical fields (from .md frontmatter) ===
    /// Telescope used
    #[serde(skip_serializing_if = "Option::is_none")]
    pub telescope: Option<String>,

    /// Mount used
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mount: Option<String>,

    /// Filters used
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filters: Option<String>,

    /// Total exposure time in hours
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_exposure_time: Option<f32>,

    /// Right ascension (e.g., "00h 42m 44s")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ra: Option<String>,

    /// Declination (e.g., "+41° 16' 09\"")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dec: Option<String>,

    /// Additional details/notes
    #[serde(skip_serializing_if = "Option::is_none")]
    pub additional_details: Option<String>,

    // === Location coordinates (from .md frontmatter) ===
    /// Latitude override
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latitude: Option<f64>,

    /// Longitude override
    #[serde(skip_serializing_if = "Option::is_none")]
    pub longitude: Option<f64>,

    // === From .toml sidecar (app-managed) ===
    /// Discussion-style comments thread
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub comments: Vec<Comment>,

    /// Whether this image is highlighted/starred
    #[serde(default)]
    pub highlighted: bool,

    /// Pick status for the image
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pick_status: Option<PickStatus>,

    /// Custom tags
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,

    /// Last modified timestamp (for .toml fields)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_modified: Option<DateTime<Utc>>,

    /// Username of last editor (for .toml fields)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modified_by: Option<String>,

    /// AI-generated keywords describing the image
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ai_keywords: Vec<String>,

    /// AI-generated accessibility alt-text
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ai_alt_text: Option<String>,

    /// When AI analysis was performed
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ai_analyzed_at: Option<DateTime<Utc>>,
}

/// A single comment in a discussion thread
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Comment {
    /// Unique identifier for the comment
    pub id: String,

    /// Username of the commenter
    pub author: String,

    /// The comment text
    pub text: String,

    /// When the comment was created
    pub created_at: DateTime<Utc>,

    /// When the comment was last edited (if edited)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub edited_at: Option<DateTime<Utc>>,

    /// Previous versions of the comment (for edit history)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub versions: Vec<CommentVersion>,

    /// Optional selected area on the image (percentage coordinates)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_area: Option<ImageArea>,
}

/// Represents a selected area on an image with percentage-based coordinates
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageArea {
    /// X coordinate as percentage (0-100)
    pub x: f32,
    /// Y coordinate as percentage (0-100)
    pub y: f32,
    /// Width as percentage (0-100)
    pub width: f32,
    /// Height as percentage (0-100)
    pub height: f32,
}

/// A previous version of a comment
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommentVersion {
    /// The old text
    pub text: String,

    /// When this version was created
    pub edited_at: DateTime<Utc>,

    /// Who edited it
    pub edited_by: String,
}

/// Pick status for culling/selection workflows
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PickStatus {
    /// Image is selected/picked
    Pick,
    /// Image is rejected
    NoPick,
    /// Image is undecided (different from None, which means no status set)
    Undecided,
}

impl ImageUserMetadata {
    /// Create new metadata with current timestamp
    pub fn new() -> Self {
        Self {
            last_modified: Some(Utc::now()),
            ..Default::default()
        }
    }

    /// Update the last modified timestamp and user
    pub fn update_modified(&mut self, username: Option<String>) {
        self.last_modified = Some(Utc::now());
        self.modified_by = username;
    }

    /// Check if metadata has any actual content
    pub fn is_empty(&self) -> bool {
        self.is_md_empty() && self.is_toml_empty()
    }

    /// Check if .md sidecar fields are empty
    pub fn is_md_empty(&self) -> bool {
        self.title.is_none()
            && self.description.is_none()
            && self.location.is_none()
            && self.camera_make.is_none()
            && self.camera_model.is_none()
            && self.lens_model.is_none()
            && self.iso.is_none()
            && self.aperture.is_none()
            && self.shutter_speed.is_none()
            && self.focal_length.is_none()
            && self.capture_date.is_none()
            && self.telescope.is_none()
            && self.mount.is_none()
            && self.filters.is_none()
            && self.total_exposure_time.is_none()
            && self.ra.is_none()
            && self.dec.is_none()
            && self.additional_details.is_none()
            && self.latitude.is_none()
            && self.longitude.is_none()
    }

    /// Check if .toml sidecar fields are empty
    pub fn is_toml_empty(&self) -> bool {
        self.comments.is_empty()
            && !self.highlighted
            && self.pick_status.is_none()
            && self.tags.is_empty()
            && self.ai_keywords.is_empty()
            && self.ai_alt_text.is_none()
    }

    /// Add a new comment to the thread
    pub fn add_comment(
        &mut self,
        author: String,
        text: String,
        image_area: Option<ImageArea>,
    ) -> String {
        let id = uuid::Uuid::new_v4().to_string();
        let comment = Comment {
            id: id.clone(),
            author: author.clone(),
            text,
            created_at: Utc::now(),
            edited_at: None,
            versions: Vec::new(),
            image_area,
        };
        self.comments.push(comment);
        self.update_modified(Some(author));
        id
    }

    /// Edit a comment (only allowed by the author)
    pub fn edit_comment(
        &mut self,
        comment_id: &str,
        editor: &str,
        new_text: String,
        new_image_area: Option<ImageArea>,
    ) -> Result<(), String> {
        let comment = self
            .comments
            .iter_mut()
            .find(|c| c.id == comment_id)
            .ok_or_else(|| "Comment not found".to_string())?;

        if comment.author != editor {
            return Err("Only the comment author can edit their comment".to_string());
        }

        // Save the old version
        let old_version = CommentVersion {
            text: comment.text.clone(),
            edited_at: comment.edited_at.unwrap_or(comment.created_at),
            edited_by: editor.to_string(),
        };
        comment.versions.push(old_version);

        // Update the comment
        comment.text = new_text;
        comment.image_area = new_image_area;
        comment.edited_at = Some(Utc::now());

        self.update_modified(Some(editor.to_string()));
        Ok(())
    }

    /// Delete a comment (only allowed by the author)
    pub fn delete_comment(&mut self, comment_id: &str, deleter: &str) -> Result<(), String> {
        let pos = self
            .comments
            .iter()
            .position(|c| c.id == comment_id)
            .ok_or_else(|| "Comment not found".to_string())?;

        if self.comments[pos].author != deleter {
            return Err("Only the comment author can delete their comment".to_string());
        }

        self.comments.remove(pos);
        self.update_modified(Some(deleter.to_string()));
        Ok(())
    }

    /// Set AI analysis results
    pub fn set_ai_analysis(&mut self, keywords: Vec<String>, alt_text: String) {
        self.ai_keywords = keywords;
        self.ai_alt_text = Some(alt_text);
        self.ai_analyzed_at = Some(Utc::now());
    }

    /// Check if AI analysis has been performed
    pub fn has_ai_analysis(&self) -> bool {
        self.ai_analyzed_at.is_some()
    }

    /// Clear AI analysis results
    pub fn clear_ai_analysis(&mut self) {
        self.ai_keywords = vec![];
        self.ai_alt_text = None;
        self.ai_analyzed_at = None;
    }
}

impl Comment {
    /// Create a new comment
    pub fn new(author: String, text: String) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            author,
            text,
            created_at: Utc::now(),
            edited_at: None,
            versions: Vec::new(),
            image_area: None,
        }
    }

    /// Create a new comment with an image area
    pub fn new_with_area(author: String, text: String, image_area: ImageArea) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            author,
            text,
            created_at: Utc::now(),
            edited_at: None,
            versions: Vec::new(),
            image_area: Some(image_area),
        }
    }
}
