use crate::ConfigStorageError;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tenrankai_storage::StorageUrl;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ConfigStorageUrl {
    FileDir { path: PathBuf },
    Storage { url: StorageUrl },
}

impl ConfigStorageUrl {
    pub fn parse(s: &str) -> Result<Self, ConfigStorageError> {
        let s = s.trim();

        if s.starts_with("s3://") {
            let url = StorageUrl::parse(s)
                .map_err(|e| ConfigStorageError::InvalidUrl(format!("Invalid S3 URL: {}", e)))?;
            return Ok(ConfigStorageUrl::Storage { url });
        }

        if s.starts_with("file://") {
            let path = s
                .strip_prefix("file://")
                .ok_or_else(|| ConfigStorageError::InvalidUrl("Invalid file:// URL".to_string()))?;
            return Ok(ConfigStorageUrl::FileDir {
                path: PathBuf::from(path),
            });
        }

        Ok(ConfigStorageUrl::FileDir {
            path: PathBuf::from(s),
        })
    }

    pub fn backend_name(&self) -> &'static str {
        match self {
            ConfigStorageUrl::FileDir { .. } => "file_dir",
            ConfigStorageUrl::Storage { .. } => "storage",
        }
    }
}

impl std::fmt::Display for ConfigStorageUrl {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigStorageUrl::FileDir { path } => write!(f, "{}", path.display()),
            ConfigStorageUrl::Storage { url } => write!(f, "{}", url),
        }
    }
}

impl std::str::FromStr for ConfigStorageUrl {
    type Err = ConfigStorageError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_relative_path() {
        let url = ConfigStorageUrl::parse("config.d").unwrap();
        assert!(matches!(url, ConfigStorageUrl::FileDir { path } if path == PathBuf::from("config.d")));
    }

    #[test]
    fn test_parse_absolute_path() {
        let url = ConfigStorageUrl::parse("/var/lib/tenrankai/config.d").unwrap();
        assert!(matches!(url, ConfigStorageUrl::FileDir { path } if path == PathBuf::from("/var/lib/tenrankai/config.d")));
    }

    #[test]
    fn test_parse_file_url() {
        let url = ConfigStorageUrl::parse("file:///var/lib/config.d").unwrap();
        assert!(matches!(url, ConfigStorageUrl::FileDir { path } if path == PathBuf::from("/var/lib/config.d")));
    }

    #[test]
    fn test_parse_s3_url() {
        let url = ConfigStorageUrl::parse("s3://mybucket/config?region=us-west-2").unwrap();
        assert!(matches!(url, ConfigStorageUrl::Storage { .. }));
    }
}
