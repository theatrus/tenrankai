use serde::de::{self, Visitor};
use serde::ser::SerializeSeq;
use std::path::PathBuf;

/// Deserialize static directories configuration from either a single string or array of strings
pub fn deserialize_static_directories<'de, D>(deserializer: D) -> Result<Vec<PathBuf>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    struct StaticDirectoriesVisitor;

    impl<'de> Visitor<'de> for StaticDirectoriesVisitor {
        type Value = Vec<PathBuf>;

        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("a path string or an array of path strings")
        }

        fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            Ok(vec![PathBuf::from(value)])
        }

        fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
        where
            A: de::SeqAccess<'de>,
        {
            let mut dirs = Vec::new();
            while let Some(dir) = seq.next_element::<String>()? {
                dirs.push(PathBuf::from(dir));
            }
            Ok(dirs)
        }
    }

    deserializer.deserialize_any(StaticDirectoriesVisitor)
}

/// Serialize static directories configuration as either a single string or array of strings
pub fn serialize_static_directories<S>(
    dirs: &Vec<PathBuf>,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    if dirs.len() == 1 {
        serializer.serialize_str(dirs[0].to_str().unwrap_or(""))
    } else {
        let mut seq = serializer.serialize_seq(Some(dirs.len()))?;
        for dir in dirs {
            seq.serialize_element(dir.to_str().unwrap_or(""))?;
        }
        seq.end()
    }
}

/// Deserialize template directories configuration from either a single string or array of strings
pub fn deserialize_template_directories<'de, D>(deserializer: D) -> Result<Vec<PathBuf>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    struct TemplateDirectoriesVisitor;

    impl<'de> Visitor<'de> for TemplateDirectoriesVisitor {
        type Value = Vec<PathBuf>;

        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("a path string or an array of path strings")
        }

        fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            Ok(vec![PathBuf::from(value)])
        }

        fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
        where
            A: de::SeqAccess<'de>,
        {
            let mut dirs = Vec::new();
            while let Some(dir) = seq.next_element::<String>()? {
                dirs.push(PathBuf::from(dir));
            }
            Ok(dirs)
        }
    }

    deserializer.deserialize_any(TemplateDirectoriesVisitor)
}

/// Serialize template directories configuration as either a single string or array of strings
pub fn serialize_template_directories<S>(
    dirs: &Vec<PathBuf>,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    if dirs.len() == 1 {
        serializer.serialize_str(dirs[0].to_str().unwrap_or(""))
    } else {
        let mut seq = serializer.serialize_seq(Some(dirs.len()))?;
        for dir in dirs {
            seq.serialize_element(dir.to_str().unwrap_or(""))?;
        }
        seq.end()
    }
}
