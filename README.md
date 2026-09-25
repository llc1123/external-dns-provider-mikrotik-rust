# ExternalDNS MikroTik Rust Webhook

An independent Rust implementation of the [ExternalDNS webhook provider
contract](https://github.com/kubernetes-sigs/external-dns/blob/master/docs/tutorials/webhook-provider.md)
for the MikroTik RouterOS REST API.

This provider maps one ExternalDNS endpoint to one or more RouterOS static DNS
records. It keeps the endpoint's configuration provenance in a versioned
RouterOS `comment` envelope, so a read after a process restart can distinguish
provider metadata from the live RouterOS state without using a second metadata
store.

## Status

The project is intended for controlled evaluation and deployment. It has been
validated against:

- the ExternalDNS webhook client, planner, and TXT registry;
- a stateful RouterOS REST test server with failure injection; and
- a disposable RouterOS CHR `7.24.4` appliance.

The tested behavior and known boundaries are documented in
[`docs/compatibility.md`](docs/compatibility.md) and
[`docs/routeros-evidence.md`](docs/routeros-evidence.md).

## Features

- ExternalDNS webhook API version 1 negotiation.
- A, AAAA, CNAME, TXT, MX, SRV, and NS records.
- Multiple targets per ExternalDNS endpoint.
- Target-level reconciliation that preserves unchanged RouterOS record IDs.
- RouterOS REST create, update, list, and ID-based delete operations.
- Versioned `edm1:` comment metadata preserving:
  - the original endpoint DNS name;
  - original and effective TTL values;
  - provider-specific properties, including unknown properties;
  - explicit empty property values; and
  - the user's exact comment suffix.
- Live RouterOS target and provider-field values remain authoritative. Stored
  metadata does not hide physical drift.
- Untagged RouterOS records are protected from automatic adoption or deletion.
- Domain include, exclude, regular-expression include, and regular-expression
  exclude filters.
- TLS verification, custom CA certificates, request timeout, readiness, health,
  and Prometheus-compatible metrics endpoints.
- Deterministic preflight validation before destructive operations.

## Architecture

```text
ExternalDNS
    │ webhook v1 JSON
    ▼
Rust webhook ── endpoint conversion / metadata restore
    │
    ├── planner-compatible endpoint aggregation
    ├── target-level reconciliation
    └── serialized RouterOS REST writes
            │
            ▼
       MikroTik RouterOS
       /rest/ip/dns/static
```

The code deliberately keeps three representations separate:

1. **ExternalDNS endpoint**: one logical DNS name, type, and target set.
2. **RouterOS record**: one physical target with RouterOS-specific fields.
3. **Comment metadata**: persistent provenance needed to reconstruct the first
   representation after reading the second.

The reconciliation layer is necessary because ExternalDNS plans endpoint-level
changes while RouterOS stores one record per target. An update from `[A, B]` to
`[A, C]` therefore becomes one delete (`B`) and one create (`C`), leaving `A`
untouched.

## Metadata model

Managed comments have this shape:

```text
edm1:<base64url(JSON)>;<user-comment>
```

The JSON envelope is provider-owned and versioned. The suffix is the user's
comment and is preserved exactly. It contains the original logical endpoint
name, TTL provenance, provider-specific properties, and the effective values
written to RouterOS.

Only comments with a valid supported envelope are managed by this provider:

- an untagged record is ignored and protected;
- a malformed `edm1:` record fails closed;
- a future metadata version is not silently interpreted; and
- a foreign record that collides with a desired managed record blocks the write.

ExternalDNS's TXT registry remains responsible for ownership records. Registry
labels are not copied into general RouterOS metadata. An explicit TTL of zero
and an omitted TTL are indistinguishable on the ExternalDNS wire format; the
adjustment endpoint applies the configured default and records the provenance
marker.

## Requirements

- Rust `1.85` or newer.
- Cargo.
- A RouterOS device exposing its REST API. RouterOS 7.x is expected; verify the
  exact release-specific behavior before production deployment.
- ExternalDNS configured with the webhook provider.

The project also includes Go-based conformance tests. Those tests use the
ExternalDNS source checkout at `../../external-dns` by default, or an explicit
module replacement described in [`conformance/README.md`](conformance/README.md).

## Configuration

### RouterOS

| Variable | Required | Default | Description |
| --- | --- | --- | --- |
| `MIKROTIK_BASEURL` | yes | none | HTTP(S) origin, without credentials or path |
| `MIKROTIK_USERNAME` | yes | none | RouterOS REST username |
| `MIKROTIK_PASSWORD` | yes | none | RouterOS REST password |
| `MIKROTIK_SKIP_TLS_VERIFY` | no | `false` | Disable TLS certificate verification |
| `MIKROTIK_CA_CERT` | no | none | PEM CA certificate file |
| `MIKROTIK_DEFAULT_TTL` | no | `3600` | Default TTL in seconds |
| `MIKROTIK_DEFAULT_COMMENT` | no | empty | Default user comment |

`MIKROTIK_BASEURL` must be an origin such as
`https://router.example.invalid`. The provider appends
`/rest/ip/dns/static`; do not include that path in the variable.

### Webhook and health servers

| Variable | Required | Default | Description |
| --- | --- | --- | --- |
| `SERVER_HOST` | no | `localhost` | Webhook bind host |
| `SERVER_PORT` | no | `8888` | Webhook port |
| `HEALTH_HOST` | no | `0.0.0.0` | Health/readiness/metrics bind host |
| `HEALTH_PORT` | no | `8080` | Health/readiness/metrics port |
| `RUST_LOG` | no | `info` | `tracing` filter |

### Domain filters

| Variable | Description |
| --- | --- |
| `DOMAIN_FILTER` | Comma-separated included domains |
| `EXCLUDE_DOMAIN_FILTER` | Comma-separated excluded domains |
| `REGEXP_DOMAIN_FILTER` | Included domain regular expression |
| `REGEXP_DOMAIN_FILTER_EXCLUSION` | Excluded domain regular expression |

List filters and regular-expression filters are mutually exclusive. Invalid
regular expressions are rejected during startup.

## Running locally

```sh
export MIKROTIK_BASEURL="https://router.example.invalid"
export MIKROTIK_USERNAME="external-dns"
export MIKROTIK_PASSWORD="change-me"

cargo run --release
```

Configure ExternalDNS with the webhook provider URL:

```text
--provider=webhook
--webhook-provider-url=http://localhost:8888
```

The webhook routes require:

```text
application/external.dns.webhook+json;version=1
```

Operational endpoints are available on port `8080` by default:

```sh
curl http://localhost:8080/healthz
curl http://localhost:8080/readyz
curl http://localhost:8080/metrics
```

## Container image

```sh
docker build -t external-dns-provider-mikrotik-rust .
docker run --rm \
  -e MIKROTIK_BASEURL=https://router.example.invalid \
  -e MIKROTIK_USERNAME=external-dns \
  -e MIKROTIK_PASSWORD='change-me' \
  -p 8888:8888 \
  -p 8080:8080 \
  external-dns-provider-mikrotik-rust
```

For production, inject credentials through the platform's secret mechanism.
Do not put passwords in this repository, command history, manifests, or image
layers.

## Verification

Run the Rust checks from the project root:

```sh
cargo fmt --all -- --check
cargo test --all-targets --all-features
cargo clippy --all-targets --all-features -- -D warnings
cargo build --release
```

Run the ExternalDNS conformance suite:

```sh
cd conformance
RUST_BINARY="$PWD/../target/release/external-dns-provider-mikrotik" \
  go test -race -shuffle=on -count=1 ./...
```

The conformance suite drives the compiled process through the real ExternalDNS
planner and TXT registry. It verifies replay convergence, multi-target updates,
metadata changes, physical drift, foreign-record protection, all seven record
types, and deletion. An opt-in CHR command is documented in
[`conformance/README.md`](conformance/README.md); never point it at a production
router.

## Migration and operational boundaries

This project uses a new `edm1:` metadata format. Existing records created by
the Go provider are not automatically adopted. Before switching a deployment:

1. Back up the RouterOS configuration.
2. Inventory existing records and ownership TXT records.
3. Resolve name/type collisions with untagged records.
4. Choose and test an explicit migration procedure.
5. Start with a narrow domain filter and observe `/readyz`, logs, and RouterOS
   changes.

RouterOS REST operations are not transactional. A later write in a batch can
fail after earlier writes have succeeded. The provider returns the error without
inventing rollback or generic retries; the next ExternalDNS reconciliation
rereads RouterOS and works from observed state.

This project does not promise DNS answer ordering, round-robin scheduling, or
identical behavior across every RouterOS release. Release-specific observations
are recorded in [`docs/routeros-evidence.md`](docs/routeros-evidence.md).

## Project layout

| Path | Purpose |
| --- | --- |
| `src/app.rs` | Webhook routes and HTTP protocol handling |
| `src/model.rs`, `src/restore.rs` | Endpoint/RouterOS conversion |
| `src/metadata.rs` | Versioned comment metadata codec |
| `src/reconcile.rs` | Preflight validation and target-level changes |
| `src/routeros.rs` | RouterOS REST client |
| `tests/` | Rust unit and HTTP tests |
| `conformance/` | Real ExternalDNS process-level conformance tests |
| `docs/` | Compatibility, migration, and appliance evidence |

## License and attribution

Licensed under Apache-2.0. The design is informed by the upstream Go provider
and its ExternalDNS integration, while this implementation is an independent
Rust project. See [`LICENSE`](LICENSE).
