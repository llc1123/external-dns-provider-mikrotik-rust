#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "test fixture assertions"
)]

use std::collections::BTreeMap;

use external_dns_provider_mikrotik::{
    metadata,
    model::{self, Endpoint, ProviderSpecific, RouterRecord},
};

#[test]
fn metadata_exposes_live_ttl_and_provider_drift() {
    let endpoint = Endpoint {
        dns_name: Some("x.example.".into()),
        record_type: "A".into(),
        record_ttl: 0,
        targets: vec!["192.0.2.1".into()],
        provider_specific: vec![ProviderSpecific {
            name: "disabled".into(),
            value: "true".into(),
        }],
        set_identifier: None,
        labels: BTreeMap::new(),
    };
    let records = model::endpoint_to_records(&endpoint, 3600, None).expect("fixture");
    let mut live = records[0].clone();
    live.ttl = "2h".into();
    live.disabled = Some("false".into());
    let restored = model::record_to_endpoint(&live).expect("fixture");
    assert_eq!(restored.record_ttl, 7200);
    assert_eq!(
        restored
            .provider_specific
            .iter()
            .find(|p| p.name == "disabled")
            .map(|p| p.value.as_str()),
        Some("false")
    );
}

#[test]
fn future_reserved_metadata_is_not_treated_as_unmanaged() {
    assert!(metadata::decode("edm2:abc;comment").is_err());
    assert!(metadata::decode("edm1:not-base64;comment").is_err());
    assert!(metadata::decode("edmund: human note").is_ok_and(|value| value.is_none()));
}

#[test]
fn live_ttl_zero_parses_and_invalid_ttl_fails_closed() {
    let ep = Endpoint {
        dns_name: Some("zero.example.org".into()),
        record_type: "A".into(),
        record_ttl: 0,
        targets: vec!["192.0.2.2".into()],
        ..Endpoint::default()
    };
    let mut record = model::endpoint_to_records(&ep, 3600, None)
        .expect("fixture")
        .remove(0);
    record.ttl = "0s".into();
    assert_eq!(
        model::record_to_endpoint(&record)
            .expect("zero is valid")
            .record_ttl,
        0
    );
    record.ttl = "bad".into();
    assert!(model::record_to_endpoint(&record).is_err());
    record.ttl.clear();
    assert!(model::record_to_endpoint(&record).is_err());
}

#[test]
fn optional_properties_preserve_presence_and_unknown_keys() {
    let ep = Endpoint {
        dns_name: Some("option.example.org".into()),
        record_type: "A".into(),
        record_ttl: 300,
        targets: vec!["192.0.2.1".into()],
        provider_specific: vec![
            ProviderSpecific {
                name: "webhook/comment".into(),
                value: "用户;literal".into(),
            },
            ProviderSpecific {
                name: "webhook/disabled".into(),
                value: "false".into(),
            },
            ProviderSpecific {
                name: "address-list".into(),
                value: String::new(),
            },
            ProviderSpecific {
                name: "opaque".into(),
                value: String::new(),
            },
        ],
        ..Endpoint::default()
    };
    let mut record = model::endpoint_to_records(&ep, 3600, Some("fallback"))
        .expect("fixture")
        .remove(0);
    record.disabled = Some("false".into());
    record.address_list = None;
    let restored = model::record_to_endpoint(&record).expect("roundtrip");
    assert_eq!(restored.provider_specific.len(), 4);
    for property in ep.provider_specific {
        assert!(restored.provider_specific.contains(&property));
    }
}

#[test]
fn boolean_aliases_normalize_physical_values_without_losing_source_text() {
    let endpoint = Endpoint {
        dns_name: Some("bool.example.org".into()),
        record_type: "A".into(),
        record_ttl: 300,
        targets: vec!["192.0.2.3".into()],
        provider_specific: vec![ProviderSpecific {
            name: "disabled".into(),
            value: "yes".into(),
        }],
        ..Endpoint::default()
    };
    let record = model::endpoint_to_records(&endpoint, 3600, None)
        .expect("boolean")
        .remove(0);
    assert_eq!(record.disabled.as_deref(), Some("true"));
    let restored = model::record_to_endpoint(&record).expect("restore");
    assert!(restored.provider_specific.contains(&ProviderSpecific {
        name: "disabled".into(),
        value: "yes".into()
    }));
}

#[test]
fn empty_regexp_is_metadata_only_but_user_txt_regexp_is_physical() {
    let named = Endpoint {
        dns_name: Some("named.example.org".into()),
        record_type: "A".into(),
        record_ttl: 300,
        targets: vec!["192.0.2.4".into()],
        provider_specific: vec![ProviderSpecific {
            name: "regexp".into(),
            value: String::new(),
        }],
        ..Endpoint::default()
    };
    let named_record = model::endpoint_to_records(&named, 3600, None)
        .expect("named")
        .remove(0);
    assert!(named_record.name.is_some() && named_record.regexp.is_none());
    let txt = Endpoint {
        dns_name: Some("txt.example.org".into()),
        record_type: "TXT".into(),
        record_ttl: 300,
        targets: vec!["payload".into()],
        provider_specific: vec![ProviderSpecific {
            name: "regexp".into(),
            value: "^payload$".into(),
        }],
        ..Endpoint::default()
    };
    let txt_record = model::endpoint_to_records(&txt, 3600, None)
        .expect("txt")
        .remove(0);
    assert_eq!(txt_record.regexp.as_deref(), Some("^payload$"));
}

#[test]
fn router_payload_omits_absent_optional_fields() {
    let record = RouterRecord {
        name: Some("x".into()),
        r#type: "A".into(),
        address: Some("192.0.2.1".into()),
        ttl: "1h".into(),
        comment: "edm1:x;".into(),
        ..RouterRecord::default()
    };
    let json = serde_json::to_value(record).expect("fixture");
    assert!(json.get("regexp").is_none());
    assert!(json.get(".id").is_none());
}

#[test]
fn metadata_json_order_is_stable() {
    let mut values = BTreeMap::new();
    values.insert("z".into(), "1".into());
    values.insert("a".into(), "2".into());
    let metadata = metadata::Metadata {
        dns_name: Some("x".into()),
        original_ttl: 1,
        applied_ttl: 2,
        applied_comment: String::new(),
        provider_specific: values,
        applied_provider_specific: BTreeMap::new(),
    };
    let encoded = metadata::encode(&metadata, "").expect("fixture");
    assert_eq!(
        metadata::decode(&encoded)
            .expect("fixture")
            .map(|(_, comment)| comment),
        Some(String::new())
    );
}
