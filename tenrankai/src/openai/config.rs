use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAIConfig {
    /// OpenAI API key (required)
    pub api_key: String,

    /// Model for vision analysis (default: "gpt-4.1-mini")
    #[serde(default = "default_model")]
    pub model: String,

    /// Delay between API calls in milliseconds (default: 1000)
    #[serde(default = "default_rate_limit_ms")]
    pub rate_limit_ms: u64,

    /// Maximum tokens for response (default: 300)
    #[serde(default = "default_max_tokens")]
    pub max_tokens: u32,

    /// Enable automatic background analysis (default: false)
    #[serde(default)]
    pub enable_background_analysis: bool,

    /// Interval for background runs in minutes (default: 60)
    #[serde(default = "default_background_interval")]
    pub background_interval_minutes: u64,

    /// Max images per background run (default: 50)
    #[serde(default = "default_batch_size")]
    pub background_batch_size: usize,
}

fn default_model() -> String {
    "gpt-5.2".to_string()
}

fn default_rate_limit_ms() -> u64 {
    1000
}

fn default_max_tokens() -> u32 {
    300
}

fn default_background_interval() -> u64 {
    60
}

fn default_batch_size() -> usize {
    50
}

impl Default for OpenAIConfig {
    fn default() -> Self {
        Self {
            api_key: String::new(),
            model: default_model(),
            rate_limit_ms: default_rate_limit_ms(),
            max_tokens: default_max_tokens(),
            enable_background_analysis: false,
            background_interval_minutes: default_background_interval(),
            background_batch_size: default_batch_size(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = OpenAIConfig::default();
        assert_eq!(config.model, "gpt-5.2");
        assert_eq!(config.rate_limit_ms, 1000);
        assert_eq!(config.max_tokens, 300);
        assert!(!config.enable_background_analysis);
    }

    #[test]
    fn test_config_deserialization() {
        let toml = r#"
            api_key = "sk-test-key"
            model = "gpt-4.1"
            rate_limit_ms = 2000
        "#;

        let config: OpenAIConfig = toml_edit::de::from_str(toml).unwrap();
        assert_eq!(config.api_key, "sk-test-key");
        assert_eq!(config.model, "gpt-4.1");
        assert_eq!(config.rate_limit_ms, 2000);
        assert_eq!(config.max_tokens, 300); // default
    }
}
