//! UDP egress: the probe and the choice are pod-cloudflare's; the variables
//! of podnet are here.
//!
//! A bind of a UDP socket does not show that a datagram can leave. The probe
//! sends one QUIC packet with a reserved version (RFC 9000 section 15) to an
//! edge address on port 7844. A QUIC server answers it with a Version
//! Negotiation packet. The result is one of three:
//!
//! * answered: a datagram came back;
//! * refused: the send failed, or the host or the network refused the
//!   datagram (an ICMP error on the connected socket);
//! * unknown: no answer before the deadline. A firewall that drops UDP gives
//!   this result, and so does a lost packet.
//!
//! `PODNET_UDP` selects what a tunnel does with UDP: `off` (no QUIC on UDP
//! 7844), `on` (no probe), or `probe` (the default: a short probe; the
//! `quic` leg of `--protocol auto` is skipped only when the result is
//! refused).

use std::net::SocketAddr;

use anyhow::Result;

pub use pod_cloudflare::udp::*;

/// The choice of `PODNET_UDP`; `probe` when it is not set.
pub fn choice_from_env() -> Result<Choice> {
    match std::env::var("PODNET_UDP") {
        Ok(value) => Choice::parse(&value).map_err(|_| {
            crate::exit::fault(
                crate::exit::Fault::Config,
                anyhow::anyhow!(
                    "PODNET_UDP={:?}: give off, on or probe",
                    value.trim().to_ascii_lowercase()
                ),
            )
        }),
        Err(_) => Ok(Choice::Probe),
    }
}

/// Check `PODNET_EDGE` as `tunnel` does: each entry must be ip:port. The
/// doctors call it, so a bad value is exit 78 there too (issue 66).
pub fn check_edge_env() -> Result<()> {
    let Ok(list) = std::env::var("PODNET_EDGE") else {
        return Ok(());
    };
    for item in list.split(',').map(str::trim).filter(|i| !i.is_empty()) {
        if item.parse::<SocketAddr>().is_err() {
            return Err(crate::exit::fault(
                crate::exit::Fault::Config,
                anyhow::anyhow!("PODNET_EDGE entry {item:?} is not ip:port"),
            ));
        }
    }
    Ok(())
}

/// The target of the probe: the first address of `PODNET_EDGE`, else
/// [`default_probe_target`].
pub fn probe_target() -> SocketAddr {
    std::env::var("PODNET_EDGE")
        .ok()
        .and_then(|list| {
            list.split(',')
                .map(str::trim)
                .find_map(|item| item.parse().ok())
        })
        .unwrap_or_else(default_probe_target)
}

/// The words of podnet in the texts of pod-cloudflare: its name, its flags
/// and its variables.
pub fn words() -> pod_cloudflare::Words {
    pod_cloudflare::Words {
        name: "podnet".into(),
        protocol: "--protocol".into(),
        udp_off: "PODNET_UDP=off".into(),
        strict: "Set PODNET_VERIFY_STRICT=1 to make this an error".into(),
        resolve: "give the address with --resolve HOST=IP, or use --no-verify".into(),
        tcp_origin: "Use --forward with \"podnet connect\", or --port for an HTTP origin.".into(),
    }
}
