# Profile `ws-upgrade-refused`

The host reaches out only through an HTTP proxy, and the proxy refuses the
WebSocket upgrade (`Upgrade: websocket`). A plain HTTPS request through the
proxy works.

## The data (form of `pod-lab` at `93bf133a`)

```rust
pub(super) const WS_REFUSED: &[Part] = &[
    net(
        "upgrade-refused",
        "the proxy refuses the WebSocket upgrade",
        &[
            needs("an HTTPS GET through the proxy that gets 200"),
            needs("a WebSocket upgrade through the proxy that gets a status other than 101, or a close"),
        ],
    ),
];

// In PROFILES (table.rs), as now:
class("ws-upgrade-refused", "the proxy refuses the WebSocket upgrade", "podssh, podnet", WS_REFUSED),
```

The change to the table at `v0.56.0`: a first observation (a plain request
passes), so that a lab that refuses each request fails its check; and
`podnet` in `source`.

## The parts and how a lab holds them

| Part | Observation | No root | User namespace | Root only | Mark |
| --- | --- | --- | --- | --- | --- |
| `upgrade-refused` | `GET /` to `good.lab` through the proxy gets 200; the same request with `Connection: Upgrade` and `Upgrade: websocket` gets 403 (or the proxy closes). | A stand-in proxy process that re-terminates TLS for the stand-in server with a CA of the lab (the `tls-reterminated` stand-in of `proxy-tls-reterminate.md`), reads the request, and refuses `Upgrade`. The client trusts the CA of the lab. Landlock (ABI 4) allows the port of the proxy only. A plain HTTP proxy (no TLS) can refuse `Upgrade` with no CA. | A network namespace with `lo` up; the stand-in outside on a unix socket. | Not needed. | assumed (no measured host of podnet refused the upgrade) |

A proxy can see the upgrade inside `CONNECT` only when it terminates TLS. So
the class needs a re-terminating proxy, or a plain HTTP proxy for `ws://`.

## What podnet does in the class

* `podnet doctor` has the lane `ws_upgrade`: an HTTP/1.1 upgrade to the edge
  host over TLS, through the proxy when one is set. `ok` on 101; `fail` on
  another status, a TLS fault or a refusal; `unknown` with no answer in two
  time limits (`src/doctor/lane_probes.rs`, `ws_upgrade`; the tests in
  `src/doctor/lane_probes/tests.rs`). Inferred (test).
* The relay (`podnet relay`, `--egress relay`) carries its bytes in a
  WebSocket (`docs/relay.md`). In this class the relay cannot work; the
  tunnel legs that need no upgrade (HTTP/2 on 7844, `quic+warp`) stay.
  Inferred.

## The commands of podnet in the class

| Command | Expected |
| --- | --- |
| `podnet doctor` | The lane `ws_upgrade` is `fail` with the status of the proxy. |
| `podnet relay node --to 127.0.0.1:22` | Exit not 0 with the status of the upgrade, no loop of retries with no message. |
| `podnet tunnel --quick` | A leg with no upgrade is used. |
