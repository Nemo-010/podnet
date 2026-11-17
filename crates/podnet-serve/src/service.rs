//! The services of podnet for the rules of one ingress: each one is built
//! once, when the tunnel starts (pod-cloudflare calls `prepare` for each
//! rule with a static or a custom service). The axum routers answer the
//! requests; the edge side is pod-cloudflare's.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use anyhow::Result;
use axum::Router;

use crate::gate::Gate;
use crate::ingress::{Podnet, Service};
use crate::share::{ShareApp, ShareControl, ShareMode};
use podnet_core::metrics::Metrics;

pub use crate::files::{static_router, static_router_with};

/// A service prepared for dispatch.
pub enum Built {
    Static(Router),
    Share(ShareApp),
    /// quickbridge proxy mode: gated forwarding to a local HTTP port.
    ShareProxy {
        target: String,
        gate: Gate,
        control: Arc<ShareControl>,
        /// Stop the session after the first request that the origin
        /// answered (`--stop-after`).
        stop_after: bool,
    },
}

impl Built {
    /// Build the service of podnet in `service`. `None` for a service that
    /// pod-cloudflare serves itself.
    pub fn build(service: &Service) -> Result<Option<Self>> {
        if let Service::Static { root, spa, tar } = service {
            return Ok(Some(Built::Static(static_router_with(root, *spa, *tar)?)));
        }
        let Some(podnet) = crate::ingress::podnet(service) else {
            return Ok(None);
        };
        Ok(Some(match podnet {
            Podnet::Files(files) => Built::Static(crate::files::router(files)?),
            Podnet::Git(git) => Built::Static(crate::git::http::router(git.clone())?),
            Podnet::Share(config) => match config.mode {
                ShareMode::Proxy => Built::ShareProxy {
                    target: config
                        .target
                        .clone()
                        .unwrap_or_else(|| "127.0.0.1:80".to_string()),
                    gate: config.gate.clone(),
                    control: Arc::new(ShareControl::default()),
                    stop_after: config.stop_after,
                },
                _ => Built::Share(ShareApp::new(config.clone())?),
            },
        }))
    }
}

/// The services of the rules of one ingress, by the index of the rule.
pub struct ServiceRuntime {
    built: Mutex<BTreeMap<usize, Arc<Built>>>,
    metrics: Metrics,
}

impl ServiceRuntime {
    pub fn new(metrics: Metrics) -> Self {
        Self {
            built: Mutex::new(BTreeMap::new()),
            metrics,
        }
    }

    /// Build the service of rule `index`. Fails fast on a bad origin
    /// (missing directory, missing file) so the tunnel never starts
    /// half-configured.
    pub fn prepare(&self, index: usize, service: &Service) -> Result<()> {
        if let Some(built) = Built::build(service)? {
            self.lock().insert(index, Arc::new(built));
        }
        Ok(())
    }

    pub fn get(&self, index: usize) -> Option<Arc<Built>> {
        self.lock().get(&index).cloned()
    }

    pub fn metrics(&self) -> &Metrics {
        &self.metrics
    }

    pub fn len(&self) -> usize {
        self.lock().len()
    }

    pub fn is_empty(&self) -> bool {
        self.lock().is_empty()
    }

    /// Session controls for every share rule, so a host can watch for
    /// `stop_after` and idle timeouts.
    pub fn share_controls(&self) -> Vec<Arc<ShareControl>> {
        self.lock()
            .values()
            .filter_map(|built| match built.as_ref() {
                Built::Share(app) => Some(app.control()),
                Built::ShareProxy { control, .. } => Some(control.clone()),
                _ => None,
            })
            .collect()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, BTreeMap<usize, Arc<Built>>> {
        self.built.lock().unwrap_or_else(|e| e.into_inner())
    }
}
