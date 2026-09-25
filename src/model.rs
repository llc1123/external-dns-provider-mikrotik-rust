use std::{collections::BTreeMap, net::IpAddr};

use crate::{metadata, metadata::Metadata};

pub const ORIGINAL_TTL_PROPERTY: &str = "webhook/original-ttl";

pub use crate::types::{Endpoint, ModelError, ProviderSpecific, RouterRecord};

pub(crate) fn uint16(value: &str, target: &str) -> Result<String, ModelError> {
    value
        .parse::<u16>()
        .map(|n| n.to_string())
        .map_err(|_| ModelError::Target(target.into()))
}

pub(crate) fn canonical_name(value: &str, target: &str) -> Result<String, ModelError> {
    if value == "." {
        return Ok(value.into());
    }
    let name = value.strip_suffix('.').unwrap_or(value);
    if name.is_empty()
        || name.len() > 253
        || name.split('.').any(|label| {
            label.is_empty()
                || label.len() > 63
                || !label
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        })
    {
        return Err(ModelError::Target(target.into()));
    }
    Ok(name.into())
}

fn physical_bool(value: &str, target: &str) -> Result<String, ModelError> {
    match value.to_ascii_lowercase().as_str() {
        "true" | "yes" => Ok("true".into()),
        "false" | "no" => Ok("false".into()),
        _ => Err(ModelError::Target(format!("invalid boolean {target}"))),
    }
}

pub use crate::ttl::{ttl_seconds, ttl_text};

pub fn endpoint_to_records(
    ep: &Endpoint,
    default_ttl: u64,
    default_comment: Option<&str>,
) -> Result<Vec<RouterRecord>, ModelError> {
    if ep.set_identifier.as_deref().is_some_and(|s| !s.is_empty()) {
        return Err(ModelError::SetIdentifier);
    }
    let original_name = ep.dns_name.clone();
    let name = canonical_name(ep.dns_name.as_deref().ok_or(ModelError::Name)?, "DNS name")?;
    if ep.targets.is_empty() {
        return Err(ModelError::Target("no targets".into()));
    }
    let marker = ep
        .provider_specific
        .iter()
        .find(|p| p.name == ORIGINAL_TTL_PROPERTY);
    if marker.is_some()
        && ep
            .provider_specific
            .iter()
            .filter(|p| p.name == ORIGINAL_TTL_PROPERTY)
            .count()
            != 1
    {
        return Err(ModelError::Target("duplicate reserved TTL marker".into()));
    }
    let original_ttl = match marker {
        Some(p) if p.value == "0" => 0,
        Some(_) => return Err(ModelError::Target("invalid reserved TTL marker".into())),
        None => ep.record_ttl,
    };
    let ttl = if ep.record_ttl == 0 {
        default_ttl
    } else {
        ep.record_ttl
    };
    let mut props = BTreeMap::new();
    let mut user = default_comment.unwrap_or("").to_owned();
    let ownership_txt = ep.record_type == "TXT"
        && (ep.labels.contains_key("ownedRecord")
            || ep
                .targets
                .iter()
                .any(|target| target.contains("heritage=external-dns")));
    let mut common = RouterRecord {
        name: Some(name),
        r#type: ep.record_type.clone(),
        ttl: ttl_text(ttl),
        ..RouterRecord::default()
    };
    for p in &ep.provider_specific {
        if p.name == ORIGINAL_TTL_PROPERTY {
            continue;
        }
        if props.insert(p.name.clone(), p.value.clone()).is_some() {
            return Err(ModelError::Target(format!(
                "duplicate providerSpecific {}",
                p.name
            )));
        }
        match p.name.as_str() {
            "comment" | "webhook/comment" => user = p.value.clone(),
            "regexp" | "webhook/regexp" if ep.record_type != "TXT" || !ownership_txt => {
                if !p.value.is_empty() {
                    common.regexp = Some(p.value.clone());
                }
            }
            "match-subdomain" | "webhook/match-subdomain" => {
                common.match_subdomain = Some(physical_bool(&p.value, &p.name)?)
            }
            "address-list" | "webhook/address-list" => common.address_list = Some(p.value.clone()),
            "disabled" | "webhook/disabled" => {
                common.disabled = Some(physical_bool(&p.value, &p.name)?)
            }
            _ => {}
        }
    }
    for key in [
        "comment",
        "disabled",
        "regexp",
        "match-subdomain",
        "address-list",
    ] {
        if props.contains_key(key) && props.contains_key(&format!("webhook/{key}")) {
            return Err(ModelError::Target(format!("conflicting {key} aliases")));
        }
    }
    if common.regexp.is_some() {
        common.name = None;
    }
    let mut applied_provider_specific = BTreeMap::new();
    for (name, value) in [
        ("regexp", common.regexp.clone()),
        ("match-subdomain", common.match_subdomain.clone()),
        ("address-list", common.address_list.clone()),
        ("disabled", common.disabled.clone()),
    ] {
        if let Some(value) = value {
            applied_provider_specific.insert(name.into(), value);
        }
    }
    let meta = Metadata {
        dns_name: original_name,
        original_ttl,
        applied_ttl: ttl,
        applied_comment: user.clone(),
        provider_specific: props,
        applied_provider_specific,
    };
    common.comment = metadata::encode(&meta, &user)?;
    ep.targets
        .iter()
        .map(|target| {
            let mut r = common.clone();
            match ep.record_type.as_str() {
                "A" | "AAAA" => {
                    let ip: IpAddr = target
                        .parse()
                        .map_err(|_| ModelError::Target(target.clone()))?;
                    if (ep.record_type == "A" && !ip.is_ipv4())
                        || (ep.record_type == "AAAA" && !ip.is_ipv6())
                    {
                        return Err(ModelError::Target(target.clone()));
                    }
                    r.address = Some(ip.to_string());
                }
                "CNAME" => r.cname = Some(canonical_name(target, target)?),
                "TXT" => r.text = Some(target.clone()),
                "NS" => r.ns = Some(canonical_name(target, target)?),
                "MX" => {
                    let mut p = target.split_whitespace();
                    r.mx_preference = Some(uint16(
                        p.next().ok_or_else(|| ModelError::Target(target.clone()))?,
                        target,
                    )?);
                    r.mx_exchange = Some(canonical_name(
                        p.next().ok_or_else(|| ModelError::Target(target.clone()))?,
                        target,
                    )?);
                    if p.next().is_some() {
                        return Err(ModelError::Target(target.clone()));
                    }
                }
                "SRV" => {
                    let mut p = target.split_whitespace();
                    r.srv_priority = Some(uint16(
                        p.next().ok_or_else(|| ModelError::Target(target.clone()))?,
                        target,
                    )?);
                    r.srv_weight = Some(uint16(
                        p.next().ok_or_else(|| ModelError::Target(target.clone()))?,
                        target,
                    )?);
                    r.srv_port = Some(uint16(
                        p.next().ok_or_else(|| ModelError::Target(target.clone()))?,
                        target,
                    )?);
                    r.srv_target = Some(canonical_name(
                        p.next().ok_or_else(|| ModelError::Target(target.clone()))?,
                        target,
                    )?);
                    if p.next().is_some() {
                        return Err(ModelError::Target(target.clone()));
                    }
                }
                other => return Err(ModelError::Unsupported(other.into())),
            }
            Ok(r)
        })
        .collect()
}

pub use crate::restore::record_to_endpoint;

pub use crate::restore::target_from_record;
