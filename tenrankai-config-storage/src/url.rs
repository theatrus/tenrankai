use crate::ConfigStorageError;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tenrankai_storage::StorageUrl;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ConfigStorageUrl {
    FileDir {
        path: PathBuf,
    },
    Storage {
        url: StorageUrl,
    },
    DynamoDb {
        table_name: String,
        region: Option<String>,
        endpoint: Option<String>,
        shard: Option<String>,
    },
}

impl ConfigStorageUrl {
    pub fn parse(s: &str) -> Result<Self, ConfigStorageError> {
        let s = s.trim();

        if s.starts_with("s3://") {
            let url = StorageUrl::parse(s)
                .map_err(|e| ConfigStorageError::InvalidUrl(format!("Invalid S3 URL: {}", e)))?;
            return Ok(ConfigStorageUrl::Storage { url });
        }

        if s.starts_with("dynamodb://") {
            return Self::parse_dynamodb(s);
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

    fn parse_dynamodb(s: &str) -> Result<Self, ConfigStorageError> {
        let rest = s
            .strip_prefix("dynamodb://")
            .ok_or_else(|| ConfigStorageError::InvalidUrl("Invalid dynamodb:// URL".to_string()))?;

        let (host_part, query_part) = match rest.split_once('?') {
            Some((h, q)) => (h, Some(q)),
            None => (rest, None),
        };

        let table_name = host_part.trim_end_matches('/').to_string();
        if table_name.is_empty() {
            return Err(ConfigStorageError::InvalidUrl(
                "DynamoDB URL must include a table name".to_string(),
            ));
        }

        let mut region = None;
        let mut endpoint = None;
        let mut shard = None;

        if let Some(query) = query_part {
            for param in query.split('&') {
                if let Some((key, value)) = param.split_once('=') {
                    match key {
                        "region" => region = Some(value.to_string()),
                        "endpoint" => {
                            endpoint = Some(
                                urlencoding::decode(value)
                                    .map_err(|e| {
                                        ConfigStorageError::InvalidUrl(format!(
                                            "Invalid endpoint encoding: {}",
                                            e
                                        ))
                                    })?
                                    .into_owned(),
                            );
                        }
                        "shard" => shard = Some(value.to_string()),
                        _ => {}
                    }
                }
            }
        }

        Ok(ConfigStorageUrl::DynamoDb {
            table_name,
            region,
            endpoint,
            shard,
        })
    }

    pub fn backend_name(&self) -> &'static str {
        match self {
            ConfigStorageUrl::FileDir { .. } => "file_dir",
            ConfigStorageUrl::Storage { .. } => "storage",
            ConfigStorageUrl::DynamoDb { .. } => "dynamodb",
        }
    }
}

impl std::fmt::Display for ConfigStorageUrl {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigStorageUrl::FileDir { path } => write!(f, "{}", path.display()),
            ConfigStorageUrl::Storage { url } => write!(f, "{}", url),
            ConfigStorageUrl::DynamoDb {
                table_name,
                region,
                endpoint,
                shard,
            } => {
                write!(f, "dynamodb://{}", table_name)?;
                let mut first = true;
                if let Some(r) = region {
                    write!(f, "?region={}", r)?;
                    first = false;
                }
                if let Some(e) = endpoint {
                    if first {
                        write!(f, "?")?;
                    } else {
                        write!(f, "&")?;
                    }
                    write!(f, "endpoint={}", urlencoding::encode(e))?;
                    first = false;
                }
                if let Some(s) = shard {
                    if first {
                        write!(f, "?")?;
                    } else {
                        write!(f, "&")?;
                    }
                    write!(f, "shard={}", s)?;
                }
                Ok(())
            }
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
        assert!(
            matches!(url, ConfigStorageUrl::FileDir { path } if path == PathBuf::from("config.d"))
        );
    }

    #[test]
    fn test_parse_absolute_path() {
        let url = ConfigStorageUrl::parse("/var/lib/tenrankai/config.d").unwrap();
        assert!(
            matches!(url, ConfigStorageUrl::FileDir { path } if path == PathBuf::from("/var/lib/tenrankai/config.d"))
        );
    }

