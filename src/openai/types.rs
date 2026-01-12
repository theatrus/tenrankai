use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Result of AI image analysis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageAnalysisResult {
    /// Descriptive keywords for the image (5-10)
    pub keywords: Vec<String>,

    /// Accessibility-friendly alt text description
    pub alt_text: String,

    /// When the analysis was performed
    pub analyzed_at: DateTime<Utc>,
}

/// OpenAI API request content types
#[derive(Debug, Serialize)]
#[serde(tag = "type")]
pub enum InputContent {
    #[serde(rename = "input_text")]
    Text { text: String },

    #[serde(rename = "input_image")]
    Image { image_url: String, detail: String },
}

/// OpenAI API request message
#[derive(Debug, Serialize)]
pub struct InputMessage {
    pub role: String,
    pub content: Vec<InputContent>,
}

/// JSON schema for structured output
#[derive(Debug, Serialize)]
pub struct JsonSchema {
    #[serde(rename = "type")]
    pub schema_type: String,
    pub name: String,
    pub schema: serde_json::Value,
    pub strict: bool,
}

/// Text format configuration
#[derive(Debug, Serialize)]
pub struct TextFormat {
    pub format: JsonSchema,
}

/// OpenAI Responses API request
#[derive(Debug, Serialize)]
pub struct OpenAIRequest {
    pub model: String,
    pub input: Vec<InputMessage>,
    pub text: TextFormat,
    pub max_output_tokens: u32,
}

/// OpenAI API response content
#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
pub enum OutputContent {
    #[serde(rename = "output_text")]
    Text { text: String },
    #[serde(other)]
    Other,
}

/// OpenAI API response message
#[derive(Debug, Deserialize)]
pub struct OutputMessage {
    #[serde(rename = "type")]
    #[allow(dead_code)]
    pub message_type: String,
    #[allow(dead_code)]
    pub role: String,
    pub content: Vec<OutputContent>,
}

/// OpenAI Responses API response
#[derive(Debug, Deserialize)]
pub struct OpenAIResponse {
    #[allow(dead_code)]
    pub id: String,
    pub output: Vec<OutputMessage>,
}

/// Parsed analysis response from structured output
#[derive(Debug, Deserialize)]
pub struct AnalysisOutput {
    pub keywords: Vec<String>,
    pub alt_text: String,
}

/// Optional location context for image analysis
#[derive(Debug, Clone, Default)]
pub struct LocationContext {
    pub latitude: f64,
    pub longitude: f64,
}

/// Context information to help refine image analysis
#[derive(Debug, Clone, Default)]
pub struct ImageContext {
    /// GPS coordinates if available
    pub location: Option<LocationContext>,
    /// Title from image or folder markdown
    pub title: Option<String>,
    /// Description from image or folder markdown
    pub description: Option<String>,
    /// Folder title for additional context
    pub folder_title: Option<String>,
    /// Camera/lens information
    pub focal_length: Option<String>,
    pub aperture: Option<String>,
    /// When the photo was taken
    pub capture_date: Option<String>,
}

impl OpenAIRequest {
    /// Create a new image analysis request
    pub fn new_image_analysis(model: &str, base64_image: &str, max_tokens: u32) -> Self {
        Self::new_image_analysis_with_context(model, base64_image, max_tokens, ImageContext::default())
    }

    /// Create a new image analysis request with context
    pub fn new_image_analysis_with_context(
        model: &str,
        base64_image: &str,
        max_tokens: u32,
        context: ImageContext,
    ) -> Self {
        let schema = serde_json::json!({
            "type": "object",
            "properties": {
                "keywords": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "8-12 keywords covering subject, location, composition, lighting, and style"
                },
                "alt_text": {
                    "type": "string",
                    "description": "1-2 sentence description of scene and photographic style"
                }
            },
            "required": ["keywords", "alt_text"],
            "additionalProperties": false
        });

        let prompt = Self::build_analysis_prompt(&context);

