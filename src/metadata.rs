use std::collections::BTreeMap;

use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const PREFIX: &str = "edm1:";

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Metadata {
    pub dns_name: Option<String>,
    pub original_ttl: u64,
    pub applied_ttl: u64,
    #[serde(default)]
    pub applied_comment: String,
    pub provider_specific: BTreeMap<String, String>,
    #[serde(default)]
    pub applied_provider_specific: BTreeMap<String, String>,
}

#[derive(Debug, Error)]
pub enum MetadataError {
    #[error("malformed reserved metadata")]
    Malformed,
    #[error("unsupported reserved metadata version")]
    Future,
}

pub fn encode(meta: &Metadata, user_comment: &str) -> Result<String, MetadataError> {
    let bytes = serde_json::to_vec(meta).map_err(|_| MetadataError::Malformed)?;
    Ok(format!(
        "{PREFIX}{};{user_comment}",
        URL_SAFE_NO_PAD.encode(bytes)
    ))
}

pub fn decode(comment: &str) -> Result<Option<(Metadata, String)>, MetadataError> {
    let Some((version, _)) = comment.split_once(':') else {
        return Ok(None);
    };
    if version != "edm"
        && !version
            .strip_prefix("edm")
            .is_some_and(|suffix| !suffix.is_empty() && suffix.bytes().all(|c| c.is_ascii_digit()))
    {
        return Ok(None);
    }
    if version != "edm1" {
        return Err(MetadataError::Future);
    }
    let rest = comment
        .strip_prefix(PREFIX)
        .ok_or(MetadataError::Malformed)?;
    let (encoded, user) = rest.split_once(';').ok_or(MetadataError::Malformed)?;
    let bytes = URL_SAFE_NO_PAD
        .decode(encoded)
        .map_err(|_| MetadataError::Malformed)?;
    let meta = serde_json::from_slice(&bytes).map_err(|_| MetadataError::Malformed)?;
    Ok(Some((meta, user.to_owned())))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, reason = "test fixture assertions")]
mod tests {
    use super::*;
    #[test]
    fn round_trip_preserves_unicode_delimiters_and_empty_values() {
        let mut values = BTreeMap::new();
        values.insert("unknown".into(), "".into());
        let m = Metadata {
            dns_name: Some("x.example.".into()),
            original_ttl: 0,
            applied_ttl: 3600,
            applied_comment: String::new(),
            provider_specific: values,
            applied_provider_specific: BTreeMap::new(),
        };
        let encoded = encode(&m, "用户;|;comment").unwrap();
        assert_eq!(
            decode(&encoded).unwrap(),
            Some((m, "用户;|;comment".into()))
        );
    }
    #[test]
    fn malformed_reserved_metadata_fails_closed() {
        assert!(decode("edm1:not-base64;u").is_err());
    }
    #[test]
    fn codec_roundtrips_many_unicode_comments() {
        let meta = Metadata::default();
        for length in 0..80 {
            let comment = "é;🔐|".repeat(length);
            assert_eq!(
                decode(&encode(&meta, &comment).unwrap()).unwrap(),
                Some((meta.clone(), comment))
            );
        }
    }
}
