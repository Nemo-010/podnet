//! The counters of a tunnel: pod-cloudflare's, with the name of podnet and
//! the counters of the decisions of podnet after them.

use std::sync::Arc;

pub use pod_cloudflare::metrics::*;

/// The counters of a tunnel of podnet: the metrics start with `podnet_`,
/// and the counters of the decisions come after them.
pub fn podnet() -> Metrics {
    Metrics::with_prefix("podnet").with_extra(Arc::new(crate::decisions::prometheus))
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_metrics_of_podnet_have_its_name_and_its_decisions() {
        crate::decisions::count("leg", "http2");
        let text = super::podnet().prometheus();
        assert!(text.contains("podnet_streams_total 0"), "{text}");
        assert!(text.contains(&crate::decisions::prometheus()), "{text}");
    }
}
