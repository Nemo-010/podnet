//! The dial of a forward origin is pod-cloudflare's. podnet gives it its
//! TLS context, its words, its log and its own configuration for an origin
//! with `noTLSVerify`: a check that accepts each certificate, which podnet
//! keeps (pod-cloudflare makes no TLS with no check).

use std::sync::{Arc, OnceLock};

use anyhow::Result;
use tokio_rustls::rustls::client::danger::{
    HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier,
};
use tokio_rustls::rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use tokio_rustls::rustls::{ClientConfig, DigitallySignedStruct, SignatureScheme};

use crate::ingress::{OriginOptions, Service};
use podnet_core::net::BoxedIo;

pub use pod_cloudflare::origin::origin_label;

/// The dial of the origins of podnet: one for the process, so that the
/// note of an origin with `noTLSVerify` comes one time.
pub fn origins() -> &'static pod_cloudflare::origin::Origins {
    static ORIGINS: OnceLock<pod_cloudflare::origin::Origins> = OnceLock::new();
    ORIGINS.get_or_init(|| {
        podnet_core::net::init_crypto();
        pod_cloudflare::origin::Origins::new(podnet_core::trust::context().clone())
            .with_report(podnet_core::decisions::report())
            .with_words(podnet_core::udp::words())
            .with_no_verify(Some(no_verify_config()))
    })
}

/// Dial the origin for a forward service.
pub async fn dial(service: &Service, options: &OriginOptions) -> Result<BoxedIo> {
    Ok(Box::new(origins().dial(service, options).await?))
}

/// The TLS configuration of an origin with `noTLSVerify`: podnet's check,
/// which accepts each certificate.
pub fn no_verify_config() -> Arc<ClientConfig> {
    static ONCE: OnceLock<Arc<ClientConfig>> = OnceLock::new();
    ONCE.get_or_init(|| {
        podnet_core::net::init_crypto();
        let provider = Arc::new(tokio_rustls::rustls::crypto::ring::default_provider());
        let config = ClientConfig::builder()
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(NoVerify(provider)))
            .with_no_client_auth();
        Arc::new(config)
    })
    .clone()
}

#[derive(Debug)]
struct NoVerify(Arc<tokio_rustls::rustls::crypto::CryptoProvider>);

impl ServerCertVerifier for NoVerify {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, tokio_rustls::rustls::Error> {
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, tokio_rustls::rustls::Error> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, tokio_rustls::rustls::Error> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.0.signature_verification_algorithms.supported_schemes()
    }
}
