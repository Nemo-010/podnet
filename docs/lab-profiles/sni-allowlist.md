# Profile `sni-allowlist`

The host has no proxy. A filter of the network reads the server name (SNI)
of each TLS ClientHello. A ClientHello with a name that is not on the list,
or with no name, gets a TCP reset.

## The data (form of `pod-lab` at `93bf133a`)

```rust
pub(super) const SNI: &[Part] = &[
    net(
        "sni-reset",
        "a ClientHello with a name not on the list, or with no name, is reset",
        &[
            needs("a ClientHello with a name on the list that gets a ServerHello"),
            needs("a ClientHello with a name not on the list, and one with no name, that the network resets"),
        ],
    ),
];

// In PROFILES (table.rs), as now:
class("sni-allowlist", "no proxy; TLS is reset for a name not on the list", "podssh, podnet", SNI),
```

The change to the table at `v0.56.0`: a first observation (a listed name
passes), so that a lab that resets each ClientHello fails its check; and
`podnet` in `source`.

## The parts and how a lab holds them

| Part | Observation | No root | User namespace | Root only | Mark |
| --- | --- | --- | --- | --- | --- |
| `sni-reset` | A ClientHello to `good.lab:443` gets a ServerHello; one to `bad.lab:443` and one with no SNI (to an IP literal) get a reset. | Not possible: no root process can see the direct connections of another process without a proxy. | A network namespace with `lo` up. `nf_tables` in the namespace (as the `netns-kernel` backend of podnet: `crates/podnet-netns`, `docs/pods.md`) redirects each TCP connect to port 443 to a stand-in filter, which reads the ClientHello, resets a name that is not on the list, and splices a listed name to a stand-in TLS server in the namespace. The stand-in servers have addresses on `lo`. | A host with no `nf_tables` in a user namespace: the same rules in the host namespace. | assumed (no measured host of podnet has this filter) |

## What podnet does in the class

* DoH by IP address (`src/resolve.rs`, step 4) sends a ClientHello with no
  name (TLS checks the IP address of the certificate). In this class it is
  reset, and the chain goes on to the next server. Inferred.
* The edge leg over HTTP/2 sends the SNI of `h2_sni()` (`src/edge_h2.rs`,
  `src/cloudflare/`); QUIC sends its own name. Only a list that has these
  names lets a tunnel start. Inferred.
* `podnet doctor` gives a TLS row for each target (`src/doctor/tls_check.rs`);
  a reset is a failed row, not a hang. Inferred.

## The commands of podnet in the class

| Command | Expected |
| --- | --- |
| `podnet doctor` | Exit 0 or 1; the rows of the names that are not on the list fail with a reset, in the time limit. |
| `podnet tunnel --quick` | Starts only when the list has the names of the API and the edge; else exit not 0 with the hop. |
