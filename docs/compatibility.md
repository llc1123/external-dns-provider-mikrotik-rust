# Compatibility and verification scope

Reference implementations:

- MikroTik Go provider: upstream `244ab9177a99a11a6ace4653cbdd3a4ddfe0db33`.
- ExternalDNS local source: `446dcf3f` (after v0.23.0).
- Original design discussion: <https://github.com/mirceanton/external-dns-provider-mikrotik/pull/292>.

## Required behavioral boundaries

The Rust implementation keeps the ExternalDNS webhook protocol and the RouterOS
one-record-per-target mapping. Its deliberate design difference is persistent
configuration provenance in the RouterOS comment. A user comment remains data
inside that representation; it is not an ownership identifier supplied by users.

| Boundary | Verification requirement |
| --- | --- |
| Webhook negotiation | Real ExternalDNS client accepts the exact v1 media type and domain filter |
| Absent versus explicit properties | Omitted, empty and explicit default values survive writes and reads |
| Planner convergence | After creation, two subsequent real planner cycles produce no changes |
| Targets | Reading aggregates records; updates retain IDs for unchanged targets |
| Physical drift | Changes to real RouterOS fields are visible; provenance cannot conceal them |
| Ownership | ExternalDNS TXT registry remains responsible for instance ownership |
| Foreign records | Untagged RouterOS records are never silently adopted or deleted |
| Invalid batch | Invalid desired data is rejected before any destructive write |
| Partial failure | Error reaches ExternalDNS; observed state drives the next reconciliation |
| Restart | Metadata is read from RouterOS; process memory is not the source of truth |

ExternalDNS serializes TTL zero with `omitempty`; the webhook cannot distinguish
an explicitly supplied zero from an omitted TTL. Provider-specific properties
are different: an entry with an empty value is still a present entry, and that
distinction must be retained.

ExternalDNS's planner does not update an unconfigured desired TTL. Consequently,
default-TTL drift needs deliberate handling at the adjustment/comparison boundary.
An `ApplyChanges` implementation that merely skips redundant writes does not prove
planner convergence.

## Tests versus appliance evidence

Rust unit and HTTP tests check provider behavior. The Go conformance module uses
the real ExternalDNS client, planner and TXT registry with a real Rust process;
its stateful REST test server provides deterministic fault injection. This does
not by itself prove RouterOS behavior. Appliance observations and their tested
version are recorded separately in [routeros-evidence.md](routeros-evidence.md).

## Existing installations

This rewrite uses a different persistent comment format. Existing records made
by the Go provider do not automatically become managed records. Introducing it
to an existing router therefore requires an explicit migration decision; none
of the test or build commands migrates existing records. Back up RouterOS state
and resolve name/ownership collisions before replacing a deployed provider.
