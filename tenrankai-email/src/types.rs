use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmailMessage {
    pub to: Vec<String>,
    pub from: String,
    pub subject: String,
    pub body: EmailBody,
    pub reply_to: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EmailBody {
    Text(String),
    Html(String),
    Both { text: String, html: String },
}

impl EmailMessage {
    pub fn new(to: impl Into<String>, from: impl Into<String>, subject: impl Into<String>) -> Self {
        Self {
            to: vec![to.into()],
            from: from.into(),
            subject: subject.into(),
            body: EmailBody::Text(String::new()),
            reply_to: None,
        }
    }

    pub fn with_text(mut self, text: impl Into<String>) -> Self {
        self.body = EmailBody::Text(text.into());
        self
    }

    pub fn with_html(mut self, html: impl Into<String>) -> Self {
        self.body = EmailBody::Html(html.into());
        self
    }

    pub fn with_both(mut self, text: impl Into<String>, html: impl Into<String>) -> Self {
        self.body = EmailBody::Both {
            text: text.into(),
            html: html.into(),
        };
        self
    }

    pub fn with_reply_to(mut self, reply_to: impl Into<String>) -> Self {
        self.reply_to = Some(reply_to.into());
        self
    }
}
