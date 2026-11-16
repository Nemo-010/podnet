//! The edge addresses and the WARP session of the `quic+warp` leg.

use std::sync::Arc;
use std::time::Duration;

use anyhow::{bail, Context, Result};
use tracing::{info, warn};

use super::{edge, edge_override, IpVersionFilter, WarpClient};

pub(super) async fn warp_targets(
    identity: Option<&std::path::Path>,
    masque_endpoint: Option<std::net::SocketAddr>,
) -> Result<(Vec<edge::EdgeAddr>, Arc<WarpLink>, noq::Endpoint)> {
    let discovered = match edge_override()? {
        Some(list) => list,
        None => crate::edge_discovery::discover(IpVersionFilter::V4Only)
            .await
            .context("edge discovery")?,
    };
    let Some(first) = discovered.first() else {
        bail!("edge discovery returned no addresses");
    };
    let client = match identity {
        Some(path) => WarpClient::register_or_load(path)
            .await
            .context("registering with WARP")?,
        None => WarpClient::register()
            .await
            .context("registering with WARP")?,
    };
    let client = client.with_masque(crate::warp::MasqueOptions {
        endpoint: masque_endpoint,
        ..Default::default()
    });
    let stack = client
        .connect()
        .await
        .context("opening the WARP MASQUE tunnel on TCP 443")?;
    let endpoint = crate::warp::quic::endpoint(&stack)
        .await
        .context("building the QUIC socket inside the WARP session")?;
    info!(
        edge = %first.socket(),
        "the tunnel edge is reached through WARP on TCP 443"
    );
    let link = Arc::new(WarpLink {
        client,
        current: tokio::sync::Mutex::new((stack, endpoint.clone(), std::time::Instant::now())),
    });
    Ok((discovered, link, endpoint))
}

/// The WARP session of the `quic+warp` leg, and the QUIC endpoint in it.
/// When the session closes, or a reconnect fails twice, the next reconnect
/// opens a new session with the same device (no new registration).
pub(crate) struct WarpLink {
    pub(super) client: WarpClient,
    pub(super) current:
        tokio::sync::Mutex<(crate::warp::WarpStack, noq::Endpoint, std::time::Instant)>,
}

/// A new session at most this often for failed reconnects (an HA leg must
/// not replace the session that another leg just opened).
const WARP_REBUILD_GAP: Duration = Duration::from_secs(10);

impl WarpLink {
    async fn endpoint(&self, stale: bool) -> Result<noq::Endpoint> {
        let mut current = self.current.lock().await;
        let closed = current.0.is_closed();
        if closed || (stale && current.2.elapsed() >= WARP_REBUILD_GAP) {
            warn!(
                session_closed = closed,
                "opening a new WARP session for the edge leg"
            );
            let stack = self
                .client
                .connect()
                .await
                .context("opening the WARP MASQUE tunnel on TCP 443 again")?;
            let endpoint = crate::warp::quic::endpoint(&stack)
                .await
                .context("building the QUIC socket inside the new WARP session")?;
            *current = (stack, endpoint, std::time::Instant::now());
        }
        Ok(current.1.clone())
    }
}

/// Where the QUIC reactor gets its endpoint for each reconnect.
#[derive(Clone)]
pub(super) enum EndpointSource {
    /// A kernel UDP socket: always the same endpoint.
    Fixed(noq::Endpoint),
    /// The WARP session: a new one when it closed (see [`WarpLink`]).
    Warp(Arc<WarpLink>),
}

impl EndpointSource {
    /// The endpoint for reconnect attempt `attempt` (1 for the first).
    pub(super) async fn endpoint(&self, attempt: u32) -> Result<noq::Endpoint> {
        match self {
            EndpointSource::Fixed(endpoint) => Ok(endpoint.clone()),
            EndpointSource::Warp(link) => link.endpoint(attempt >= 3).await,
        }
    }

    /// Ends when the WARP session under the endpoint has closed (T-402); a
    /// kernel socket never closes this way. The session is read each 2 s.
    pub(super) async fn closed(&self) {
        let EndpointSource::Warp(link) = self else {
            return std::future::pending().await;
        };
        loop {
            tokio::time::sleep(WARP_WATCH).await;
            if link.current.try_lock().is_ok_and(|c| c.0.is_closed()) {
                return;
            }
        }
    }
}

/// How often a leg reads its WARP session (T-402).
const WARP_WATCH: Duration = Duration::from_secs(2);
