# RouterOS observations

These observations were collected on 2026-09-25 against a disposable QEMU/KVM
CHR running RouterOS **7.24.4 (stable)**, build `2026-09-16 11:32:21`.
The VM used a snapshot disk and loopback-only REST/DNS forwarding. No existing
router was used. These are observed results, not assumptions built into a fake.

| Experiment | Observed result |
| --- | --- |
| Create A with TTL `3600s` | REST returns `1h` |
| Create A with TTL `1w2d3h4m5s` | REST returns `1w2d3h4m5s` |
| Omit TTL | REST returns `1d` |
| Omit disabled | REST returns string `"false"` |
| Omit match-subdomain/address-list | Fields absent from REST response |
| Regexp instead of name | Accepted; `name` absent from response |
| Supply name and regexp together | HTTP 400: `only name or regexp allowed` |
| Unicode and delimiter characters in comment | Preserved by REST |
| 8,192-byte ASCII comment | Accepted and returned with exactly 8,192 bytes |
| Quoted ExternalDNS ownership TXT target | Surrounding quotes preserved by REST |
| Two A records with the same name and different targets | Both accepted; one real UDP DNS query returned both addresses |
| SRV target `.` with priority/weight/port `0` | HTTP 400: `bad SRV data` |
| Empty TXT text | Accepted and returned as an empty string |
| TTL `0s` | Accepted and returned as `0s` |
| disabled `yes` / match-subdomain `no` | Returned as `disabled: "true"`; false match-subdomain omitted |
| Empty address-list | Omitted from response |
| Name `Zero.Ttl.Rust-Qa.Test.` | Returned as `Zero.Ttl.Rust-Qa.Test`: trailing dot removed, case retained |
| CNAME target `Host.Example.Test.` | HTTP 400; `Host.Example.Test` accepted and case retained |
| SRV target `Srv.Example.Test.` | HTTP 400; `srv.example.test` with 10/20/443 accepted |
| GET filter `type=A,AAAA,CNAME,TXT,MX,SRV,NS` | Returns records across these types |
| TTL `1.5h` / MX preference `0010` | Returned as `1h30m` / `10` |
| Expanded IPv6 `2001:0db8:0000:0000:0000:0000:0000:0001` | Returned as `2001:db8::1` |
| TTL `00:01:30` | Accepted and returned as `1m30s` |

The 8 KiB experiment establishes one accepted size, **not a maximum comment
length**. No numeric comment limit was found in the official documentation.
The multi-target query establishes observed behavior on this release; it does
not promise answer ordering, round-robin scheduling, or behavior on all releases.

Official references:

- <https://manual.mikrotik.com/docs/developer-guides/rest-api/>
- <https://manual.mikrotik.com/docs/cli-reference/ip/dns/static/>
- <https://help.mikrotik.com/docs/spaces/ROS/pages/37748767/DNS>

The current manual cautions against overlapping name/type entries; the older
DNS guide describes ordering differently. Empirical results above are scoped
to the tested version and do not resolve every overlapping-record case.
