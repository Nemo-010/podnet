# Profile `proxy-per-name`

The host reaches out only through an HTTP `CONNECT` proxy. The proxy answers
each target in its own way: 200 for one name, 504 (or a reset) for another,
a reset for an IP literal.

## The data (form of `pod-lab` at `93bf133a`)

```rust
pub(super) const PER_NAME: &[Part] = &[
    net(
        "answer-per-name",
        "the proxy answers 200 for one name and 504 for another",
        &[
            needs("two CONNECT requests that get 200 for one name and 504 for another"),
            needs("a CONNECT to an IP literal that the proxy resets"),
        ],
    ),
];

// In PROFILES (table.rs), as now:
class("proxy-per-name", "the proxy answers each name in its own way", "podssh, podnet", PER_NAME),
```

The change to the table at `v0.56.0`: a second observation (the reset of
an IP literal, which podnet measured), and `podnet` in `source`.

## The parts and how a lab holds them

| Part | Observation | No root | User namespace | Root only | Mark |
| --- | --- | --- | --- | --- | --- |
| `answer-per-name` | `CONNECT good.lab:443` gets 200; `CONNECT bad.lab:443` gets 504; `CONNECT 192.0.2.1:443` gets a reset (the proxy closes with no answer). | A stand-in proxy process with a table: target to answer (200 and a splice to a stand-in server, 504, a reset). `HTTPS_PROXY` names it. Landlock (ABI 4) allows a TCP connect to the port of the proxy only. | A network namespace with `lo` up and no other interface; the stand-in proxy outside on a unix socket, with a forwarder to `127.0.0.1:3128` in the namespace. | Not needed. | measured (podnet, below); the 504 for a name: assumed (the class of podssh) |

## What podnet does in the class

* Measured on 2026-11-01 (`docs/target-environment.md`, A sandbox with an
  egress proxy): `CONNECT 1.1.1.1:443` is reset; `CONNECT
  cloudflare-dns.com:443` works; `CONNECT 198.41.200.x:7844` is reset.
* When the proxy refuses `CONNECT` to an IP address, DoH goes by name
  (`cloudflare-dns.com`, `dns.google`, `dns.google.com`) through the proxy
  (`src/resolve.rs`, `settings`, `pod_dns::Names::Last`). Measured.
* A 502, 503 or 504 of the proxy names the destination, not the proxy:
  "the proxy answered HTTP 504: it could not reach the destination"
  (`src/report.rs`, `error_line`; the test `a_502_of_the_proxy_names_the_destination`).
  Inferred (test).
* A refusal of the proxy is a `Refusal` with its status (`src/egress.rs`,
  `refusal`), and `podnet doctor` gives one row for each target.

## The commands of podnet in the class

| Command | Expected |
| --- | --- |
| `podnet doctor` | Exit 0 or 1; each target row gives the answer of the proxy for its name. |
| `podnet tunnel --quick` | DoH by name through the proxy; the edge leg that gets 504 names the destination. |
| `podnet send FILE` | Works when the name of the service gets 200. |
