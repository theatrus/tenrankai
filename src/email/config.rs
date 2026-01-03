use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct EmailConfig {
    pub from_address: String,
    pub from_name: Option<String>,
    pub reply_to: Option<String>,
    #[serde(flatten)]
    pub provider: EmailProviderConfig,
}

/// Email provider type with capability methods
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum EmailProviderType {
    Null,
    Ses,
    Smtp,
    SendGrid,
    Mailgun,
}

/// Email feature set capabilities
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmailFeatureSet {
    pub supports_html: bool,
    pub supports_attachments: bool,
    pub supports_templates: bool,
    pub supports_bulk_send: bool,
    pub supports_tracking: bool,
    pub max_recipients: Option<usize>,
}

impl EmailProviderType {
    /// Get the display name for this provider
    pub fn display_name(&self) -> &'static str {
        match self {
            EmailProviderType::Null => "Development (Null)",
            EmailProviderType::Ses => "Amazon SES",
            EmailProviderType::Smtp => "SMTP",
            EmailProviderType::SendGrid => "SendGrid",
            EmailProviderType::Mailgun => "Mailgun",
        }
    }

    /// Check if this provider requires credentials
    pub fn requires_credentials(&self) -> bool {
        match self {
            EmailProviderType::Null => false,
            EmailProviderType::Ses => true,
            EmailProviderType::Smtp => true,
            EmailProviderType::SendGrid => true,
            EmailProviderType::Mailgun => true,
        }
    }

    /// Get the supported feature set for this provider
    pub fn supported_features(&self) -> EmailFeatureSet {
        match self {
            EmailProviderType::Null => EmailFeatureSet {
                supports_html: true,
                supports_attachments: false,
                supports_templates: false,
                supports_bulk_send: false,
                supports_tracking: false,
                max_recipients: None,
            },
            EmailProviderType::Ses => EmailFeatureSet {
                supports_html: true,
                supports_attachments: true,
                supports_templates: true,
                supports_bulk_send: true,
                supports_tracking: true,
                max_recipients: Some(50), // SES v2 API limit
            },
            EmailProviderType::Smtp => EmailFeatureSet {
                supports_html: true,
                supports_attachments: true,
                supports_templates: false,
                supports_bulk_send: false,
                supports_tracking: false,
                max_recipients: Some(1),
            },
            EmailProviderType::SendGrid => EmailFeatureSet {
                supports_html: true,
                supports_attachments: true,
                supports_templates: true,
                supports_bulk_send: true,
                supports_tracking: true,
                max_recipients: Some(1000),
            },
            EmailProviderType::Mailgun => EmailFeatureSet {
                supports_html: true,
                supports_attachments: true,
                supports_templates: true,
                supports_bulk_send: true,
                supports_tracking: true,
                max_recipients: Some(1000),
            },
        }
    }

    /// Get the default timeout for this provider
    pub fn default_timeout(&self) -> Duration {
        match self {
            EmailProviderType::Null => Duration::from_secs(1),
            EmailProviderType::Ses => Duration::from_secs(30),
            EmailProviderType::Smtp => Duration::from_secs(60),
            EmailProviderType::SendGrid => Duration::from_secs(30),
            EmailProviderType::Mailgun => Duration::from_secs(30),
        }
    }

    /// Check if this provider supports rate limiting information
    pub fn provides_rate_limits(&self) -> bool {
        match self {
            EmailProviderType::Null => false,
            EmailProviderType::Ses => true,
            EmailProviderType::Smtp => false,
            EmailProviderType::SendGrid => true,
            EmailProviderType::Mailgun => true,
        }
    }

    /// Get all available provider types
    pub const ALL: &'static [EmailProviderType] = &[
        EmailProviderType::Null,
        EmailProviderType::Ses,
        EmailProviderType::Smtp,
        EmailProviderType::SendGrid,
        EmailProviderType::Mailgun,
    ];

    /// Get providers that are production-ready
    pub const PRODUCTION_READY: &'static [EmailProviderType] = &[
        EmailProviderType::Ses,
        EmailProviderType::Smtp,
        EmailProviderType::SendGrid,
        EmailProviderType::Mailgun,
    ];

    /// Parse from string
    pub fn parse(s: &str) -> Option<EmailProviderType> {
        match s.to_lowercase().as_str() {
            "null" => Some(EmailProviderType::Null),
            "ses" => Some(EmailProviderType::Ses),
            "smtp" => Some(EmailProviderType::Smtp),
            "sendgrid" => Some(EmailProviderType::SendGrid),
            "mailgun" => Some(EmailProviderType::Mailgun),
            _ => None,
        }
    }

    /// Convert to string representation
    pub fn as_str(&self) -> &'static str {
        match self {
            EmailProviderType::Null => "null",
            EmailProviderType::Ses => "ses",
            EmailProviderType::Smtp => "smtp",
            EmailProviderType::SendGrid => "sendgrid",
            EmailProviderType::Mailgun => "mailgun",
        }
    }
}

