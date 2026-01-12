use super::{
    config::OpenAIConfig,
    error::OpenAIError,
    types::{AnalysisOutput, ImageAnalysisResult, OpenAIRequest, OpenAIResponse, OutputContent},
};
use base64::{Engine as _, engine::general_purpose};
use chrono::Utc;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;
use tracing::{debug, info, warn};

const OPENAI_API_URL: &str = "https://api.openai.com/v1/responses";

/// OpenAI Vision API client for image analysis
pub struct OpenAIClient {
    config: OpenAIConfig,
    http_client: reqwest::Client,
    last_request_time: Arc<Mutex<Option<Instant>>>,
}

impl OpenAIClient {
    /// Create a new OpenAI client
    pub fn new(config: OpenAIConfig) -> Result<Self, OpenAIError> {
        if config.api_key.is_empty() {
            return Err(OpenAIError::InvalidApiKey);
        }

        let http_client = reqwest::Client::builder()
            .timeout(Duration::from_secs(60))
            .build()?;

        Ok(Self {
            config,
            http_client,
            last_request_time: Arc::new(Mutex::new(None)),
        })
    }

    /// Analyze an image and return keywords and alt-text
    pub async fn analyze_image(
        &self,
        image_path: &Path,
    ) -> Result<ImageAnalysisResult, OpenAIError> {
        // Read and encode the image
        let image_data = tokio::fs::read(image_path)
            .await
            .map_err(|e| OpenAIError::ImageNotFound(format!("{}: {}", image_path.display(), e)))?;

        let base64_image = general_purpose::STANDARD.encode(&image_data);

        // Apply rate limiting
        self.wait_for_rate_limit().await;

        // Build and send request
        let request = OpenAIRequest::new_image_analysis(
            &self.config.model,
            &base64_image,
            self.config.max_tokens,
        );

        debug!(
            "Sending image analysis request for: {}",
            image_path.display()
        );

        let response = self
            .http_client
            .post(OPENAI_API_URL)
            .header("Authorization", format!("Bearer {}", self.config.api_key))
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await?;

        // Update last request time
        {
            let mut last_time = self.last_request_time.lock().await;
            *last_time = Some(Instant::now());
        }

        // Handle response
        let status = response.status();
        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            let retry_after = response
                .headers()
                .get("retry-after")
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.parse::<u64>().ok())
                .unwrap_or(60);

            return Err(OpenAIError::RateLimited {
                retry_after_ms: retry_after * 1000,
            });
        }

        if status == reqwest::StatusCode::UNAUTHORIZED {
            return Err(OpenAIError::InvalidApiKey);
        }

        if !status.is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(OpenAIError::ApiError(format!(
                "HTTP {}: {}",
                status, error_text
            )));
        }

        let api_response: OpenAIResponse = response
            .json()
            .await
            .map_err(|e| OpenAIError::ResponseParseError(e.to_string()))?;

        // Extract the analysis from the response
        let analysis = self.parse_analysis_response(api_response)?;

        info!(
            "Analyzed image {}: {} keywords",
            image_path.display(),
            analysis.keywords.len()
        );

        Ok(ImageAnalysisResult {
            keywords: analysis.keywords,
            alt_text: analysis.alt_text,
            analyzed_at: Utc::now(),
        })
    }

    /// Wait for rate limit if needed
    async fn wait_for_rate_limit(&self) {
        let last_time = self.last_request_time.lock().await;
        if let Some(last) = *last_time {
            let elapsed = last.elapsed();
            let rate_limit = Duration::from_millis(self.config.rate_limit_ms);
            if elapsed < rate_limit {
                let wait_time = rate_limit - elapsed;
                drop(last_time); // Release lock before sleeping
                debug!("Rate limiting: waiting {:?}", wait_time);
                tokio::time::sleep(wait_time).await;
            }
        }
    }

    /// Parse the structured analysis response
    fn parse_analysis_response(
        &self,
        response: OpenAIResponse,
    ) -> Result<AnalysisOutput, OpenAIError> {
        for output in response.output {
            for content in output.content {
                if let OutputContent::Text { text } = content {
                    let analysis: AnalysisOutput = serde_json::from_str(&text).map_err(|e| {
                        OpenAIError::ResponseParseError(format!(
                            "Failed to parse analysis JSON: {}",
                            e
                        ))
                    })?;
                    return Ok(analysis);
                }
            }
        }

        Err(OpenAIError::ResponseParseError(
            "No text output found in response".to_string(),
        ))
    }

    /// Analyze an image from base64 data (for pre-resized images)
    pub async fn analyze_image_data(
        &self,
        base64_image: &str,
        image_name: &str,
    ) -> Result<ImageAnalysisResult, OpenAIError> {
        // Apply rate limiting
        self.wait_for_rate_limit().await;

        // Build and send request
        let request = OpenAIRequest::new_image_analysis(
            &self.config.model,
            base64_image,
            self.config.max_tokens,
        );

        debug!("Sending image analysis request for: {}", image_name);

        let response = self
            .http_client
            .post(OPENAI_API_URL)
            .header("Authorization", format!("Bearer {}", self.config.api_key))
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await?;

        // Update last request time
        {
            let mut last_time = self.last_request_time.lock().await;
            *last_time = Some(Instant::now());
        }

        // Handle response
        let status = response.status();
        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            let retry_after = response
                .headers()
                .get("retry-after")
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.parse::<u64>().ok())
                .unwrap_or(60);

            warn!("Rate limited by OpenAI API, retry after {}s", retry_after);
            return Err(OpenAIError::RateLimited {
                retry_after_ms: retry_after * 1000,
            });
        }

        if status == reqwest::StatusCode::UNAUTHORIZED {
            return Err(OpenAIError::InvalidApiKey);
        }

        if !status.is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(OpenAIError::ApiError(format!(
                "HTTP {}: {}",
                status, error_text
            )));
        }

        let api_response: OpenAIResponse = response
            .json()
            .await
            .map_err(|e| OpenAIError::ResponseParseError(e.to_string()))?;

        // Extract the analysis from the response
        let analysis = self.parse_analysis_response(api_response)?;

        info!(
            "Analyzed image {}: {} keywords",
            image_name,
            analysis.keywords.len()
        );

        Ok(ImageAnalysisResult {
            keywords: analysis.keywords,
            alt_text: analysis.alt_text,
            analyzed_at: Utc::now(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_client_requires_api_key() {
        let config = OpenAIConfig::default();
        let result = OpenAIClient::new(config);
        assert!(matches!(result, Err(OpenAIError::InvalidApiKey)));
    }

    #[test]
    fn test_client_creation_with_key() {
        let config = OpenAIConfig {
            api_key: "sk-test-key".to_string(),
            ..Default::default()
        };
        let result = OpenAIClient::new(config);
        assert!(result.is_ok());
    }
}
