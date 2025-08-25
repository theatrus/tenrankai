use async_trait::async_trait;
use aws_config::BehaviorVersion;
use aws_sdk_sesv2::{
    Client,
    config::{Credentials, Region},
    types::{Body, Content, Destination, EmailContent, Message},
};
use tracing::{debug, error};

use crate::email::{EmailBody, EmailError, EmailMessage, EmailProvider, SesConfig};

pub struct SesProvider {
    client: Client,
}

impl SesProvider {
    pub async fn new(config: &SesConfig) -> Result<Self, EmailError> {
        let mut aws_config_builder = aws_config::defaults(BehaviorVersion::latest());
        
        // Set region if provided, otherwise use default from environment
        if let Some(region) = &config.region {
            aws_config_builder = aws_config_builder.region(Region::new(region.clone()));
        }

        // If credentials are provided, use them. Otherwise, use the default provider chain
        if let (Some(access_key), Some(secret_key)) =
            (&config.access_key_id, &config.secret_access_key)
        {
            let credentials = Credentials::new(
                access_key,
                secret_key,
                None,
                None,
                "tenrankai-ses-provider"
            );
            aws_config_builder = aws_config_builder.credentials_provider(credentials);
        }

        let aws_config = aws_config_builder.load().await;
        let client = Client::new(&aws_config);

        Ok(Self { client })
    }
}

#[async_trait]
impl EmailProvider for SesProvider {
    async fn send_email(&self, message: EmailMessage) -> Result<(), EmailError> {
        debug!("Sending email via SES to: {:?}", message.to);

        // Build destination
        let destination = Destination::builder()
            .set_to_addresses(Some(message.to.clone()))
            .build();

        // Build email content based on body type
        let body_builder = match &message.body {
            EmailBody::Text(text) => Body::builder().text(
                Content::builder()
                    .data(text)
                    .charset("UTF-8")
                    .build()
                    .map_err(|e| EmailError::ProviderError(e.to_string()))?,
            ),
            EmailBody::Html(html) => Body::builder().html(
                Content::builder()
                    .data(html)
                    .charset("UTF-8")
                    .build()
                    .map_err(|e| EmailError::ProviderError(e.to_string()))?,
            ),
            EmailBody::Both { text, html } => Body::builder()
                .text(
                    Content::builder()
                        .data(text)
                        .charset("UTF-8")
                        .build()
                        .map_err(|e| EmailError::ProviderError(e.to_string()))?,
                )
                .html(
                    Content::builder()
                        .data(html)
                        .charset("UTF-8")
                        .build()
                        .map_err(|e| EmailError::ProviderError(e.to_string()))?,
                ),
        };

        let body = body_builder.build();

        let subject = Content::builder()
            .data(&message.subject)
            .charset("UTF-8")
            .build()
            .map_err(|e| EmailError::ProviderError(e.to_string()))?;

        let email_message = Message::builder().subject(subject).body(body).build();

        let content = EmailContent::builder().simple(email_message).build();

        // Build and send the email
        let mut send_email_builder = self
            .client
            .send_email()
            .from_email_address(&message.from)
            .destination(destination)
            .content(content);

        if let Some(reply_to) = &message.reply_to {
            send_email_builder = send_email_builder.reply_to_addresses(reply_to);
        }

        match send_email_builder.send().await {
            Ok(output) => {
                debug!(
                    "Email sent successfully. Message ID: {:?}",
                    output.message_id()
                );
                Ok(())
            }
            Err(e) => {
                error!("Failed to send email via SES: {}", e);
                Err(EmailError::AwsError(e.to_string()))
            }
        }
    }

    fn name(&self) -> &str {
        "Amazon SES"
    }
}