        Self {
            model: model.to_string(),
            input: vec![InputMessage {
                role: "user".to_string(),
                content: vec![
                    InputContent::Text { text: prompt },
                    InputContent::Image {
                        image_url: format!("data:image/jpeg;base64,{}", base64_image),
                        detail: "auto".to_string(),
                    },
                ],
            }],
            text: TextFormat {
                format: JsonSchema {
                    schema_type: "json_schema".to_string(),
                    name: "image_analysis".to_string(),
                    schema,
                    strict: true,
                },
            },
            max_output_tokens: max_tokens,
        }
    }

    /// Build the analysis prompt incorporating all available context
    fn build_analysis_prompt(context: &ImageContext) -> String {
        let mut prompt = String::from("Analyze this photograph and provide descriptive keywords and alt-text.\n\n");

        // Check if we have any context to add
        let has_context = context.title.is_some()
            || context.description.is_some()
            || context.folder_title.is_some()
            || context.location.is_some()
            || context.focal_length.is_some()
            || context.aperture.is_some()
            || context.capture_date.is_some();

        if has_context {
            prompt.push_str("CONTEXT INFORMATION (for your reference only - do not repeat verbatim):\n");
        }

        if let Some(ref folder_title) = context.folder_title {
            prompt.push_str(&format!("- Album/Collection: {}\n", folder_title));
        }

        if let Some(ref title) = context.title {
            prompt.push_str(&format!("- Image title: {}\n", title));
        }

        if let Some(ref description) = context.description {
            // Strip HTML tags for cleaner context
            let clean_desc = description
                .replace("<p>", "")
                .replace("</p>", " ")
                .replace("<br>", " ")
                .replace("<br/>", " ")
                .trim()
                .to_string();
            if !clean_desc.is_empty() {
                prompt.push_str(&format!("- Description: {}\n", clean_desc));
            }
        }

        if let Some(ref capture_date) = context.capture_date {
            prompt.push_str(&format!("- Capture date: {}\n", capture_date));
        }

        // Camera settings
        if context.focal_length.is_some() || context.aperture.is_some() {
            let mut camera_info = Vec::new();
            if let Some(ref fl) = context.focal_length {
                camera_info.push(fl.clone());
            }
            if let Some(ref ap) = context.aperture {
                camera_info.push(ap.clone());
            }
            prompt.push_str(&format!("- Camera settings: {}\n", camera_info.join(", ")));
        }

        if let Some(ref loc) = context.location {
            prompt.push_str(&format!(
                "- GPS coordinates: {:.6}, {:.6} (use to identify location, but never include coordinates in output)\n",
                loc.latitude, loc.longitude
            ));
        }

        if has_context {
            prompt.push_str("\nIMPORTANT RULES:\n\
                - Use this context to inform your analysis, but describe what you actually see\n\
                - NEVER repeat exact values like GPS coordinates, dates, or camera settings in your output\n\
                - If you can identify the specific location from coordinates, use the location NAME only\n\
                - If you cannot identify a specific location, describe the general scene and environment\n\n");
        }

        prompt.push_str(
            "Generate 8-12 relevant keywords covering:\n\
            1) Key subjects and elements in the scene\n\
            2) Photo composition (e.g., wide angle, leading lines, rule of thirds, symmetry)\n\
            3) Lighting and mood (e.g., golden hour, overcast, dramatic shadows)\n\
            4) Photography style if notable (e.g., landscape, architectural, long exposure)\n"
        );

        if context.location.is_some() {
            prompt.push_str("5) Location name if identifiable (NOT coordinates)\n");
        }

        prompt.push_str("\nProvide a 1-2 sentence alt-text describing the scene and photographic style. \
            Focus on what is visually depicted, not technical metadata.");

        prompt
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_request_serialization() {
        let request = OpenAIRequest::new_image_analysis("gpt-5.2", "dGVzdA==", 300);
        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("gpt-5.2"));
        assert!(json.contains("input_text"));
        assert!(json.contains("input_image"));
    }

    #[test]
    fn test_analysis_output_deserialization() {
        let json = r#"{
            "keywords": ["sunset", "beach", "ocean"],
            "alt_text": "A beautiful sunset over the ocean"
        }"#;

        let output: AnalysisOutput = serde_json::from_str(json).unwrap();
        assert_eq!(output.keywords.len(), 3);
        assert_eq!(output.alt_text, "A beautiful sunset over the ocean");
    }
}
