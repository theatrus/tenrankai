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

impl OpenAIRequest {
    /// Create a new image analysis request
    pub fn new_image_analysis(model: &str, base64_image: &str, max_tokens: u32) -> Self {
        let schema = serde_json::json!({
            "type": "object",
            "properties": {
                "keywords": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "5-10 descriptive keywords for the image"
                },
                "alt_text": {
                    "type": "string",
                    "description": "1-2 sentence description suitable for screen readers"
                }
            },
            "required": ["keywords", "alt_text"],
            "additionalProperties": false
        });

        Self {
            model: model.to_string(),
            input: vec![InputMessage {
                role: "user".to_string(),
                content: vec![
                    InputContent::Text {
                        text: "Analyze this image and provide descriptive keywords and alt-text for accessibility. Generate 5-10 relevant keywords that describe the image content, mood, and key elements. Also provide a concise 1-2 sentence alt-text description suitable for screen readers.".to_string(),
                    },
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