impl std::fmt::Display for EmailProviderType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.display_name())
    }
}

impl std::str::FromStr for EmailProviderType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s).ok_or_else(|| {
            format!(
                "Invalid email provider '{}'. Valid providers: {}",
                s,
                Self::ALL
                    .iter()
                    .map(|p| p.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        })
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "provider", rename_all = "lowercase")]
pub enum EmailProviderConfig {
    Null,
    Ses(SesConfig),
    Smtp(SmtpConfig),
    SendGrid(SendGridConfig),
    Mailgun(MailgunConfig),
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SmtpConfig {
    pub host: String,
    pub port: u16,
    pub username: Option<String>,
    pub password: Option<String>,
    pub use_tls: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SendGridConfig {
    pub api_key: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MailgunConfig {
    pub api_key: String,
    pub domain: String,
    pub region: Option<String>, // "us" or "eu"
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SesConfig {
    pub region: Option<String>,
    pub access_key_id: Option<String>,
    pub secret_access_key: Option<String>,
}

impl EmailProviderConfig {
    /// Get the provider type for this configuration
    pub fn provider_type(&self) -> EmailProviderType {
        match self {
            EmailProviderConfig::Null => EmailProviderType::Null,
            EmailProviderConfig::Ses(_) => EmailProviderType::Ses,
            EmailProviderConfig::Smtp(_) => EmailProviderType::Smtp,
            EmailProviderConfig::SendGrid(_) => EmailProviderType::SendGrid,
            EmailProviderConfig::Mailgun(_) => EmailProviderType::Mailgun,
        }
    }

    /// Check if this configuration has the required credentials
    pub fn has_required_credentials(&self) -> bool {
        match self {
            EmailProviderConfig::Null => true, // No credentials required
            EmailProviderConfig::Ses(config) => {
                // SES can use default AWS credentials or explicit ones
                config.access_key_id.is_some() && config.secret_access_key.is_some()
                    || std::env::var("AWS_ACCESS_KEY_ID").is_ok()
                    || std::env::var("AWS_PROFILE").is_ok()
            }
            EmailProviderConfig::Smtp(config) => {
                // SMTP may or may not require auth
                config.username.is_some() == config.password.is_some()
            }
            EmailProviderConfig::SendGrid(config) => !config.api_key.is_empty(),
            EmailProviderConfig::Mailgun(config) => {
                !config.api_key.is_empty() && !config.domain.is_empty()
            }
        }
    }
}

impl EmailConfig {
    pub fn format_from(&self) -> String {
        match &self.from_name {
            Some(name) => format!("{} <{}>", name, self.from_address),
            None => self.from_address.clone(),
        }
    }

    /// Get the provider type for this configuration
    pub fn provider_type(&self) -> EmailProviderType {
        self.provider.provider_type()
    }

    /// Get the supported features for this configuration
    pub fn supported_features(&self) -> EmailFeatureSet {
        self.provider_type().supported_features()
    }

    /// Check if this configuration is valid
    pub fn is_valid(&self) -> bool {
        !self.from_address.is_empty()
            && self.from_address.contains('@')
            && self.provider.has_required_credentials()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_email_provider_type_functionality() {
        // Test display names
        assert_eq!(EmailProviderType::Null.display_name(), "Development (Null)");
        assert_eq!(EmailProviderType::Ses.display_name(), "Amazon SES");
        assert_eq!(EmailProviderType::Smtp.display_name(), "SMTP");
        assert_eq!(EmailProviderType::SendGrid.display_name(), "SendGrid");
        assert_eq!(EmailProviderType::Mailgun.display_name(), "Mailgun");

        // Test credential requirements
        assert!(!EmailProviderType::Null.requires_credentials());
        assert!(EmailProviderType::Ses.requires_credentials());
        assert!(EmailProviderType::Smtp.requires_credentials());
        assert!(EmailProviderType::SendGrid.requires_credentials());
        assert!(EmailProviderType::Mailgun.requires_credentials());

        // Test feature sets
        let null_features = EmailProviderType::Null.supported_features();
        assert!(null_features.supports_html);
        assert!(!null_features.supports_attachments);
        assert!(!null_features.supports_templates);
        assert!(!null_features.supports_bulk_send);
        assert!(!null_features.supports_tracking);
        assert!(null_features.max_recipients.is_none());

        let ses_features = EmailProviderType::Ses.supported_features();
        assert!(ses_features.supports_html);
        assert!(ses_features.supports_attachments);
        assert!(ses_features.supports_templates);
        assert!(ses_features.supports_bulk_send);
        assert!(ses_features.supports_tracking);
        assert_eq!(ses_features.max_recipients, Some(50));

        let sendgrid_features = EmailProviderType::SendGrid.supported_features();
        assert_eq!(sendgrid_features.max_recipients, Some(1000));

        // Test timeouts
        assert!(EmailProviderType::Null.default_timeout().as_secs() > 0);
        assert!(EmailProviderType::Ses.default_timeout().as_secs() >= 30);

        // Test rate limit support
        assert!(!EmailProviderType::Null.provides_rate_limits());
        assert!(EmailProviderType::Ses.provides_rate_limits());
        assert!(!EmailProviderType::Smtp.provides_rate_limits());
        assert!(EmailProviderType::SendGrid.provides_rate_limits());
        assert!(EmailProviderType::Mailgun.provides_rate_limits());

        // Test constants
        assert_eq!(EmailProviderType::ALL.len(), 5);
        assert!(EmailProviderType::ALL.contains(&EmailProviderType::Null));
        assert!(EmailProviderType::ALL.contains(&EmailProviderType::Ses));
        assert!(EmailProviderType::ALL.contains(&EmailProviderType::Smtp));
        assert!(EmailProviderType::ALL.contains(&EmailProviderType::SendGrid));
        assert!(EmailProviderType::ALL.contains(&EmailProviderType::Mailgun));

        assert_eq!(EmailProviderType::PRODUCTION_READY.len(), 4);
        assert!(!EmailProviderType::PRODUCTION_READY.contains(&EmailProviderType::Null));
        assert!(EmailProviderType::PRODUCTION_READY.contains(&EmailProviderType::Ses));
    }

    #[test]
    fn test_email_provider_type_parsing() {
        // Test successful parsing
        assert_eq!(
            EmailProviderType::parse("null"),
            Some(EmailProviderType::Null)
        );
        assert_eq!(
            EmailProviderType::parse("NULL"),
            Some(EmailProviderType::Null)
        );
        assert_eq!(
            EmailProviderType::parse("ses"),
            Some(EmailProviderType::Ses)
        );
        assert_eq!(
            EmailProviderType::parse("SES"),
            Some(EmailProviderType::Ses)
        );
        assert_eq!(
            EmailProviderType::parse("smtp"),
            Some(EmailProviderType::Smtp)
        );
        assert_eq!(
            EmailProviderType::parse("sendgrid"),
            Some(EmailProviderType::SendGrid)
        );
        assert_eq!(
            EmailProviderType::parse("mailgun"),
            Some(EmailProviderType::Mailgun)
        );

        // Test invalid parsing
        assert_eq!(EmailProviderType::parse("invalid"), None);
        assert_eq!(EmailProviderType::parse(""), None);
        assert_eq!(EmailProviderType::parse("gmail"), None);

        // Test string conversion
        assert_eq!(EmailProviderType::Null.as_str(), "null");
        assert_eq!(EmailProviderType::Ses.as_str(), "ses");
        assert_eq!(EmailProviderType::Smtp.as_str(), "smtp");
        assert_eq!(EmailProviderType::SendGrid.as_str(), "sendgrid");
        assert_eq!(EmailProviderType::Mailgun.as_str(), "mailgun");

        // Test Display trait
        assert_eq!(format!("{}", EmailProviderType::Null), "Development (Null)");
        assert_eq!(format!("{}", EmailProviderType::Ses), "Amazon SES");

        // Test FromStr trait
        let provider: EmailProviderType = "ses".parse().unwrap();
        assert_eq!(provider, EmailProviderType::Ses);

        let result: Result<EmailProviderType, _> = "invalid".parse();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid email provider"));
    }

    #[test]
    fn test_email_provider_config_functionality() {
        // Test provider type extraction
        let null_config = EmailProviderConfig::Null;
        assert_eq!(null_config.provider_type(), EmailProviderType::Null);

        let ses_config = EmailProviderConfig::Ses(SesConfig {
            region: Some("us-east-1".to_string()),
            access_key_id: Some("key".to_string()),
            secret_access_key: Some("secret".to_string()),
        });
        assert_eq!(ses_config.provider_type(), EmailProviderType::Ses);

        // Test credential validation
        assert!(null_config.has_required_credentials());
        assert!(ses_config.has_required_credentials());

        // Test SMTP config
        let smtp_with_auth = EmailProviderConfig::Smtp(SmtpConfig {
            host: "smtp.example.com".to_string(),
            port: 587,
            username: Some("user".to_string()),
            password: Some("pass".to_string()),
            use_tls: true,
        });
        assert_eq!(smtp_with_auth.provider_type(), EmailProviderType::Smtp);
        assert!(smtp_with_auth.has_required_credentials());

        let smtp_no_auth = EmailProviderConfig::Smtp(SmtpConfig {
            host: "smtp.example.com".to_string(),
            port: 25,
            username: None,
            password: None,
            use_tls: false,
        });
        assert!(smtp_no_auth.has_required_credentials()); // No auth required is valid

        // Test SendGrid config
        let sendgrid_config = EmailProviderConfig::SendGrid(SendGridConfig {
            api_key: "sg_key".to_string(),
        });
        assert_eq!(sendgrid_config.provider_type(), EmailProviderType::SendGrid);
        assert!(sendgrid_config.has_required_credentials());

        let sendgrid_empty = EmailProviderConfig::SendGrid(SendGridConfig {
            api_key: "".to_string(),
        });
        assert!(!sendgrid_empty.has_required_credentials());

        // Test Mailgun config
        let mailgun_config = EmailProviderConfig::Mailgun(MailgunConfig {
            api_key: "mg_key".to_string(),
            domain: "example.com".to_string(),
            region: Some("us".to_string()),
        });
        assert_eq!(mailgun_config.provider_type(), EmailProviderType::Mailgun);
        assert!(mailgun_config.has_required_credentials());

        let mailgun_incomplete = EmailProviderConfig::Mailgun(MailgunConfig {
            api_key: "mg_key".to_string(),
            domain: "".to_string(),
            region: None,
        });
        assert!(!mailgun_incomplete.has_required_credentials());
    }

    #[test]
    fn test_email_config_functionality() {
        let config = EmailConfig {
            from_address: "noreply@example.com".to_string(),
            from_name: Some("Test App".to_string()),
            reply_to: Some("support@example.com".to_string()),
            provider: EmailProviderConfig::Null,
        };

        // Test from formatting
        assert_eq!(config.format_from(), "Test App <noreply@example.com>");

        let config_no_name = EmailConfig {
            from_address: "noreply@example.com".to_string(),
            from_name: None,
            reply_to: None,
            provider: EmailProviderConfig::Null,
        };
        assert_eq!(config_no_name.format_from(), "noreply@example.com");

        // Test provider type delegation
        assert_eq!(config.provider_type(), EmailProviderType::Null);

        // Test feature delegation
        let features = config.supported_features();
        assert!(features.supports_html);
        assert!(!features.supports_attachments);

        // Test validation
        assert!(config.is_valid());

        let invalid_config = EmailConfig {
            from_address: "invalid-email".to_string(),
            from_name: None,
            reply_to: None,
            provider: EmailProviderConfig::Null,
        };
        assert!(!invalid_config.is_valid());

        let empty_config = EmailConfig {
            from_address: "".to_string(),
            from_name: None,
            reply_to: None,
            provider: EmailProviderConfig::Null,
        };
        assert!(!empty_config.is_valid());
    }

    #[test]
    fn test_email_feature_set() {
        let basic_features = EmailFeatureSet {
            supports_html: true,
            supports_attachments: false,
            supports_templates: false,
            supports_bulk_send: false,
            supports_tracking: false,
            max_recipients: Some(1),
        };

        let advanced_features = EmailFeatureSet {
            supports_html: true,
            supports_attachments: true,
            supports_templates: true,
            supports_bulk_send: true,
            supports_tracking: true,
            max_recipients: Some(1000),
        };

        // Test equality
        assert_ne!(basic_features, advanced_features);
        assert_eq!(basic_features.clone(), basic_features);

        // Test feature availability
        assert!(basic_features.supports_html);
        assert!(!basic_features.supports_attachments);
        assert!(advanced_features.supports_templates);
    }
}
