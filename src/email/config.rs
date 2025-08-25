use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct EmailConfig {
    pub from_address: String,
    pub from_name: Option<String>,
    pub reply_to: Option<String>,
    #[serde(flatten)]
    pub provider: EmailProviderConfig,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "provider", rename_all = "lowercase")]
pub enum EmailProviderConfig {
    Ses(SesConfig),
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SesConfig {
    pub region: Option<String>,
    pub access_key_id: Option<String>,
    pub secret_access_key: Option<String>,
}

impl EmailConfig {
    pub fn format_from(&self) -> String {
        match &self.from_name {
            Some(name) => format!("{} <{}>", name, self.from_address),
            None => self.from_address.clone(),
        }
    }
}
