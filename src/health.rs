use crate::backend::Backend;
use crate::config::HealthCheckConfig;
use bytes::Bytes;
use http_body_util::Empty;
use hyper::Request;
use hyper_util::client::legacy::connect::HttpConnector;
use hyper_util::client::legacy::Client;
use hyper_util::rt::TokioExecutor;
use std::sync::Arc;
use std::time::Duration;

pub type HttpClient = Client<HttpConnector, Empty<Bytes>>;

pub fn spawn_health_checks(backends: Vec<Arc<Backend>>, cfg: HealthCheckConfig) {
    let client: HttpClient = Client::builder(TokioExecutor::new()).build(HttpConnector::new());

    for backend in backends {
        let client = client.clone();
        let cfg = cfg.clone();
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(Duration::from_secs(cfg.interval_secs));
            loop {
                ticker.tick().await;
                let ok = probe(&client, &backend, &cfg).await;
                backend.record_check(ok, cfg.unhealthy_threshold, cfg.healthy_threshold);
            }
        });
    }
}

async fn probe(client: &HttpClient, backend: &Backend, cfg: &HealthCheckConfig) -> bool {
    let uri = format!("http://{}{}", backend.addr, cfg.path);
    let req = match Request::get(&uri).body(Empty::<Bytes>::new()) {
        Ok(r) => r,
        Err(e) => {
            tracing::error!(backend = %backend.addr, error = %e, "failed to build health check request");
            return false;
        }
    };

    let call = client.request(req);
    match tokio::time::timeout(Duration::from_secs(cfg.timeout_secs), call).await {
        Ok(Ok(resp)) => resp.status().is_success(),
        Ok(Err(e)) => {
            tracing::debug!(backend = %backend.addr, error = %e, "health check request failed");
            false
        }
        Err(_) => {
            tracing::debug!(backend = %backend.addr, "health check timed out");
            false
        }
    }
}
