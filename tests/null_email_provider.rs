use tenrankai::email::{create_provider, EmailBody, EmailConfig, EmailMessage, EmailProviderConfig};

#[tokio::test]
async fn test_null_provider_integration() {
    // Create a null email configuration
    let email_config = EmailConfig {
        from_address: "noreply@example.com".to_string(),
        from_name: Some("Test App".to_string()),
        reply_to: Some("support@example.com".to_string()),
        provider: EmailProviderConfig::Null,
    };
    
    // Create the provider
    let provider = create_provider(&email_config.provider).await.unwrap();
    
    // Create and send a test email
    let message = EmailMessage {
        to: vec!["user@example.com".to_string()],
        from: email_config.format_from(),
        subject: "Test Email from Null Provider".to_string(),
        body: EmailBody::Text("This is a test email that will only be logged.".to_string()),
        reply_to: email_config.reply_to.clone(),
    };
    
    // This should succeed and log the email
    let result = provider.send_email(message).await;
    assert!(result.is_ok());
    
    println!("Email logged successfully by {}", provider.name());
}

#[tokio::test]
async fn test_null_provider_with_login_email() {
    // Simulate sending a login email with the null provider
    let email_config = EmailConfig {
        from_address: "auth@tenrankai.app".to_string(),
        from_name: Some("Tenrankai Gallery".to_string()),
        reply_to: None,
        provider: EmailProviderConfig::Null,
    };
    
    let provider = create_provider(&email_config.provider).await.unwrap();
    
    // Create a login email similar to what the app would send
    let login_token = "abc123def456";
    let login_url = format!("http://localhost:8080/_login/verify?token={}", login_token);
    
    let html_body = format!(
        r#"<html>
<body>
    <h2>Login to Tenrankai Gallery</h2>
    <p>Click the link below to login:</p>
    <p><a href="{}" style="background-color: #007bff; color: white; padding: 10px 20px; text-decoration: none; border-radius: 5px;">Login to Gallery</a></p>
    <p>Or copy and paste this URL into your browser:</p>
    <p>{}</p>
    <p>This link will expire in 15 minutes.</p>
</body>
</html>"#,
        login_url, login_url
    );
    
    let text_body = format!(
        "Login to Tenrankai Gallery\n\n\
         Click the link below to login:\n\
         {}\n\n\
         This link will expire in 15 minutes.",
        login_url
    );
    
    let message = EmailMessage {
        to: vec!["photographer@example.com".to_string()],
        from: email_config.format_from(),
        subject: "Login to Tenrankai Gallery".to_string(),
        body: EmailBody::Both {
            text: text_body,
            html: html_body,
        },
        reply_to: None,
    };
    
    let result = provider.send_email(message).await;
    assert!(result.is_ok());
    
    println!("Login email logged successfully");
}