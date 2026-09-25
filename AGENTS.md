# ExternalDNS MikroTik Rust Webhook

## Project purpose

This repository is an independent Rust implementation of an ExternalDNS
webhook provider for MikroTik RouterOS REST. It converts ExternalDNS logical
endpoints into RouterOS static DNS records and reconstructs endpoints from live
RouterOS state.

The provider stores endpoint provenance in a versioned `edm1:` envelope in the
RouterOS `comment` field. The envelope preserves the original DNS name, TTL
provenance, provider-specific properties, and the user's comment. Live RouterOS
targets and provider fields remain authoritative so metadata cannot conceal
physical drift.

## Repository map

- `src/`: Rust service, protocol, conversion, metadata, reconciliation, and REST
  client code.
- `tests/`: Rust unit and HTTP tests.
- `conformance/`: Go tests using the real ExternalDNS webhook client, TXT
  registry, and planner against the compiled Rust binary.
- `docs/`: compatibility notes, migration guidance, and version-scoped
  RouterOS observations.
- `.github/workflows/ci.yml`: formatting, tests, clippy, release build, and
  ExternalDNS conformance checks.

## Development rules

### Preserve the protocol boundary

- Keep the exact webhook media type:
  `application/external.dns.webhook+json;version=1`.
- Keep ExternalDNS JSON field names and null/empty semantics compatible with the
  real client.
- Run the process-level conformance suite when changing routes, endpoint
  conversion, defaults, labels, or planner behavior.

### Preserve metadata safety

- Do not use a second hidden metadata store in place of the comment envelope.
- Do not persist ExternalDNS TXT-registry ownership labels as general provider
  metadata.
- Do not silently adopt, alter, or delete untagged RouterOS records.
- Reject malformed or future metadata rather than guessing its meaning.
- Keep user comments byte-for-byte intact after the metadata suffix.
- Treat live RouterOS values as authoritative when detecting drift.

### Preserve reconciliation behavior

- ExternalDNS changes are endpoint-level; RouterOS records are target-level.
- Target-only changes must preserve IDs for unchanged targets.
- Validate the complete change batch before destructive calls.
- Do not add generic retries or pretend RouterOS REST is transactional.
- After a partial write, the next reconciliation must reread RouterOS and
  converge from observed state.

### Code quality

- Use stable Rust and keep `cargo fmt`, `cargo test`, and clippy with warnings
  denied green.
- Do not use `unsafe`, `unwrap`, `expect`, `panic`, or type-error suppressions.
- Keep modules focused and avoid introducing abstractions without a demonstrated
  protocol or domain need.
- Never log RouterOS credentials or include them in tests, fixtures, commits, or
  documentation.
- Add regression coverage for every change to metadata, conversion, filtering,
  RouterOS field mapping, or reconciliation.

## Required verification

```sh
cargo fmt --all -- --check
cargo test --all-targets --all-features
cargo clippy --all-targets --all-features -- -D warnings
cargo build --release

cd conformance
RUST_BINARY="$PWD/../target/release/external-dns-provider-mikrotik" \
  go test -race -shuffle=on -count=1 ./...
```

Use the disposable CHR procedure in `conformance/README.md` only when testing
RouterOS-specific behavior. Never use production credentials or endpoints in
automated tests.

## Git workflow

These repository rules are mandatory:

1. **Every new feature or behavior change must start on a new branch.** Do not
   develop features directly on `main`.
2. **Every change must be submitted through a pull request.** Do not push
   feature branches directly into `main`.
3. **Pull requests must be squash-merged.** The resulting `main` history should
   contain one purposeful commit per pull request.
4. **Commit messages must be English Conventional Commits**, for example:
   - `feat: preserve endpoint metadata`
   - `fix: clear stale RouterOS fields`
   - `test: add planner convergence coverage`
   - `docs: document migration boundaries`
   - `ci: pin ExternalDNS checkout`
5. Keep commits focused and reviewable. Do not mix unrelated refactors,
   generated output, credentials, or local build artifacts into a pull request.
6. Before opening a pull request, run the required verification commands and
   report any unavailable external validation explicitly.

For documentation-only changes, use the same branch and pull request workflow;
the rule is about repository history and reviewability, not only executable
code.
