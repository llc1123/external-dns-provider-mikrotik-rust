use anyhow::{Context, Result, bail};

use crate::{
    app::{AppState, Changes},
    metadata,
    model::{self, Endpoint, RouterRecord},
};

fn identity(record: &RouterRecord) -> Result<(String, String, String)> {
    let endpoint = model::record_to_endpoint(record)?;
    Ok((
        endpoint
            .dns_name
            .context("managed record missing dnsName")?,
        endpoint.record_type,
        model::target_from_record(record)?,
    ))
}

fn desired(state: &AppState, endpoints: &[Endpoint]) -> Result<Vec<RouterRecord>> {
    endpoints
        .iter()
        .map(|ep| {
            model::endpoint_to_records(
                ep,
                state.config.default_ttl,
                state.config.default_comment.as_deref(),
            )
            .map_err(Into::into)
        })
        .collect::<Result<Vec<_>>>()
        .map(|groups| groups.into_iter().flatten().collect())
}

fn find(live: &[RouterRecord], needle: &(String, String, String)) -> Result<Option<usize>> {
    let mut found = None;
    for (index, record) in live.iter().enumerate() {
        if metadata::decode(&record.comment)?.is_none() {
            continue;
        }
        if &identity(record)? == needle && found.replace(index).is_some() {
            bail!("ambiguous managed RouterOS target");
        }
    }
    Ok(found)
}

fn equivalent(mut left: Endpoint, mut right: Endpoint) -> bool {
    left.provider_specific
        .sort_by(|a, b| a.name.cmp(&b.name).then(a.value.cmp(&b.value)));
    right
        .provider_specific
        .sort_by(|a, b| a.name.cmp(&b.name).then(a.value.cmp(&b.value)));
    left.targets.sort();
    right.targets.sort();
    left.labels.clear();
    right.labels.clear();
    left == right
}

pub async fn apply(state: &AppState, changes: Changes) -> Result<()> {
    if changes.update_old.len() != changes.update_new.len() {
        bail!("updateOld/updateNew lengths differ");
    }
    for (old, new) in changes.update_old.iter().zip(&changes.update_new) {
        if old.dns_name != new.dns_name || old.record_type != new.record_type {
            bail!("updateOld/updateNew name and type differ");
        }
    }
    let old = desired(
        state,
        &changes
            .update_old
            .iter()
            .chain(&changes.delete)
            .cloned()
            .collect::<Vec<_>>(),
    )?;
    let new = desired(
        state,
        &changes
            .update_new
            .iter()
            .chain(&changes.create)
            .cloned()
            .collect::<Vec<_>>(),
    )?;
    let live = state.routeros.list().await?;
    let old_keys = old.iter().map(identity).collect::<Result<Vec<_>>>()?;
    let new_keys = new.iter().map(identity).collect::<Result<Vec<_>>>()?;
    for key in &old_keys {
        if old_keys.iter().filter(|other| *other == key).count() > 1 {
            bail!("duplicate old RouterOS target");
        }
    }
    for key in &new_keys {
        if new_keys.iter().filter(|other| *other == key).count() > 1 {
            bail!("duplicate desired RouterOS target");
        }
    }
    let mut deletes = Vec::new();
    let mut updates = Vec::new();
    let mut creates = Vec::new();
    for (expected, key) in changes
        .update_old
        .iter()
        .chain(&changes.delete)
        .flat_map(|ep| ep.targets.iter().map(move |target| (ep, target)))
        .zip(&old_keys)
    {
        if let Some(i) = find(&live, key)? {
            let record = &live[i];
            let mut observed = model::record_to_endpoint(record)?;
            let mut requested = expected.0.clone();
            requested.targets = vec![expected.1.clone()];
            observed.labels.clear();
            requested.labels.clear();
            if requested.set_identifier.as_deref() == Some("") {
                requested.set_identifier = None;
            }
            if requested.record_type == "TXT"
                && requested.record_ttl == 0
                && requested
                    .provider_specific
                    .iter()
                    .any(|p| p.name == model::ORIGINAL_TTL_PROPERTY && p.value == "0")
            {
                requested.record_ttl = observed.record_ttl;
            }
            if requested.record_type == "TXT" {
                requested.record_ttl = observed.record_ttl;
                requested.provider_specific = observed.provider_specific.clone();
            }
            if !equivalent(observed, requested) {
                bail!("managed RouterOS record differs from observed updateOld/delete");
            }
            let id = record
                .id
                .clone()
                .context("managed RouterOS record missing .id")?;
            if !new_keys.contains(key) && !deletes.contains(&id) {
                deletes.push(id);
            }
        }
    }
    for (record, key) in new.iter().zip(&new_keys) {
        for entry in &live {
            if metadata::decode(&entry.comment)?.is_none()
                && entry.r#type == record.r#type
                && entry.name == record.name
                && entry.regexp == record.regexp
            {
                bail!("foreign RouterOS record collides with managed name and type");
            }
        }
        match find(&live, key)? {
            Some(i) => {
                let current = &live[i];
                let current_endpoint = model::record_to_endpoint(current)?;
                let desired_endpoint = model::record_to_endpoint(record)?;
                if !equivalent(current_endpoint, desired_endpoint)
                    && !deletes.contains(&current.id.clone().unwrap_or_default())
                {
                    updates.push((
                        current
                            .id
                            .clone()
                            .context("managed RouterOS record missing .id")?,
                        record.clone(),
                    ));
                }
            }
            None => creates.push(record.clone()),
        }
    }
    for id in deletes {
        state.routeros.delete(&id).await?;
    }
    for (id, record) in updates {
        state.routeros.update(&id, &record).await?;
    }
    for record in creates {
        state.routeros.create(&record).await?;
    }
    Ok(())
}
