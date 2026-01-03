use serde::{Deserialize, Serialize};
use std::str::FromStr;

/// Log level type system for configuration and tracing integration
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize, Serialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Trace,
    Debug,
    #[default]
    Info,
    Warn,
    Error,
}

impl LogLevel {
    /// Convert to tracing::Level
    pub fn to_tracing_level(&self) -> tracing::Level {
        match self {
            LogLevel::Trace => tracing::Level::TRACE,
            LogLevel::Debug => tracing::Level::DEBUG,
            LogLevel::Info => tracing::Level::INFO,
            LogLevel::Warn => tracing::Level::WARN,
            LogLevel::Error => tracing::Level::ERROR,
        }
    }

    /// Convert to tracing filter level
    pub fn to_tracing_filter(&self) -> tracing::metadata::LevelFilter {
        match self {
            LogLevel::Trace => tracing::metadata::LevelFilter::TRACE,
            LogLevel::Debug => tracing::metadata::LevelFilter::DEBUG,
            LogLevel::Info => tracing::metadata::LevelFilter::INFO,
            LogLevel::Warn => tracing::metadata::LevelFilter::WARN,
            LogLevel::Error => tracing::metadata::LevelFilter::ERROR,
        }
    }

    /// Check if this is a verbose log level (trace or debug)
    pub fn is_verbose(&self) -> bool {
        matches!(self, LogLevel::Trace | LogLevel::Debug)
    }

    /// Check if a message at a given level would be logged at this log level
    pub fn would_log(&self, level: LogLevel) -> bool {
        // Lower numeric values in tracing = higher priority
        // Error = 1, Warn = 2, Info = 3, Debug = 4, Trace = 5
        self.to_tracing_level() >= level.to_tracing_level()
    }

    /// All available log levels in order from most verbose to least
    pub const ALL: &'static [LogLevel] = &[
        LogLevel::Trace,
        LogLevel::Debug,
        LogLevel::Info,
        LogLevel::Warn,
        LogLevel::Error,
    ];

    /// Parse a log level from a string (case insensitive)
    pub fn parse(s: &str) -> Option<LogLevel> {
        match s.to_lowercase().as_str() {
            "trace" => Some(LogLevel::Trace),
            "debug" => Some(LogLevel::Debug),
            "info" => Some(LogLevel::Info),
            "warn" => Some(LogLevel::Warn),
            "error" => Some(LogLevel::Error),
            _ => None,
        }
    }

    /// Get the string representation of this log level
    pub fn as_str(&self) -> &'static str {
        match self {
            LogLevel::Trace => "trace",
            LogLevel::Debug => "debug",
            LogLevel::Info => "info",
            LogLevel::Warn => "warn",
            LogLevel::Error => "error",
        }
    }
}

impl std::fmt::Display for LogLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl FromStr for LogLevel {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s).ok_or_else(|| {
            format!(
                "Invalid log level '{}', valid options are: {}",
                s,
                Self::ALL
                    .iter()
                    .map(|l| l.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_log_level_enum_functionality() {
        // Test parsing
        assert_eq!(LogLevel::parse("debug"), Some(LogLevel::Debug));
        assert_eq!(LogLevel::parse("INFO"), Some(LogLevel::Info)); // Case insensitive
        assert_eq!(LogLevel::parse("invalid"), None);

        // Test string conversion
        assert_eq!(LogLevel::Debug.as_str(), "debug");
        assert_eq!(LogLevel::Info.as_str(), "info");

        // Test verbose checking
        assert!(LogLevel::Trace.is_verbose());
        assert!(LogLevel::Debug.is_verbose());
        assert!(!LogLevel::Info.is_verbose());
        assert!(!LogLevel::Warn.is_verbose());
        assert!(!LogLevel::Error.is_verbose());

        // Test level filtering
        assert!(LogLevel::Info.would_log(LogLevel::Info));
        assert!(LogLevel::Info.would_log(LogLevel::Warn));
        assert!(LogLevel::Info.would_log(LogLevel::Error));
        assert!(!LogLevel::Info.would_log(LogLevel::Debug));
        assert!(!LogLevel::Info.would_log(LogLevel::Trace));

        // Test tracing conversion
        assert_eq!(LogLevel::Debug.to_tracing_level(), tracing::Level::DEBUG);
        assert_eq!(LogLevel::Info.to_tracing_filter(), tracing::metadata::LevelFilter::INFO);

        // Test default
        assert_eq!(LogLevel::default(), LogLevel::Info);

        // Test display
        assert_eq!(format!("{}", LogLevel::Warn), "warn");

        // Test FromStr
        assert_eq!("error".parse::<LogLevel>().unwrap(), LogLevel::Error);
        assert!("invalid".parse::<LogLevel>().is_err());

        // Test constants
        assert_eq!(LogLevel::ALL.len(), 5);
        assert_eq!(LogLevel::ALL[0], LogLevel::Trace);
        assert_eq!(LogLevel::ALL[4], LogLevel::Error);
    }

    #[test]
    fn test_log_level_serde() {
        // Test with a simple struct to verify serde integration
        #[derive(Deserialize, Serialize, PartialEq, Debug)]
        struct TestConfig {
            log_level: LogLevel,
        }

        let config = TestConfig {
            log_level: LogLevel::Debug,
        };

        // Test serialization with TOML
        let toml_str = toml_edit::ser::to_string_pretty(&config).unwrap();
        assert!(toml_str.contains("log_level = \"debug\""));

        // Test deserialization from TOML
        let parsed: TestConfig = toml_edit::de::from_str(&toml_str).unwrap();
        assert_eq!(parsed.log_level, LogLevel::Debug);
    }
}