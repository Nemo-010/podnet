//! The roots that podnet trusts for each TLS connection that it opens as a
//! client: the quick-tunnel API, DNS over HTTPS, the WARP API, the fetch of
//! the public URL, the edge, and HTTPS origins. They are the roots of
//! `pod-tls` (PodUtils/podcrates) in one context of the process
//! ([`context`]), with podnet's settings and ring as its provider.
//!
//! The order:
//!
//! 1. A file given with `--ca-file`, else the file in `SSL_CERT_FILE`. The file
//!    is used alone: podnet adds no other root to it.
//! 2. Else all of these: `podnet-ca.pem` beside the binary, the first readable
//!    bundle of [`pod_tls::SYSTEM_BUNDLES`], and the Mozilla roots compiled
//!    into podnet (webpki-roots [`pod_tls::ROOTS_VERSION`]).
//!
//! The edge also trusts the Cloudflare roots of the edge of `pod-services`
//! (`edge_roots_pem` of the Cloudflare adapter). podnet has
//! no option that accepts each certificate for these connections; only an
//! origin can turn off the check (`noTLSVerify`, `--no-tls-verify`).
//!
//! When a certificate is expired or not valid yet, [`pod_tls::clock_hint`]
//! gives the clock of the host, because a wrong clock is a frequent cause on
//! a small host or in a container.

use std::path::PathBuf;
use std::sync::{Arc, OnceLock};

use pod_tls::{CaFile, Context, Settings, Words};

/// The variable that names a file of roots, as in OpenSSL.
pub const CA_FILE_VARIABLE: &str = "SSL_CERT_FILE";
/// The name of the file of roots beside the binary.
pub const BESIDE_BINARY: &str = "podnet-ca.pem";
/// The first date when a root of the edge stops being valid: the fact
/// `roots-first-end` of the edge (`pod-services`).
pub fn edge_roots_first_end() -> String {
    let edge = crate::services::get(crate::services::EDGE);
    edge.date("roots-first-end")
        .map(|d| d.to_string())
        .unwrap_or_default()
}

static CA_FILE: OnceLock<PathBuf> = OnceLock::new();

/// Use `path` as the only file of roots (`--ca-file`). Call it before the
/// first connection; a later call has no effect.
pub fn set_ca_file(path: PathBuf) {
    let _ = CA_FILE.set(path);
}

/// The TLS of this process: its roots, read one time, and its client
/// configurations, on ring.
pub fn context() -> &'static Arc<Context> {
    static CONTEXT: OnceLock<Arc<Context>> = OnceLock::new();
    CONTEXT.get_or_init(|| {
        let mut words = Words::new("podnet");
        words.replace_roots = format!("--ca-file FILE or {CA_FILE_VARIABLE}");
        words.add_roots = Some(format!("{BESIDE_BINARY} beside the binary"));
        let mut settings = Settings::new(words);
        settings.ca_file = CA_FILE
            .get()
            .map(|path| CaFile::new("--ca-file", path.clone()))
            .or_else(|| {
                std::env::var_os(CA_FILE_VARIABLE)
                    .filter(|v| !v.is_empty())
                    .map(|v| CaFile::new(CA_FILE_VARIABLE, v))
            });
        settings.extra_bundle = crate::self_exe::this_binary()
            .and_then(|exe| exe.parent().map(|dir| dir.join(BESIDE_BINARY)));
        let provider = Arc::new(rustls::crypto::ring::default_provider());
        Arc::new(Context::new(provider, settings))
    })
}

/// The trust of each WebSocket and relay connection (`pod-ws`, `pod-relay`):
/// the roots of [`context`], as each other client of podnet has them. A
/// `--ca-file` or `SSL_CERT_FILE` replaces the default roots here too.
/// `pod_tls::Trust::Default` does not read the CA file of the context: it
/// gives the system bundles and the compiled-in roots only, so a proxy that
/// re-signs TLS refused each relay connection with its CA in `--ca-file`.
pub fn ws_trust() -> pod_tls::Trust {
    static TRUST: OnceLock<pod_tls::Trust> = OnceLock::new();
    TRUST
        .get_or_init(|| {
            let ca_file = CA_FILE.get().cloned().or_else(|| {
                std::env::var_os(CA_FILE_VARIABLE)
                    .filter(|v| !v.is_empty())
                    .map(PathBuf::from)
            });
            trust_of(context().client_config_with(&[], &[]), ca_file)
        })
        .clone()
}

/// The trust over the configuration of the context. When the roots of the
/// context do not load, a CA file is used alone ([`pod_tls::Trust::File`]),
/// so that the connection gives the error of the file: podnet never falls
/// back to other roots than the file of the user.
fn trust_of(
    config: Result<rustls::ClientConfig, pod_tls::Error>,
    ca_file: Option<PathBuf>,
) -> pod_tls::Trust {
    match (config, ca_file) {
        (Ok(config), _) => {
            pod_tls::Trust::caller(Arc::new(config)).unwrap_or(pod_tls::Trust::Default)
        }
        (Err(_), Some(path)) => pod_tls::Trust::File(path),
        (Err(_), None) => pod_tls::Trust::Default,
    }
}

/// The seconds of `time` since the Unix epoch, negative before it, for
/// [`pod_core::clock::http_date`].
pub fn unix_secs(time: std::time::SystemTime) -> i64 {
    match time.duration_since(std::time::SystemTime::UNIX_EPOCH) {
        Ok(d) => d.as_secs() as i64,
        Err(e) => -(e.duration().as_secs() as i64),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> rustls::ClientConfig {
        let provider = Arc::new(rustls::crypto::ring::default_provider());
        rustls::ClientConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()
            .unwrap()
            .with_root_certificates(rustls::RootCertStore::empty())
            .with_no_client_auth()
    }

    // The relay and the WebSocket clients take the configuration of the
    // context, which holds the roots of --ca-file: not the default roots.
    #[test]
    fn ws_trust_is_the_configuration_of_the_context() {
        let trust = trust_of(Ok(config()), Some(PathBuf::from("/ca.pem")));
        assert!(matches!(trust, pod_tls::Trust::Caller(_)), "{trust:?}");
        let trust = trust_of(Ok(config()), None);
        assert!(matches!(trust, pod_tls::Trust::Caller(_)), "{trust:?}");
    }

    // A CA file that does not load is used alone, so the connection gives
    // its error: no fall back to the default roots.
    #[test]
    fn ws_trust_keeps_a_bad_ca_file() {
        let err = || pod_tls::Error::Config("no roots".into());
        let trust = trust_of(Err(err()), Some(PathBuf::from("/missing.pem")));
        assert_eq!(trust, pod_tls::Trust::File(PathBuf::from("/missing.pem")));
        assert_eq!(trust_of(Err(err()), None), pod_tls::Trust::Default);
    }
}
