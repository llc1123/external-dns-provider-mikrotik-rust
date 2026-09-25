# ExternalDNS/MikroTik conformance

This module drives a compiled Rust webhook through the real ExternalDNS webhook
client, TXT registry, and planner. It uses an in-process stateful RouterOS REST
fake; no RouterOS appliance or other external infrastructure is required.

From the Rust checkout root:

```sh
cargo build --release
cd conformance
RUST_BINARY="$PWD/../target/release/external-dns-provider-mikrotik" \
  go test -race -shuffle=on -count=1 ./...
```

`RUST_BINARY` may point at another compiled executable. The suite allocates both
HTTP ports dynamically and passes `MIKROTIK_BASEURL`, `MIKROTIK_USERNAME`,
`MIKROTIK_PASSWORD`, `SERVER_HOST`, and `SERVER_PORT` to the child process.
The committed `replace` requires the sibling `../../external-dns` checkout at
commit `446dcf3f`; CI checks out both repositories in that layout. An unset
`RUST_BINARY` fails the test rather than skipping conformance.

For an explicitly opt-in RouterOS/CHR run, set all three variables below. The
suite refuses partial credentials and scopes records to a random
`conformance-<nonce>.example.org` name with a matching unique TXT owner:

```sh
CONFORMANCE_ROUTEROS_BASEURL=https://router.example.test \
CONFORMANCE_ROUTEROS_USERNAME=external-dns \
CONFORMANCE_ROUTEROS_PASSWORD='secret' \
RUST_BINARY="$PWD/../target/release/external-dns-provider-mikrotik" \
go test -race -shuffle=on -count=1 ./...
```

With no `CONFORMANCE_ROUTEROS_*` variables, an isolated in-process REST fake is
used. Partial variables are rejected rather than silently falling back.
The CHR run deletes only its own unique records; never point it at a production router.

The committed relative `replace` runs against sibling `../../external-dns` for
latest-source compatibility. To exercise the downloaded v0.23.0 tag without
changing the committed module, run from `conformance/`:

```sh
tmp_dir=$(mktemp -d)
cp go.mod "$tmp_dir/tagged.mod"
cp go.sum "$tmp_dir/tagged.sum"
go mod edit -modfile "$tmp_dir/tagged.mod" -dropreplace=sigs.k8s.io/external-dns
RUST_BINARY="$PWD/../target/release/external-dns-provider-mikrotik" \
  go test -mod=mod -modfile "$tmp_dir/tagged.mod" -race -shuffle=on -count=1 ./...
rm -rf "$tmp_dir"
```
