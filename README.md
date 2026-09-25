# ExternalDNS MikroTik Rust webhook

This is an independent Rust implementation of the ExternalDNS webhook contract for RouterOS REST.

## Configuration

`MIKROTIK_BASEURL`, `MIKROTIK_USERNAME`, `MIKROTIK_PASSWORD`, `MIKROTIK_SKIP_TLS_VERIFY`, and `MIKROTIK_CA_CERT` configure RouterOS. The base URL is an HTTP(S) origin; `/rest/ip/dns/static` is appended. `MIKROTIK_DEFAULT_TTL` defaults to `3600`; `MIKROTIK_DEFAULT_COMMENT` defaults to an empty string. The webhook binds `SERVER_HOST`/`SERVER_PORT` (defaults `localhost:8888`; set `SERVER_HOST=0.0.0.0` for containers). Health, readiness, and metrics bind `HEALTH_HOST`/`HEALTH_PORT` (defaults `0.0.0.0:8080`). Domain filters use `DOMAIN_FILTER`, `EXCLUDE_DOMAIN_FILTER`, `REGEXP_DOMAIN_FILTER`, and `REGEXP_DOMAIN_FILTER_EXCLUSION`.

Managed records carry an `edm1:` base64url JSON prefix in RouterOS `comment`; the suffix is the user's exact comment. This preserves provider-specific metadata and original TTL/name while allowing live RouterOS targets to remain authoritative. Untagged records are intentionally protected and ignored. A malformed or future metadata prefix fails closed.

The implementation supports A, AAAA, CNAME, TXT, MX, SRV, and NS. REST writes are serialized and use targeted `.id` deletes. No production RouterOS calls are made by unit tests.

`MIKROTIK_DEFAULT_COMMENT` is the default-comment variable. DNS names and domain targets are normalized by removing a terminal dot for RouterOS, while the original endpoint name is retained in metadata. RouterOS live targets remain authoritative, and live TTL/provider-field drift is surfaced rather than hidden. This project does not promise DNS round-robin scheduling.

Set `MIKROTIK_BASEURL`, `MIKROTIK_USERNAME`, and `MIKROTIK_PASSWORD`, then run `cargo run --release`. The default local webhook URL is `http://localhost:8888/`; configure ExternalDNS with `--provider=webhook` and `--webhook-provider-url=http://localhost:8888`. The provider accepts `application/external.dns.webhook+json;version=1` on webhook routes. Record migration is deliberate: existing untagged RouterOS entries are never exposed or deleted, and overlapping unmanaged name/type entries block creation. Manually remove or migrate an overlapping record before enabling a managed endpoint.

The versioned comment includes original provider-specific keys, effective/default TTL provenance, original endpoint name (including regexp entries), and the user comment. An explicit TTL of zero and an omitted TTL are indistinguishable in ExternalDNS; `/adjustendpoints` substitutes the configured default and carries a reserved marker so planner retries converge. For TXT registry ownership, labels are registry input and are not persisted as general RouterOS metadata. If RouterOS fields differ from the stored write baseline, the live value is surfaced. A malformed or future reserved prefix causes a read error; never rewrite these comments by hand. A batch is checked before writes; if RouterOS rejects a later operation, preceding operations remain applied and the next sync rereads live state. There is no automatic rollback or generic retry.

Run `cargo fmt --all -- --check`, `cargo test --all-targets --all-features`, `cargo clippy --all-targets --all-features -- -D warnings`, and `cargo build --release`. The separate [`conformance/`](conformance/) module drives the compiled binary through ExternalDNS's real planner and TXT registry against an isolated RouterOS REST fake; follow its README to run it. RouterOS-specific observations and their version scope are in [`docs/routeros-evidence.md`](docs/routeros-evidence.md). Licensed under Apache-2.0; the original Go provider is by its upstream contributors.
