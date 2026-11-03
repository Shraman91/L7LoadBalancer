use hyper_util::client::legacy::connect::HttpConnector;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, ServerName, UnixTime};
use rustls::server::ServerConfig;
use rustls::{ClientConfig, RootCertStore};
use std::fs::File;
use std::io::BufReader;
use std::path::Path;
use std::sync::Arc;
use tokio_rustls::TlsAcceptor;

pub fn init_crypto_provider() {
    let _ = rustls::crypto::ring::default_provider().install_default();
}

#[derive(Debug)]
pub struct DangerAcceptAnyServerCert(Arc<rustls::crypto::CryptoProvider>);

impl DangerAcceptAnyServerCert {
    pub fn new() -> Self {
        Self(Arc::new(rustls::crypto::ring::default_provider()))
    }
}

impl Default for DangerAcceptAnyServerCert {
    fn default() -> Self {
        Self::new()
    }
}

impl rustls::client::danger::ServerCertVerifier for DangerAcceptAnyServerCert {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(
            message,
            cert,
            dss,
            &self.0.signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(
            message,
            cert,
            dss,
            &self.0.signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        self.0.signature_verification_algorithms.supported_schemes()
    }
}

pub fn load_certs(path: impl AsRef<Path>) -> anyhow::Result<Vec<CertificateDer<'static>>> {
    let file = File::open(path.as_ref()).map_err(|e| {
        anyhow::anyhow!(
            "failed to open cert file {}: {}",
            path.as_ref().display(),
            e
        )
    })?;
    let mut reader = BufReader::new(file);
    let certs = rustls_pemfile::certs(&mut reader)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| {
            anyhow::anyhow!(
                "failed to parse certs from {}: {}",
                path.as_ref().display(),
                e
            )
        })?;
    if certs.is_empty() {
        anyhow::bail!("no certificates found in {}", path.as_ref().display());
    }
    Ok(certs)
}

pub fn load_key(path: impl AsRef<Path>) -> anyhow::Result<PrivateKeyDer<'static>> {
    let file = File::open(path.as_ref()).map_err(|e| {
        anyhow::anyhow!("failed to open key file {}: {}", path.as_ref().display(), e)
    })?;
    let mut reader = BufReader::new(file);
    let key = rustls_pemfile::private_key(&mut reader)
        .map_err(|e| {
            anyhow::anyhow!(
                "failed to parse private key from {}: {}",
                path.as_ref().display(),
                e
            )
        })?
        .ok_or_else(|| anyhow::anyhow!("no private key found in {}", path.as_ref().display()))?;
    Ok(key)
}

pub fn create_tls_acceptor(
    cert_path: impl AsRef<Path>,
    key_path: impl AsRef<Path>,
    alpn_protocols: &[String],
) -> anyhow::Result<TlsAcceptor> {
    init_crypto_provider();
    let certs = load_certs(&cert_path)?;
    let key = load_key(&key_path)?;

    let mut server_config = ServerConfig::builder_with_protocol_versions(&[
        &rustls::version::TLS13,
        &rustls::version::TLS12,
    ])
    .with_no_client_auth()
    .with_single_cert(certs, key)
    .map_err(|e| anyhow::anyhow!("invalid TLS server config: {}", e))?;

    if alpn_protocols.is_empty() {
        server_config.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];
    } else {
        server_config.alpn_protocols = alpn_protocols
            .iter()
            .map(|p| p.as_bytes().to_vec())
            .collect();
    }

    Ok(TlsAcceptor::from(Arc::new(server_config)))
}

pub fn create_client_tls_config(insecure: bool) -> Arc<ClientConfig> {
    init_crypto_provider();
    if insecure {
        let config = ClientConfig::builder_with_protocol_versions(&[
            &rustls::version::TLS13,
            &rustls::version::TLS12,
        ])
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(DangerAcceptAnyServerCert::new()))
        .with_no_client_auth();
        Arc::new(config)
    } else {
        let mut root_store = RootCertStore::empty();
        root_store.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
        let config = ClientConfig::builder_with_protocol_versions(&[
            &rustls::version::TLS13,
            &rustls::version::TLS12,
        ])
        .with_root_certificates(root_store)
        .with_no_client_auth();
        Arc::new(config)
    }
}

pub fn create_https_connector(
    mut http_connector: HttpConnector,
    insecure: bool,
) -> hyper_rustls::HttpsConnector<HttpConnector> {
    http_connector.enforce_http(false);
    let tls_config = create_client_tls_config(insecure);
    hyper_rustls::HttpsConnectorBuilder::new()
        .with_tls_config((*tls_config).clone())
        .https_or_http()
        .enable_http1()
        .enable_http2()
        .wrap_connector(http_connector)
}
