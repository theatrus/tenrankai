use crate::email::{EmailBody, EmailError, EmailMessage, EmailProvider};
use async_trait::async_trait;
use tracing::info;

pub struct NullProvider;

impl NullProvider {
    pub fn new() -> Self {
        Self
    }
}

impl Default for NullProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl EmailProvider for NullProvider {
    async fn send_email(&self, message: EmailMessage) -> Result<(), EmailError> {
        // Format the recipients
        let recipients = message.to.join(", ");

        // Extract body content
        let body_preview = match &message.body {
            EmailBody::Text(text) => text.chars().take(200).collect::<String>(),
            EmailBody::Html(html) => html.chars().take(200).collect::<String>(),
            EmailBody::Both { text, .. } => text.chars().take(200).collect::<String>(),
        };

        // Log the email that would have been sent
        info!(
            "NULL EMAIL PROVIDER - Would send email:\n\
             From: {}\n\
             To: {}\n\
             Reply-To: {}\n\
             Subject: {}\n\
             Body (first 200 chars): {}{}",
            message.from,
            recipients,
            message.reply_to.as_deref().unwrap_or("(none)"),
            message.subject,
            body_preview,
            if body_preview.len() >= 200 { "..." } else { "" }
        );

        // For debugging, also log the full message at debug level
        let full_body = match &message.body {
            EmailBody::Text(text) => format!("Text:\n{}", text),
            EmailBody::Html(html) => format!("HTML:\n{}", html),
            EmailBody::Both { text, html } => format!("Text:\n{}\n\nHTML:\n{}", text, html),
        };

        tracing::debug!(
            "NULL EMAIL PROVIDER - Full email message:\n\
             From: {}\n\
             To: {:?}\n\
             Reply-To: {:?}\n\
             Subject: {}\n\
             Body:\n{}",
            message.from,
            message.to,
            message.reply_to,
            message.subject,
            full_body
        );

        Ok(())
    }

    fn name(&self) -> &str {
        "Null Email Provider (Logging Only)"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_null_provider_send() {
        let provider = NullProvider::new();
        let message = EmailMessage {
            to: vec!["test@example.com".to_string()],
            from: "sender@example.com".to_string(),
            subject: "Test Subject".to_string(),
            body: EmailBody::Text("Test body content".to_string()),
            reply_to: Some("reply@example.com".to_string()),
        };

        // Should always succeed
        let result = provider.send_email(message).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_null_provider_send_html() {
        let provider = NullProvider::new();
        let message = EmailMessage {
            to: vec![
                "test1@example.com".to_string(),
                "test2@example.com".to_string(),
            ],
            from: "sender@example.com".to_string(),
            subject: "HTML Test".to_string(),
            body: EmailBody::Html("<h1>Test HTML</h1>".to_string()),
            reply_to: None,
        };

        let result = provider.send_email(message).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_null_provider_send_both() {
        let provider = NullProvider::new();
        let message = EmailMessage {
            to: vec!["test@example.com".to_string()],
            from: "sender@example.com".to_string(),
            subject: "Multi-part Test".to_string(),
            body: EmailBody::Both {
                text: "Plain text version".to_string(),
                html: "<p>HTML version</p>".to_string(),
            },
            reply_to: None,
        };

        let result = provider.send_email(message).await;
        assert!(result.is_ok());
    }

    #[test]
    fn test_null_provider_name() {
        let provider = NullProvider::new();
        assert_eq!(provider.name(), "Null Email Provider (Logging Only)");
    }
}
