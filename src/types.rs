use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::metadata;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderSpecific {
    pub name: String,
    #[serde(default)]
    pub value: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Endpoint {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dns_name: Option<String>,
    pub record_type: String,
    #[serde(rename = "recordTTL", default)]
    pub record_ttl: u64,
    #[serde(default, deserialize_with = "null_vec_default")]
    pub targets: Vec<String>,
    #[serde(default, deserialize_with = "null_vec_default")]
    pub provider_specific: Vec<ProviderSpecific>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub set_identifier: Option<String>,
    #[serde(default, deserialize_with = "null_map_default")]
    pub labels: BTreeMap<String, String>,
}

fn null_vec_default<'de, D, T>(deserializer: D) -> Result<Vec<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Ok(Option::<Vec<T>>::deserialize(deserializer)?.unwrap_or_default())
}

fn null_map_default<'de, D, K, V>(deserializer: D) -> Result<BTreeMap<K, V>, D::Error>
where
    D: serde::Deserializer<'de>,
    K: Ord + Deserialize<'de>,
    V: Deserialize<'de>,
{
    Ok(Option::<BTreeMap<K, V>>::deserialize(deserializer)?.unwrap_or_default())
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RouterRecord {
    #[serde(rename = ".id", default, skip_serializing)]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub regexp: Option<String>,
    pub r#type: String,
    #[serde(default)]
    pub ttl: String,
    #[serde(default)]
    pub comment: String,
    #[serde(
        rename = "match-subdomain",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub match_subdomain: Option<String>,
    #[serde(
        rename = "address-list",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub address_list: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disabled: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub address: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cname: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(
        rename = "mx-exchange",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub mx_exchange: Option<String>,
    #[serde(
        rename = "mx-preference",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub mx_preference: Option<String>,
    #[serde(rename = "srv-port", default, skip_serializing_if = "Option::is_none")]
    pub srv_port: Option<String>,
    #[serde(
        rename = "srv-target",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub srv_target: Option<String>,
    #[serde(
        rename = "srv-priority",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub srv_priority: Option<String>,
    #[serde(
        rename = "srv-weight",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub srv_weight: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ns: Option<String>,
}

#[derive(Debug, Error)]
pub enum ModelError {
    #[error("unsupported DNS type {0}")]
    Unsupported(String),
    #[error("invalid target: {0}")]
    Target(String),
    #[error("invalid RouterOS TTL {0}")]
    Ttl(String),
    #[error("reserved metadata: {0}")]
    Metadata(#[from] metadata::MetadataError),
    #[error("setIdentifier is unsupported")]
    SetIdentifier,
    #[error("invalid DNS name")]
    Name,
}
