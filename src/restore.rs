use std::{collections::BTreeMap, net::IpAddr};

use crate::{
    metadata,
    model::{
        Endpoint, ModelError, ORIGINAL_TTL_PROPERTY, ProviderSpecific, RouterRecord, ttl_seconds,
    },
};

pub fn record_to_endpoint(r: &RouterRecord) -> Result<Endpoint, ModelError> {
    let (meta, user) = match metadata::decode(&r.comment)? {
        Some(x) => x,
        None => return Err(ModelError::Target("record is not managed".into())),
    };
    let target = target_from_record(r)?;
    let actual_name = r.name.clone();
    let actual_ttl = ttl_seconds(&r.ttl)?;
    let exposed_ttl = if actual_ttl == meta.applied_ttl {
        if meta.original_ttl == 0 {
            meta.applied_ttl
        } else {
            meta.original_ttl
        }
    } else {
        actual_ttl
    };
    let mut ps: Vec<ProviderSpecific> = meta
        .provider_specific
        .into_iter()
        .map(|(name, value)| ProviderSpecific { name, value })
        .collect();
    if meta.original_ttl == 0 {
        ps.push(ProviderSpecific {
            name: ORIGINAL_TTL_PROPERTY.into(),
            value: "0".into(),
        });
    }
    if let Some(p) = ps
        .iter_mut()
        .find(|p| p.name == "comment" || p.name == "webhook/comment")
    {
        p.value = user;
    } else if user != meta.applied_comment {
        ps.push(ProviderSpecific {
            name: "comment".into(),
            value: user,
        });
    }
    for name in ["regexp", "match-subdomain", "address-list", "disabled"] {
        let baseline = meta.applied_provider_specific.get(name).map(String::as_str);
        let actual = match name {
            "regexp" => r.regexp.clone(),
            "match-subdomain" => r.match_subdomain.clone(),
            "address-list" => r.address_list.clone(),
            "disabled" => r.disabled.clone(),
            _ => None,
        };
        let observed = match name {
            "disabled" | "match-subdomain" if actual.as_deref().is_none_or(|v| v == "false") => {
                Some("false")
            }
            "address-list" if actual.as_deref().is_none_or(str::is_empty) => Some(""),
            _ => actual.as_deref(),
        };
        let expected = match name {
            "disabled" | "match-subdomain" if baseline.is_none() => Some("false"),
            "address-list" if baseline.is_none() => Some(""),
            _ => baseline,
        };
        if observed != expected {
            if let Some(property) = ps
                .iter_mut()
                .find(|p| p.name == name || p.name == format!("webhook/{name}"))
            {
                property.value = actual.clone().unwrap_or_default();
            } else {
                ps.push(ProviderSpecific {
                    name: name.to_owned(),
                    value: actual.unwrap_or_default(),
                });
            }
        }
    }
    if r.regexp.is_none()
        && actual_name.as_deref()
            != meta
                .dns_name
                .as_deref()
                .map(|n| n.strip_suffix('.').unwrap_or(n))
    {
        ps.push(ProviderSpecific {
            name: "webhook/name".into(),
            value: actual_name.unwrap_or_default(),
        });
    }
    Ok(Endpoint {
        dns_name: meta.dns_name.or_else(|| r.name.clone()),
        record_type: r.r#type.clone(),
        record_ttl: exposed_ttl,
        targets: vec![target],
        provider_specific: ps,
        set_identifier: None,
        labels: BTreeMap::new(),
    })
}

pub fn target_from_record(r: &RouterRecord) -> Result<String, ModelError> {
    let target = match r.r#type.as_str() {
        "A" | "AAAA" => r.address.clone(),
        "CNAME" => r.cname.clone(),
        "TXT" => r.text.clone(),
        "NS" => r.ns.clone(),
        "MX" => Some(format!(
            "{} {}",
            r.mx_preference
                .as_deref()
                .ok_or_else(|| ModelError::Target("missing MX preference".into()))?,
            r.mx_exchange
                .as_deref()
                .ok_or_else(|| ModelError::Target("missing MX exchange".into()))?
        )),
        "SRV" => Some(format!(
            "{} {} {} {}",
            r.srv_priority
                .as_deref()
                .ok_or_else(|| ModelError::Target("missing SRV priority".into()))?,
            r.srv_weight
                .as_deref()
                .ok_or_else(|| ModelError::Target("missing SRV weight".into()))?,
            r.srv_port
                .as_deref()
                .ok_or_else(|| ModelError::Target("missing SRV port".into()))?,
            r.srv_target
                .as_deref()
                .ok_or_else(|| ModelError::Target("missing SRV target".into()))?
        )),
        other => return Err(ModelError::Unsupported(other.into())),
    }
    .ok_or_else(|| ModelError::Target("missing target".into()))?;
    match r.r#type.as_str() {
        "A" | "AAAA" => {
            let ip = target
                .parse::<IpAddr>()
                .map_err(|_| ModelError::Target(target.clone()))?;
            if (r.r#type == "A" && !ip.is_ipv4()) || (r.r#type == "AAAA" && !ip.is_ipv6()) {
                return Err(ModelError::Target(target));
            }
            Ok(ip.to_string())
        }
        "MX" => {
            let mut p = target.split_whitespace();
            Ok(format!(
                "{} {}",
                super::model::uint16(
                    p.next().ok_or_else(|| ModelError::Target(target.clone()))?,
                    &target
                )?,
                super::model::canonical_name(
                    p.next().ok_or_else(|| ModelError::Target(target.clone()))?,
                    &target
                )?
            ))
        }
        "SRV" => {
            let mut p = target.split_whitespace();
            Ok(format!(
                "{} {} {} {}",
                super::model::uint16(
                    p.next().ok_or_else(|| ModelError::Target(target.clone()))?,
                    &target
                )?,
                super::model::uint16(
                    p.next().ok_or_else(|| ModelError::Target(target.clone()))?,
                    &target
                )?,
                super::model::uint16(
                    p.next().ok_or_else(|| ModelError::Target(target.clone()))?,
                    &target
                )?,
                super::model::canonical_name(
                    p.next().ok_or_else(|| ModelError::Target(target.clone()))?,
                    &target
                )?
            ))
        }
        "CNAME" | "NS" => super::model::canonical_name(&target, &target),
        "TXT" => Ok(target),
        other => Err(ModelError::Unsupported(other.into())),
    }
}
