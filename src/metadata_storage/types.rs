use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// User-editable metadata for images
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ImageUserMetadata {
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

    /// Last modified timestamp
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_modified: Option<DateTime<Utc>>,

    /// Username of last editor
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modified_by: Option<String>,
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
        self.comments.is_empty()
            && !self.highlighted
            && self.pick_status.is_none()
            && self.tags.is_empty()
    }

    /// Add a new comment to the thread
    pub fn add_comment(&mut self, author: String, text: String) -> String {
        let id = uuid::Uuid::new_v4().to_string();
        let comment = Comment {
            id: id.clone(),
            author: author.clone(),
            text,
            created_at: Utc::now(),
            edited_at: None,
        };
        self.comments.push(comment);
        self.update_modified(Some(author));
        id
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
        }
    }
}
