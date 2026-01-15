//! URL parsing for user storage backends.
//!
//! Supports various URL formats:
//! - `users.toml` - relative TOML file path
//! - `/absolute/path/users.toml` - absolute TOML file path
//! - `file:///path/to/users.toml` - file:// URL scheme
//! - `sqlite:///path/to/users.db` - SQLite database
//! - `postgresql://user:pass@host/db` - PostgreSQL database
//! - `dynamodb://table-name?region=us-west-2` - DynamoDB table

use super::DynUserStorage;
use super::error::UserStorageError;
#[cfg(feature = "user-storage-sql")]
use super::sql::SqlUserStorage;
use super::toml_backend::TomlUserStorage;
use std::path::PathBuf;
use std::sync::Arc;

/// Parsed user storage URL.
#[derive(Debug, Clone)]
pub enum UserStorageUrl {
    /// TOML file backend.
    Toml { path: PathBuf },

    /// SQLite database backend.
    #[allow(dead_code)]
    Sqlite { path: PathBuf },

    /// PostgreSQL database backend.
    #[allow(dead_code)]
    PostgreSql { connection_string: String },

    /// DynamoDB backend.
    #[allow(dead_code)]
    DynamoDb {
        table_name: String,
        region: Option<String>,
        endpoint: Option<String>,
    },
}

impl UserStorageUrl {
    /// Parse a user storage URL string.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// let url = UserStorageUrl::parse("users.toml")?;
    /// let url = UserStorageUrl::parse("postgresql://localhost/tenrankai")?;
    /// let url = UserStorageUrl::parse("dynamodb://users?region=us-west-2")?;
    /// ```
    pub fn parse(s: &str) -> Result<Self, UserStorageError> {
        let s = s.trim();

        // Handle URL schemes
        if let Some(rest) = s.strip_prefix("file://") {
            // file:// URL -> TOML file
            let path = if rest.starts_with('/') {
                PathBuf::from(rest)
            } else {
                PathBuf::from(format!("/{}", rest))
            };
            return Ok(Self::Toml { path });
        }

        if let Some(rest) = s.strip_prefix("sqlite://") {
            // sqlite:// URL - path can be absolute or relative
            return Ok(Self::Sqlite {
                path: PathBuf::from(rest),
            });
        }

        if s.starts_with("postgresql://") || s.starts_with("postgres://") {
            // PostgreSQL connection string
            return Ok(Self::PostgreSql {
                connection_string: s.to_string(),
            });
        }

        if let Some(rest) = s.strip_prefix("dynamodb://") {
            // dynamodb://table-name?region=us-west-2&endpoint=http://localhost:8000
            let (table_name, query) = match rest.find('?') {
                Some(pos) => (&rest[..pos], Some(&rest[pos + 1..])),
                None => (rest, None),
            };

            let mut region = None;
            let mut endpoint = None;

            if let Some(query_str) = query {
                for pair in query_str.split('&') {
                    if let Some((key, value)) = pair.split_once('=') {
                        match key {
                            "region" => region = Some(value.to_string()),
                            "endpoint" => endpoint = Some(value.to_string()),
                            _ => {} // Ignore unknown parameters
                        }
                    }
                }
            }

            return Ok(Self::DynamoDb {
                table_name: table_name.to_string(),
                region,
                endpoint,
            });
        }

        // No scheme - treat as TOML file path
        if s.ends_with(".toml") || s.ends_with(".TOML") {
            return Ok(Self::Toml {
                path: PathBuf::from(s),
            });
        }

        // Default: treat as TOML file path
        Ok(Self::Toml {
            path: PathBuf::from(s),
        })
    }

