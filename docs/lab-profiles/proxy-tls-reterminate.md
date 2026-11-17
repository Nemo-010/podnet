# Profile `proxy-tls-reterminate`

The host reaches out only through an HTTP `CONNECT` proxy. The proxy
terminates TLS for some names with its own CA, and passes TLS through for
the others. A `CONNECT` to port 22 gets 200, then the proxy sends no byte.

## The data (form of `pod-lab` at `93bf133a`)

```rust
pub(super) const TLS_RETERMINATED: &[Part] = &[
    net(
        "tls-reterminated",
        "the proxy shows its own CA for some names",
        &[
            needs("a TLS handshake through the proxy to a name on the list, whose chain ends at the CA of the proxy"),
            needs("a TLS handshake through the proxy to a name not on the list, whose chain ends at the CA of the server"),
        ],
    ),
    net(
        "ssh-port-silent",
        "CONNECT to port 22 gets 200, then no byte",
        &[needs("a CONNECT to port 22 that gets 200, then no byte in 5 s after the client sends its banner")],
    ),
];

// In PROFILES (table.rs), as now:
class("proxy-tls-reterminate", "the proxy shows its own CA for some names", "podssh, podnet", TLS_RETERMINATED),
```

The change to the table at `v0.56.0`: two observations in
`tls-reterminated` (the CA of the proxy for a listed name, the CA of the
server for another), a time in `ssh-port-silent`, and `podnet` in `source`.

## The parts and how a lab holds them

| Part | Observation | No root | User namespace | Root only | Mark |
| --- | --- | --- | --- | --- | --- |
| `tls-reterminated` | Two TLS handshakes through the proxy: the issuer of the leaf for a listed name is the CA of the lab; for another name it is the CA of the stand-in server. | A stand-in proxy process on `127.0.0.1:PORT` that terminates TLS for the listed names with a CA of the lab, and splices the others to stand-in TLS servers. `HTTPS_PROXY` names it. Landlock (ABI 4) allows a TCP connect to `PORT` only, so a direct connect to another port fails with `EACCES`. | A network namespace with `lo` up and no other interface: the proxy is the only way out. The stand-in proxy runs outside and listens on a unix socket; a forwarder in the namespace serves `127.0.0.1:3128` from it. | Not needed. | assumed: which names a real proxy re-terminates |
| `ssh-port-silent` | `CONNECT HOST:22` gets `HTTP/1.1 200`, then no byte comes back. | The same stand-in proxy: for port 22 it answers 200, reads, and sends nothing. | As no root. | Not needed. | measured for other ports (below); assumed for port 22 |

## What podnet does in the class

* The proxy shape "answers 200 and carries nothing" was measured on
  2026-11-01 for `CONNECT` to the edge on 7844 and to MASQUE
  `162.159.198.2:443` (`docs/target-environment.md`, A sandbox with an
  egress proxy). `podnet doctor` gives "the proxy answers and carries
  nothing: no TLS byte came back" (`src/doctor/tls_check.rs`, `tls`).
* For a re-terminated name, the API and DoH need the root of the proxy:
  `--ca-file` or `SSL_CERT_FILE` (`crates/podnet-core/src/trust.rs`;
  `tests/tls.rs`). With no extra root, the check of the certificate fails
  and the error names the issuer (`src/doctor/tls_check.rs`, `issuer_of`).
  Inferred, not measured on a real re-terminating proxy.
* The edge (`h2_sni()` of `pod-cloudflare`) and MASQUE need
  the Cloudflare chain (MASQUE also a client certificate). Through a proxy
  that re-terminates them, they cannot work (`docs/target-environment.md`,
  Other host conditions: inferred).

## The commands of podnet in the class

| Command | Expected |
| --- | --- |
| `podnet doctor` | Exit 0 or 1. The row of each re-terminated name gives its issuer and a failed check; a silent port gives "answers and carries nothing". No hang past the time limit. |
| `podnet --ca-file LAB-CA.pem doctor` | The API and DoH rows pass for the re-terminated names. |
| `podnet tunnel --quick` | Exit not 0, with the hop of the failure (the edge), in less than the time limit. |
