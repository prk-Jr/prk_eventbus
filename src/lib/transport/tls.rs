use axum_server::tls_rustls::RustlsConfig;
use std::path::PathBuf;

pub async fn configure_tls(
    cert_path: PathBuf,
    key_path: PathBuf,
) -> Result<RustlsConfig, anyhow::Error> {
    RustlsConfig::from_pem_file(cert_path, key_path)
        .await
        .map_err(|e| anyhow::anyhow!(e))
}