# Profile `no-proxy-no-dns`

The host has no proxy and no resolver. A lookup of a name fails. A direct
TCP connect to an IP literal reaches out.

## The data (form of `pod-lab` at `93bf133a`)

```rust
pub(super) const NO_DNS: &[Part] = &[
    net(
        "no-resolver",
        "a lookup of a name fails",
        &[
            needs("a lookup of a name with getaddrinfo, with no proxy, that fails"),
            needs("a DNS query on UDP 53 and on TCP 53 to the server of /etc/resolv.conf that gets no answer"),
        ],
    ),
    net(
        "literal-reaches",
        "a TCP connect to an IP literal reaches out",
        &[needs("a TCP connect to an IP literal outside, and a TLS handshake to 1.1.1.1:443 that checks the IP address")],
    ),
];

// In PROFILES (table.rs), as now:
class("no-proxy-no-dns", "no proxy and no resolver; only IP literals connect", "podssh, podnet", NO_DNS),
```

The change to the table at `v0.56.0`: a second observation of `no-resolver`
(the server of `/etc/resolv.conf` gives no answer), the TLS handshake by IP
address in `literal-reaches`, and `podnet` in `source`.

## The parts and how a lab holds them

| Part | Observation | No root | User namespace | Root only | Mark |
| --- | --- | --- | --- | --- | --- |
| `no-resolver` | `getaddrinfo("example.com")` fails; a DNS query to the server of `/etc/resolv.conf` gets no answer. | Not complete: seccomp can refuse UDP (`udp-denied`), but not a TCP connect to port 53 only; Landlock (ABI 4) can refuse each TCP connect except listed ports (443), which also refuses TCP 53. With both: no lookup goes out. | A mount namespace binds an `/etc/resolv.conf` with no `nameserver` line, and `/etc/hosts` with `localhost` only. With a network namespace, no server is reachable. | Not needed. | measured (podnet, below) |
| `literal-reaches` | A TCP connect to an IP literal gets a connection; a TLS handshake to `1.1.1.1:443` checks the IP address in the certificate. | The host gives it (no namespace). Landlock allows port 443. | A network namespace with a stand-in TLS server at an address on `lo` (`192.0.2.1`, a certificate with that IP address and a CA of the lab). Outside reach needs root. | The route out of the namespace (NAT). | measured on a runner (below); the lab stand-in: assumed |

## What podnet does in the class

* Measured on a GitHub runner (2026-10-31 and 2026-11-01,
  `docs/target-environment.md`): with no system resolver, names come from DoH
  by IP address at `1.1.1.1`; with `1.1.1.1` and `1.0.0.1` blocked, from
  `8.8.8.8`.
* The chain of `src/resolve.rs`: an IP literal, the pins (`--resolve`,
  `PODNET_RESOLVE`), the system resolver with a limit of 5 s
  (`STEP_TIMEOUT`), then DoH by IP address (Cloudflare, then Google; IPv6
  first on a host with no IPv4 default route).
* With no answer of DoH, the edge discovery uses a static edge list
  (`docs/target-environment.md`, Limits). Inferred.

## The commands of podnet in the class

| Command | Expected |
| --- | --- |
| `podnet doctor` | Exit 0 or 1; the resolver row gives DoH by IP address. The run takes the 5 s of the system resolver at most once for each name. |
| `podnet tunnel --quick` | Names from DoH by IP address; the tunnel starts where the edge is reachable. |
| `podnet --resolve NAME=IP doctor` | The pin is used; no lookup goes out for NAME. |