    /// Create a user storage instance from this URL.
    ///
    /// # Arguments
    ///
    /// * `site_id` - The site identifier for multi-tenant isolation
    pub async fn into_storage(self, site_id: &str) -> Result<DynUserStorage, UserStorageError> {
        match self {
            Self::Toml { path } => {
                let storage = TomlUserStorage::new(path, site_id.to_string()).await?;
                Ok(Arc::new(storage))
            }

            Self::Sqlite { path } => {
                #[cfg(feature = "user-storage-sql")]
                {
                    // Build SQLite connection string
                    let connection_string = format!("sqlite:{}", path.display());
                    let storage =
                        SqlUserStorage::new(&connection_string, site_id.to_string()).await?;
                    Ok(Arc::new(storage))
                }
                #[cfg(not(feature = "user-storage-sql"))]
                {
                    let _ = path; // Suppress unused warning
                    Err(UserStorageError::UnsupportedBackend(
                        "SQLite support requires the 'user-storage-sql' feature".to_string(),
                    ))
                }
            }

            Self::PostgreSql { connection_string } => {
                #[cfg(feature = "user-storage-sql")]
                {
                    let storage =
                        SqlUserStorage::new(&connection_string, site_id.to_string()).await?;
                    Ok(Arc::new(storage))
                }
                #[cfg(not(feature = "user-storage-sql"))]
                {
                    let _ = connection_string; // Suppress unused warning
                    Err(UserStorageError::UnsupportedBackend(
                        "PostgreSQL support requires the 'user-storage-sql' feature".to_string(),
                    ))
                }
            }

            Self::DynamoDb {
                table_name,
                region,
                endpoint,
            } => {
                #[cfg(feature = "user-storage-dynamodb")]
                {
                    use super::dynamodb::DynamoUserStorage;
                    let storage =
                        DynamoUserStorage::new(table_name, site_id.to_string(), region, endpoint)
                            .await?;
                    Ok(Arc::new(storage))
                }
                #[cfg(not(feature = "user-storage-dynamodb"))]
                {
                    let _ = (table_name, region, endpoint); // Suppress unused warnings
                    Err(UserStorageError::UnsupportedBackend(
                        "DynamoDB support requires the 'user-storage-dynamodb' feature".to_string(),
                    ))
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_toml_relative() {
        let url = UserStorageUrl::parse("users.toml").unwrap();
        match url {
            UserStorageUrl::Toml { path } => {
                assert_eq!(path, PathBuf::from("users.toml"));
            }
            _ => panic!("Expected Toml variant"),
        }
    }

    #[test]
    fn test_parse_toml_absolute() {
        let url = UserStorageUrl::parse("/etc/tenrankai/users.toml").unwrap();
        match url {
            UserStorageUrl::Toml { path } => {
                assert_eq!(path, PathBuf::from("/etc/tenrankai/users.toml"));
            }
            _ => panic!("Expected Toml variant"),
        }
    }

    #[test]
    fn test_parse_file_url() {
        let url = UserStorageUrl::parse("file:///etc/users.toml").unwrap();
        match url {
            UserStorageUrl::Toml { path } => {
                assert_eq!(path, PathBuf::from("/etc/users.toml"));
            }
            _ => panic!("Expected Toml variant"),
        }
    }

    #[test]
    fn test_parse_sqlite() {
        let url = UserStorageUrl::parse("sqlite:///var/data/users.db").unwrap();
        match url {
            UserStorageUrl::Sqlite { path } => {
                assert_eq!(path, PathBuf::from("/var/data/users.db"));
            }
            _ => panic!("Expected Sqlite variant"),
        }
    }

    #[test]
    fn test_parse_postgresql() {
        let url = UserStorageUrl::parse("postgresql://user:pass@localhost/tenrankai").unwrap();
        match url {
            UserStorageUrl::PostgreSql { connection_string } => {
                assert_eq!(
                    connection_string,
                    "postgresql://user:pass@localhost/tenrankai"
                );
            }
            _ => panic!("Expected PostgreSql variant"),
        }
    }

    #[test]
    fn test_parse_postgres_alias() {
        let url = UserStorageUrl::parse("postgres://localhost/db").unwrap();
        match url {
            UserStorageUrl::PostgreSql { connection_string } => {
                assert_eq!(connection_string, "postgres://localhost/db");
            }
            _ => panic!("Expected PostgreSql variant"),
        }
    }

    #[test]
    fn test_parse_dynamodb() {
        let url = UserStorageUrl::parse("dynamodb://users-table?region=us-west-2").unwrap();
        match url {
            UserStorageUrl::DynamoDb {
                table_name,
                region,
                endpoint,
            } => {
                assert_eq!(table_name, "users-table");
                assert_eq!(region, Some("us-west-2".to_string()));
                assert_eq!(endpoint, None);
            }
            _ => panic!("Expected DynamoDb variant"),
        }
    }

    #[test]
    fn test_parse_dynamodb_with_endpoint() {
        let url = UserStorageUrl::parse(
            "dynamodb://users?region=us-east-1&endpoint=http://localhost:8000",
        )
        .unwrap();
        match url {
            UserStorageUrl::DynamoDb {
                table_name,
                region,
                endpoint,
            } => {
                assert_eq!(table_name, "users");
                assert_eq!(region, Some("us-east-1".to_string()));
                assert_eq!(endpoint, Some("http://localhost:8000".to_string()));
            }
            _ => panic!("Expected DynamoDb variant"),
        }
    }

    #[test]
    fn test_parse_dynamodb_no_params() {
        let url = UserStorageUrl::parse("dynamodb://my-table").unwrap();
        match url {
            UserStorageUrl::DynamoDb {
                table_name,
                region,
                endpoint,
            } => {
                assert_eq!(table_name, "my-table");
                assert_eq!(region, None);
                assert_eq!(endpoint, None);
            }
            _ => panic!("Expected DynamoDb variant"),
        }
    }
}