    #[test]
    fn test_parse_file_url() {
        let url = ConfigStorageUrl::parse("file:///var/lib/config.d").unwrap();
        assert!(
            matches!(url, ConfigStorageUrl::FileDir { path } if path == PathBuf::from("/var/lib/config.d"))
        );
    }

    #[test]
    fn test_parse_s3_url() {
        let url = ConfigStorageUrl::parse("s3://mybucket/config?region=us-west-2").unwrap();
        assert!(matches!(url, ConfigStorageUrl::Storage { .. }));
    }

    #[test]
    fn test_parse_dynamodb_url() {
        let url = ConfigStorageUrl::parse("dynamodb://my-table?region=us-west-2").unwrap();
        match url {
            ConfigStorageUrl::DynamoDb {
                table_name,
                region,
                endpoint,
                shard,
            } => {
                assert_eq!(table_name, "my-table");
                assert_eq!(region.as_deref(), Some("us-west-2"));
                assert!(endpoint.is_none());
                assert!(shard.is_none());
            }
            _ => panic!("Expected DynamoDb variant"),
        }
    }

    #[test]
    fn test_parse_dynamodb_url_with_endpoint() {
        let url = ConfigStorageUrl::parse(
            "dynamodb://my-table?region=us-west-2&endpoint=http%3A%2F%2Flocalhost%3A8000",
        )
        .unwrap();
        match url {
            ConfigStorageUrl::DynamoDb {
                table_name,
                region,
                endpoint,
                shard,
            } => {
                assert_eq!(table_name, "my-table");
                assert_eq!(region.as_deref(), Some("us-west-2"));
                assert_eq!(endpoint.as_deref(), Some("http://localhost:8000"));
                assert!(shard.is_none());
            }
            _ => panic!("Expected DynamoDb variant"),
        }
    }

    #[test]
    fn test_parse_dynamodb_url_no_params() {
        let url = ConfigStorageUrl::parse("dynamodb://config-table").unwrap();
        match url {
            ConfigStorageUrl::DynamoDb {
                table_name,
                region,
                endpoint,
                shard,
            } => {
                assert_eq!(table_name, "config-table");
                assert!(region.is_none());
                assert!(endpoint.is_none());
                assert!(shard.is_none());
            }
            _ => panic!("Expected DynamoDb variant"),
        }
    }

    #[test]
    fn test_parse_dynamodb_url_with_shard() {
        let url =
            ConfigStorageUrl::parse("dynamodb://my-table?region=us-west-2&shard=prod").unwrap();
        match url {
            ConfigStorageUrl::DynamoDb {
                table_name,
                region,
                shard,
                ..
            } => {
                assert_eq!(table_name, "my-table");
                assert_eq!(region.as_deref(), Some("us-west-2"));
                assert_eq!(shard.as_deref(), Some("prod"));
            }
            _ => panic!("Expected DynamoDb variant"),
        }
    }

    #[test]
    fn test_dynamodb_url_display_with_shard() {
        let url =
            ConfigStorageUrl::parse("dynamodb://my-table?region=us-west-2&shard=prod").unwrap();
        assert_eq!(
            url.to_string(),
            "dynamodb://my-table?region=us-west-2&shard=prod"
        );
    }

    #[test]
    fn test_parse_dynamodb_url_empty_table_name() {
        let result = ConfigStorageUrl::parse("dynamodb://");
        assert!(result.is_err());
    }

    #[test]
    fn test_dynamodb_url_display_roundtrip() {
        let url = ConfigStorageUrl::parse("dynamodb://my-table?region=us-west-2").unwrap();
        let display = url.to_string();
        assert_eq!(display, "dynamodb://my-table?region=us-west-2");
    }

    #[test]
    fn test_dynamodb_backend_name() {
        let url = ConfigStorageUrl::parse("dynamodb://my-table").unwrap();
        assert_eq!(url.backend_name(), "dynamodb");
    }
}
