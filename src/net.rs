//! HTTP clients for the native builds. Desktop reqwest uses the OS TLS stack; Android has no
//! system OpenSSL to link, so it gets rustls (ring) with the Mozilla roots bundled.

/// A reqwest client builder that can reach `https` URLs on every native target.
pub fn client_builder() -> reqwest::ClientBuilder {
    let builder = reqwest::Client::builder();
    #[cfg(target_os = "android")]
    let builder = builder.tls_backend_preconfigured(android::tls());
    builder
}

#[cfg(target_os = "android")]
pub use android::tls as tls_config;

#[cfg(target_os = "android")]
mod android {
    use std::sync::Arc;

    // Bundled roots instead of the platform verifier, which needs its own Java classes and a
    // JNI handshake at start-up.
    pub fn tls() -> rustls::ClientConfig {
        let roots = rustls::RootCertStore { roots: webpki_roots::TLS_SERVER_ROOTS.to_vec() };
        rustls::ClientConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
            .with_safe_default_protocol_versions()
            .expect("ring supports the default TLS versions")
            .with_root_certificates(roots)
            .with_no_client_auth()
    }
}
