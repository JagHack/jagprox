//! The shared HTTP client: rustls with the `ring` provider (builds fully static) and
//! trust in both the OS certificate store and Mozilla's bundled roots, so HTTPS works
//! on minimal systems without a CA bundle too.

use std::sync::{Arc, OnceLock};
use std::time::Duration;

static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
static NO_REDIRECT: OnceLock<reqwest::Client> = OnceLock::new();

pub const USER_AGENT: &str = concat!("JagProx/", env!("CARGO_PKG_VERSION"));

fn tls_config() -> rustls::ClientConfig {
    let mut roots = rustls::RootCertStore::empty();
    roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    let native = rustls_native_certs::load_native_certs();
    for cert in native.certs {
        let _ = roots.add(cert);
    }
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    rustls::ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .expect("ring supports the default TLS versions")
        .with_root_certificates(roots)
        .with_no_client_auth()
}

/// The process-wide client (connection pooling, keep-alive).
pub fn client() -> &'static reqwest::Client {
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .use_preconfigured_tls(tls_config())
            .user_agent(USER_AGENT)
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(20))
            .build()
            .expect("HTTP client configuration is valid")
    })
}

/// A client that does not follow redirects, so a 302 `Location` header can be read
/// (the cookie sign-in reads the OAuth token out of that redirect).
pub fn no_redirect_client() -> &'static reqwest::Client {
    NO_REDIRECT.get_or_init(|| {
        reqwest::Client::builder()
            .use_preconfigured_tls(tls_config())
            .user_agent(USER_AGENT)
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(20))
            .build()
            .expect("HTTP client configuration is valid")
    })
}
