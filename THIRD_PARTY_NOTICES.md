# Third-party notices

`podnet` is 0BSD (see `LICENSE`). The binary holds these files of the crate
`pod-cloudflare` (PodUtils/podcrates). They are from `cloudflared`
(<https://github.com/cloudflare/cloudflared>), release 2026.10.7
(`cloudflare/cloudflared-pin.txt` of the crate), and have the Apache License
2.0:

| Path in `pod-cloudflare` | Source in cloudflared |
| --- | --- |
| `cloudflare/schemas/tunnelrpc.capnp` | `tunnelrpc/proto/tunnelrpc.capnp` |
| `cloudflare/schemas/quic_metadata_protocol.capnp` | `tunnelrpc/proto/quic_metadata_protocol.capnp` |
| `cloudflare/cf-edge-roots.pem` | `tlsconfig/cloudflare_ca.go` (`cloudflareRootCA`) |
| `tools/header-oracle.txt`, the Go program in `tools/check-header-oracle.sh` | `connection/header.go` (`SerializeHeaders`) |

The schemas are the wire format. The crate does not compile them: the layouts
in its `cloudflare/schema.rs` follow from them.

The Rust dependencies are listed in `Cargo.lock`. Each has its own license.
Each release has `THIRD-PARTY-LICENSES.md`, which holds the license text of
each crate in the binaries. The `notes` job of `.github/workflows/release.yml`
makes it with `cargo about generate --locked` from `about.toml` and
`about.hbs`. A crate with a license that `about.toml` does not accept stops
the release.
