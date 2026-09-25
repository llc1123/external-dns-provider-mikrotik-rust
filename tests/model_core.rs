#![allow(clippy::unwrap_used, reason = "model fixtures")]

use external_dns_provider_mikrotik::model::{
    Endpoint, ORIGINAL_TTL_PROPERTY, ProviderSpecific, endpoint_to_records, record_to_endpoint,
};

#[test]
fn all_supported_types_round_trip_targets() {
    for (record_type, target) in [
        ("A", "192.0.2.1"),
        ("AAAA", "2001:db8::1"),
        ("CNAME", "target.example."),
        ("TXT", "hello"),
        ("NS", "ns.example."),
        ("MX", "10 mail.example."),
        ("SRV", "1 2 443 service.example."),
    ] {
        let endpoint = Endpoint {
            dns_name: Some("x.example.".into()),
            record_type: record_type.into(),
            record_ttl: 60,
            targets: vec![target.into()],
            ..Endpoint::default()
        };
        let restored =
            record_to_endpoint(&endpoint_to_records(&endpoint, 3600, None).unwrap()[0]).unwrap();
        let expected: String = if record_type == "TXT" {
            target.to_owned()
        } else {
            target.trim_end_matches('.').to_owned()
        };
        assert_eq!(restored.targets, vec![expected]);
    }
}

#[test]
fn zero_ttl_marker_survives_adjustment() {
    let endpoint = Endpoint {
        dns_name: Some("x".into()),
        record_type: "A".into(),
        record_ttl: 3600,
        targets: vec!["192.0.2.1".into()],
        provider_specific: vec![ProviderSpecific {
            name: ORIGINAL_TTL_PROPERTY.into(),
            value: "0".into(),
        }],
        ..Endpoint::default()
    };
    assert_eq!(
        endpoint_to_records(&endpoint, 3600, None).unwrap()[0].ttl,
        "1h"
    );
}
