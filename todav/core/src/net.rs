//! What every HTTPS request trusts: the system's certificate authorities, so a server whose
//! certificate comes from one the user added to the system (a self-hosted Nextcloud's own CA)
//! connects too, as in the browser. On Android the app hands them over (`set_roots`), the
//! system's and the user's; elsewhere the platform's verifier reads them.

use std::sync::{Arc, RwLock};
use ureq::tls::{Certificate, RootCerts, TlsConfig};

static ROOTS: RwLock<Option<Arc<Vec<Certificate<'static>>>>> = RwLock::new(None);

/// Trust these certificate authorities (DER) instead of the platform's: Android's, which the
/// platform verifier could read only through the JVM. An empty list changes nothing.
pub fn set_roots(certs: Vec<Vec<u8>>) {
    if certs.is_empty() {
        return;
    }
    let certs = certs
        .iter()
        .map(|c| Certificate::from_der(c).to_owned())
        .collect();
    *ROOTS.write().unwrap() = Some(Arc::new(certs));
}

/// For every agent's `tls_config`.
pub(crate) fn tls() -> TlsConfig {
    let roots = match ROOTS.read().unwrap().clone() {
        Some(certs) => RootCerts::Specific(certs),
        None => platform(),
    };
    TlsConfig::builder().root_certs(roots).build()
}

#[cfg(not(target_os = "android"))]
fn platform() -> RootCerts {
    RootCerts::PlatformVerifier
}

/// Mozilla's, until the app has set Android's.
#[cfg(target_os = "android")]
fn platform() -> RootCerts {
    RootCerts::WebPki
}
