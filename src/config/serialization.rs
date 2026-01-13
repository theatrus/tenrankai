use serde::de::{self, Visitor};
use serde::ser::SerializeSeq;

/// Deserialize storage URLs from either a single string or array of strings
///
/// Supports both filesystem paths and S3 URLs:
/// - `"static"` - relative filesystem path
/// - `"/var/data/static"` - absolute filesystem path
/// - `"s3://bucket/prefix"` - S3 bucket with optional prefix
pub fn deserialize_storage_urls<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    struct StorageUrlsVisitor;

    impl<'de> Visitor<'de> for StorageUrlsVisitor {
        type Value = Vec<String>;

        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("a path/URL string or an array of path/URL strings")
        }

        fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            Ok(vec![value.to_string()])
        }

        fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
        where
            A: de::SeqAccess<'de>,
        {
            let mut urls = Vec::new();
            while let Some(url) = seq.next_element::<String>()? {
                urls.push(url);
            }
            Ok(urls)
        }
    }

    deserializer.deserialize_any(StorageUrlsVisitor)
}

/// Serialize storage URLs as either a single string or array of strings
pub fn serialize_storage_urls<S>(urls: &Vec<String>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    if urls.len() == 1 {
        serializer.serialize_str(&urls[0])
    } else {
        let mut seq = serializer.serialize_seq(Some(urls.len()))?;
        for url in urls {
            seq.serialize_element(url)?;
        }
        seq.end()
    }
}

