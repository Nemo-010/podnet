//! The quick tunnels of the trycloudflare service: pod-cloudflare's.

pub use pod_cloudflare::quicktunnel::*;

/// The refusal of a provision in the chain of `err` (a failure of
/// pod-cloudflare in an error of podnet), when there is one.
pub fn retry_of(err: &anyhow::Error) -> Option<Refusal> {
    err.chain()
        .filter_map(|e| e.downcast_ref::<pod_core::Failure>())
        .find_map(pod_cloudflare::quicktunnel::retry_of)
}

#[cfg(test)]
mod tests {
    use anyhow::Context as _;

    /// The refusal of a provision of pod-cloudflare in an error of podnet
    /// reaches the error report with its fields.
    #[test]
    fn the_refusal_of_the_crate_reaches_the_report() {
        let failure = pod_core::Failure::from(super::ProvisionError::Status {
            code: 429,
            body: "error code: 1015".into(),
            retry_after: Some(std::time::Duration::from_secs(1379)),
        });
        let err = Err::<(), _>(failure)
            .context("starting the tunnel")
            .unwrap_err();
        let refusal = super::retry_of(&err).expect("a refusal");
        assert_eq!(refusal.retry_after_s, Some(1379));
        assert!(super::retry_of(&anyhow::anyhow!("other")).is_none());
        crate::report::read_refusals_with(super::retry_of);
        let doc = crate::report::document(Some("tunnel"), 75, Some(&err));
        assert_eq!(doc["retry_after_s"], 1379);
        assert_eq!(doc["code"], 1015);
        assert_eq!(doc["status"], 429);
        assert_eq!(doc["retryable"], true);
        assert_eq!(crate::exit::code_of(&err), crate::exit::TEMPFAIL);
    }
}
