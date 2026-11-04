use crate::retry::is_idempotent_method;
use crate::state::{AppState, HashKeyTarget};
use bytes::Bytes;
use http_body_util::combinators::BoxBody;
use http_body_util::{BodyExt, Full};
use hyper::body::{Body, Incoming};
use hyper::header::{HeaderMap, HeaderName, HeaderValue};
use hyper::{Request, Response, StatusCode, Uri};
use hyper_util::rt::TokioIo;
use std::net::SocketAddr;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::{Duration, Instant};

pub type BoxedBody = BoxBody<Bytes, hyper::Error>;

pub fn full_body<T: Into<Bytes>>(chunk: T) -> BoxedBody {
    Full::new(chunk.into())
        .map_err(|never| match never {})
        .boxed()
}

pub fn error_response(status: StatusCode, msg: &'static str) -> Response<BoxedBody> {
    Response::builder()
        .status(status)
        .header(hyper::header::CONTENT_TYPE, "text/plain; charset=utf-8")
        .body(full_body(msg))
        .expect("static error response is always valid")
}

fn is_hop_by_hop(name: &HeaderName) -> bool {
    let s = name.as_str();
    s.eq_ignore_ascii_case("connection")
        || s.eq_ignore_ascii_case("keep-alive")
        || s.eq_ignore_ascii_case("proxy-authenticate")
        || s.eq_ignore_ascii_case("proxy-authorization")
        || s.eq_ignore_ascii_case("te")
        || s.eq_ignore_ascii_case("trailer")
        || s.eq_ignore_ascii_case("transfer-encoding")
        || s.eq_ignore_ascii_case("upgrade")
        || s.to_ascii_lowercase().starts_with("proxy-")
}

pub fn sanitize_request_headers(
    headers: &mut HeaderMap,
    client_addr: SocketAddr,
    is_upgrade: bool,
) {
    sanitize_request_headers_with_tls(headers, client_addr, is_upgrade, false);
}

pub fn sanitize_request_headers_with_tls(
    headers: &mut HeaderMap,
    client_addr: SocketAddr,
    is_upgrade: bool,
    is_tls: bool,
) {
    let mut extra_hop_by_hop = Vec::new();
    if let Some(conn_val) = headers.get(hyper::header::CONNECTION) {
        if let Ok(conn_str) = conn_val.to_str() {
            for part in conn_str.split(',') {
                let trimmed = part.trim();
                if !trimmed.is_empty() {
                    extra_hop_by_hop.push(trimmed.to_ascii_lowercase());
                }
            }
        }
    }

    let keys_to_remove: Vec<HeaderName> = headers
        .keys()
        .filter(|key| {
            let s = key.as_str().to_ascii_lowercase();
            if is_upgrade && (s == "upgrade" || s == "connection") {
                return false;
            }
            is_hop_by_hop(key) || extra_hop_by_hop.contains(&s)
        })
        .cloned()
        .collect();

    for k in keys_to_remove {
        headers.remove(k);
    }

    let x_ff = HeaderName::from_static("x-forwarded-for");
    let forwarded_value = match headers.get(&x_ff) {
        Some(existing) => {
            if let Ok(s) = existing.to_str() {
                format!("{}, {}", s, client_addr.ip())
            } else {
                client_addr.ip().to_string()
            }
        }
        None => client_addr.ip().to_string(),
    };
    if let Ok(hv) = HeaderValue::from_str(&forwarded_value) {
        headers.insert(x_ff, hv);
    }

    let x_fp = HeaderName::from_static("x-forwarded-proto");
    if !headers.contains_key(&x_fp) {
        let proto = if is_tls { "https" } else { "http" };
        headers.insert(x_fp, HeaderValue::from_static(proto));
    }

    let x_fh = HeaderName::from_static("x-forwarded-host");
    if !headers.contains_key(&x_fh) {
        if let Some(host_val) = headers.get(hyper::header::HOST) {
            let host_clone = host_val.clone();
            headers.insert(x_fh, host_clone);
        }
    }
}

pub fn sanitize_response_headers(headers: &mut HeaderMap, is_upgrade: bool) {
    let mut extra_hop_by_hop = Vec::new();
    if let Some(conn_val) = headers.get(hyper::header::CONNECTION) {
        if let Ok(conn_str) = conn_val.to_str() {
            for part in conn_str.split(',') {
                let trimmed = part.trim();
                if !trimmed.is_empty() {
                    extra_hop_by_hop.push(trimmed.to_ascii_lowercase());
                }
            }
        }
    }

    let keys_to_remove: Vec<HeaderName> = headers
        .keys()
        .filter(|key| {
            let s = key.as_str().to_ascii_lowercase();
            if is_upgrade && (s == "upgrade" || s == "connection") {
                return false;
            }
            is_hop_by_hop(key) || extra_hop_by_hop.contains(&s)
        })
        .cloned()
        .collect();

    for k in keys_to_remove {
        headers.remove(k);
    }
}

fn headers_byte_size(headers: &HeaderMap) -> usize {
    headers
        .iter()
        .map(|(name, value)| name.as_str().len() + value.len() + 4)
        .sum()
}

struct InFlightGuard(Arc<AppState>);

impl Drop for InFlightGuard {
    fn drop(&mut self) {
        self.0.in_flight_requests.fetch_sub(1, Ordering::Relaxed);
    }
}

struct AccessLogParams<'a> {
    request_id: &'a str,
    client_addr: SocketAddr,
    method: &'a hyper::Method,
    path: &'a str,
    status: StatusCode,
    pool: &'a str,
    backend: Option<SocketAddr>,
    latency: Duration,
    retries: usize,
}

fn log_access(state: &AppState, p: AccessLogParams<'_>) {
    if !state.config.access_log.enabled {
        return;
    }

    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();

    let backend_str = p
        .backend
        .map(|b| b.to_string())
        .unwrap_or_else(|| "none".to_string());
    let latency_ms = p.latency.as_secs_f64() * 1000.0;

    if state.config.access_log.format == "json" {
        let entry = serde_json::json!({
            "timestamp": timestamp,
            "request_id": p.request_id,
            "client_ip": p.client_addr.ip().to_string(),
            "method": p.method.as_str(),
            "path": p.path,
            "status": p.status.as_u16(),
            "pool": p.pool,
            "backend": backend_str,
            "latency_ms": (latency_ms * 100.0).round() / 100.0,
            "retries": p.retries,
        });
        tracing::info!(target: "access_log", "{}", entry);
    } else {
        tracing::info!(
            target: "access_log",
            "{} [{}] {} {} {} pool={} backend={} latency_ms={:.2} retries={}",
            p.client_addr.ip(),
            p.request_id,
            p.method,
            p.path,
            p.status.as_u16(),
            p.pool,
            backend_str,
            latency_ms,
            p.retries
        );
    }
}

pub async fn handle(
    state: Arc<AppState>,
    client_addr: SocketAddr,
    req: Request<Incoming>,
) -> Result<Response<BoxedBody>, hyper::Error> {
    let start_time = Instant::now();
    let method = req.method().clone();
    let raw_path_and_query = req
        .uri()
        .path_and_query()
        .map(|pq| pq.as_str())
        .unwrap_or("/")
        .to_string();

    let request_id = req
        .headers()
        .get("x-request-id")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
        .unwrap_or_else(|| {
            format!(
                "{:016x}{:016x}",
                rand::random::<u64>(),
                rand::random::<u64>()
            )
        });

    // 1. Global concurrency limit with load shedding (503)
    let max_conn = state.config.limits.max_concurrent_connections;
    if max_conn > 0 {
        let mut current = state.in_flight_requests.load(Ordering::Relaxed);
        loop {
            if current >= max_conn {
                state.load_shed_count.fetch_add(1, Ordering::Relaxed);
                state.metrics.load_shed_requests_total.inc();
                state
                    .metrics
                    .http_requests_total
                    .with_label_values(&["503", "none"])
                    .inc();
                state
                    .metrics
                    .http_request_duration_seconds
                    .with_label_values(&["503", "none"])
                    .observe(start_time.elapsed().as_secs_f64());

                log_access(
                    &state,
                    AccessLogParams {
                        request_id: &request_id,
                        client_addr,
                        method: &method,
                        path: &raw_path_and_query,
                        status: StatusCode::SERVICE_UNAVAILABLE,
                        pool: "none",
                        backend: None,
                        latency: start_time.elapsed(),
                        retries: 0,
                    },
                );

                tracing::warn!(
                    client = %client_addr,
                    current,
                    max_conn,
                    "load shedding: global concurrency limit reached"
                );
                let mut resp = error_response(
                    StatusCode::SERVICE_UNAVAILABLE,
                    "503 Service Unavailable: Server Overloaded\n",
                );
                if let Ok(hv) = HeaderValue::from_str(&request_id) {
                    resp.headers_mut()
                        .insert(HeaderName::from_static("x-request-id"), hv);
                }
                return Ok(resp);
            }
            match state.in_flight_requests.compare_exchange_weak(
                current,
                current + 1,
                Ordering::AcqRel,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(actual) => current = actual,
            }
        }
    } else {
        state.in_flight_requests.fetch_add(1, Ordering::Relaxed);
    }

    let _guard = InFlightGuard(Arc::clone(&state));
    state.retry_budget.record_request();

    // 2. Rate limiting per client IP (429 with Retry-After)
    if let Some(ref limiter) = state.rate_limiter {
        if let Err(retry_after) = limiter.check(client_addr.ip()) {
            state.rate_limited_count.fetch_add(1, Ordering::Relaxed);
            state.metrics.rate_limited_requests_total.inc();
            state
                .metrics
                .http_requests_total
                .with_label_values(&["429", "none"])
                .inc();
            state
                .metrics
                .http_request_duration_seconds
                .with_label_values(&["429", "none"])
                .observe(start_time.elapsed().as_secs_f64());

            log_access(
                &state,
                AccessLogParams {
                    request_id: &request_id,
                    client_addr,
                    method: &method,
                    path: &raw_path_and_query,
                    status: StatusCode::TOO_MANY_REQUESTS,
                    pool: "none",
                    backend: None,
                    latency: start_time.elapsed(),
                    retries: 0,
                },
            );

            tracing::warn!(client = %client_addr, retry_after, "rate limit exceeded");
            let mut resp = Response::builder()
                .status(StatusCode::TOO_MANY_REQUESTS)
                .header(hyper::header::CONTENT_TYPE, "text/plain; charset=utf-8")
                .header(hyper::header::RETRY_AFTER, retry_after.to_string())
                .body(full_body("429 Too Many Requests\n"))
                .unwrap();
            if let Ok(hv) = HeaderValue::from_str(&request_id) {
                resp.headers_mut()
                    .insert(HeaderName::from_static("x-request-id"), hv);
            }
            return Ok(resp);
        }
    }

    let (mut resp, pool_name, backend_addr, retries) =
        handle_inner(state.clone(), client_addr, req, &request_id).await?;

    let elapsed = start_time.elapsed();
    let status = resp.status();

    state
        .metrics
        .http_requests_total
        .with_label_values(&[status.as_str(), &pool_name])
        .inc();
    state
        .metrics
        .http_request_duration_seconds
        .with_label_values(&[status.as_str(), &pool_name])
        .observe(elapsed.as_secs_f64());
    if retries > 0 {
        state
            .metrics
            .http_retries_total
            .with_label_values(&[&pool_name])
            .inc_by(retries as f64);
    }

    log_access(
        &state,
        AccessLogParams {
            request_id: &request_id,
            client_addr,
            method: &method,
            path: &raw_path_and_query,
            status,
            pool: &pool_name,
            backend: backend_addr,
            latency: elapsed,
            retries,
        },
    );

    if let Ok(hv) = HeaderValue::from_str(&request_id) {
        resp.headers_mut()
            .insert(HeaderName::from_static("x-request-id"), hv);
    }

    Ok(resp)
}

async fn handle_inner(
    state: Arc<AppState>,
    client_addr: SocketAddr,
    mut req: Request<Incoming>,
    request_id: &str,
) -> Result<(Response<BoxedBody>, String, Option<SocketAddr>, usize), hyper::Error> {
    // 1. Check max header size limit
    if headers_byte_size(req.headers()) > state.config.limits.max_request_header_size_bytes {
        tracing::warn!(client = %client_addr, "request header size exceeded limit");
        return Ok((
            error_response(
                StatusCode::REQUEST_HEADER_FIELDS_TOO_LARGE,
                "431 Request Header Fields Too Large\n",
            ),
            "none".to_string(),
            None,
            0,
        ));
    }

    // 2. Check max body size limit from Content-Length header if present
    if state.config.limits.max_body_size_bytes > 0 {
        if let Some(cl) = req.headers().get(hyper::header::CONTENT_LENGTH) {
            if let Ok(cl_str) = cl.to_str() {
                if let Ok(len) = cl_str.parse::<usize>() {
                    if len > state.config.limits.max_body_size_bytes {
                        tracing::warn!(client = %client_addr, len, "request payload size exceeded limit");
                        return Ok((
                            error_response(
                                StatusCode::PAYLOAD_TOO_LARGE,
                                "413 Payload Too Large\n",
                            ),
                            "none".to_string(),
                            None,
                            0,
                        ));
                    }
                }
            }
        }
    }

    // 3. Match route and determine pool
    let host = req
        .headers()
        .get(hyper::header::HOST)
        .and_then(|v| v.to_str().ok());

    let raw_path_and_query = req
        .uri()
        .path_and_query()
        .map(|pq| pq.as_str())
        .unwrap_or("/")
        .to_string();

    let route_res = state.router.route(host, &raw_path_and_query);
    let pool = state
        .pools
        .get(route_res.pool)
        .cloned()
        .unwrap_or_else(|| Arc::clone(state.default_pool()));
    let target_path_and_query = route_res.path_and_query;

    // 4. Extract consistent hash key if configured
    let hash_key: Option<String> = match &pool.hash_target {
        Some(HashKeyTarget::Ip) => Some(client_addr.ip().to_string()),
        Some(HashKeyTarget::Header(h)) => req
            .headers()
            .get(h.as_str())
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string()),
        Some(HashKeyTarget::Cookie(c)) => req
            .headers()
            .get(hyper::header::COOKIE)
            .and_then(|v| v.to_str().ok())
            .and_then(|cookies| {
                cookies.split(';').find_map(|pair| {
                    let mut parts = pair.trim().splitn(2, '=');
                    let k = parts.next()?.trim();
                    let v = parts.next()?.trim();
                    if k == c {
                        Some(v.to_string())
                    } else {
                        None
                    }
                })
            }),
        None => None,
    };

    let is_upgrade = req.headers().get(hyper::header::UPGRADE).is_some()
        || req
            .headers()
            .get(hyper::header::CONNECTION)
            .and_then(|c| c.to_str().ok())
            .map(|s| s.to_ascii_lowercase().contains("upgrade"))
            .unwrap_or(false);

    let is_idempotent = is_idempotent_method(req.method());
    let can_retry = is_idempotent && !is_upgrade && req.body().is_end_stream();

    let method = req.method().clone();
    let original_headers = req.headers().clone();
    let req_upgrade = if is_upgrade {
        Some(hyper::upgrade::on(&mut req))
    } else {
        None
    };

    let (_, incoming_body) = req.into_parts();
    let mut initial_body = Some(incoming_body.map_err(|e| e).boxed());

    let max_attempts = if can_retry && state.config.retry.enabled {
        state.config.retry.max_attempts.max(1)
    } else {
        1
    };

    let mut excluded_backends: Vec<SocketAddr> = Vec::new();
    let mut attempt = 0;

    let is_client_tls = state.tls_acceptor.is_some();

    loop {
        attempt += 1;

        let backend = match pool.balancer.select_with_key(
            &pool.backends,
            &excluded_backends,
            hash_key.as_deref(),
        ) {
            Some(b) => b,
            None => {
                if excluded_backends.is_empty() {
                    tracing::warn!(pool = %pool.name, "no healthy backends available");
                    return Ok((
                        error_response(
                            StatusCode::SERVICE_UNAVAILABLE,
                            "503 Service Unavailable: No healthy backends\n",
                        ),
                        pool.name.clone(),
                        None,
                        attempt - 1,
                    ));
                } else {
                    tracing::warn!(pool = %pool.name, "no additional healthy backends available for retry");
                    return Ok((
                        error_response(
                            StatusCode::BAD_GATEWAY,
                            "502 Bad Gateway: Upstream request failed\n",
                        ),
                        pool.name.clone(),
                        None,
                        attempt - 1,
                    ));
                }
            }
        };

        let scheme = if backend.is_tls() { "https" } else { "http" };
        let authority = if let Some(ref sni) = backend.tls_sni() {
            if sni.contains(':') {
                sni.clone()
            } else {
                format!("{}:{}", sni, backend.addr.port())
            }
        } else {
            backend.addr.to_string()
        };
        let new_uri_str = format!("{}://{}{}", scheme, authority, target_path_and_query);
        let new_uri = match new_uri_str.parse::<Uri>() {
            Ok(u) => u,
            Err(e) => {
                tracing::error!(error = %e, uri = %new_uri_str, "failed to build upstream uri");
                return Ok((
                    error_response(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "500 Internal Server Error: Bad upstream URI\n",
                    ),
                    pool.name.clone(),
                    Some(backend.addr),
                    attempt - 1,
                ));
            }
        };

        let mut headers = original_headers.clone();
        sanitize_request_headers_with_tls(&mut headers, client_addr, is_upgrade, is_client_tls);
        if let Ok(hv) = HeaderValue::from_str(request_id) {
            headers.insert(HeaderName::from_static("x-request-id"), hv);
        }

        let mut builder = Request::builder().method(method.clone()).uri(new_uri);
        for (k, v) in headers.iter() {
            builder = builder.header(k, v);
        }

        let body: BoxedBody = if can_retry {
            full_body(Bytes::new())
        } else {
            initial_body
                .take()
                .unwrap_or_else(|| full_body(Bytes::new()))
        };

        let upstream_req = match builder.body(body) {
            Ok(r) => r,
            Err(e) => {
                tracing::error!(error = %e, "failed to build upstream request");
                return Ok((
                    error_response(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "500 Internal Server Error\n",
                    ),
                    pool.name.clone(),
                    Some(backend.addr),
                    attempt - 1,
                ));
            }
        };

        let start_time = Instant::now();
        backend.inc_connections();
        backend.inc_requests();
        let timeout_dur =
            Duration::from_secs(state.config.timeouts.upstream_response_header_secs.max(1));
        let upstream_call = state.client.request(upstream_req);
        let result = tokio::time::timeout(timeout_dur, upstream_call).await;
        backend.dec_connections();
        let elapsed = start_time.elapsed();

        match result {
            Ok(Ok(mut resp)) => {
                let status = resp.status();
                if (status == StatusCode::BAD_GATEWAY
                    || status == StatusCode::SERVICE_UNAVAILABLE
                    || status == StatusCode::GATEWAY_TIMEOUT)
                    && can_retry
                    && attempt < max_attempts
                    && state.retry_budget.try_acquire_retry()
                {
                    backend.record_passive_check(
                        false,
                        state.config.passive_health_check.unhealthy_threshold,
                    );
                    excluded_backends.push(backend.addr);
                    let backoff = state.retry_budget.compute_backoff(attempt);
                    tracing::debug!(
                        backend = %backend.addr,
                        status = %status,
                        attempt,
                        backoff_ms = backoff.as_millis(),
                        "retrying upstream request after 5xx"
                    );
                    tokio::time::sleep(backoff).await;
                    continue;
                }

                if status.is_server_error() {
                    backend.inc_errors();
                    backend.record_passive_check(
                        false,
                        state.config.passive_health_check.unhealthy_threshold,
                    );
                } else {
                    backend.record_latency(elapsed);
                    backend.record_passive_check(
                        true,
                        state.config.passive_health_check.unhealthy_threshold,
                    );
                }

                // Handle HTTP Upgrade / WebSocket
                if status == StatusCode::SWITCHING_PROTOCOLS && is_upgrade {
                    if let Some(on_req) = req_upgrade {
                        let on_resp = hyper::upgrade::on(&mut resp);
                        tokio::spawn(async move {
                            match tokio::try_join!(on_req, on_resp) {
                                Ok((client_upgraded, upstream_upgraded)) => {
                                    let mut client_io = TokioIo::new(client_upgraded);
                                    let mut upstream_io = TokioIo::new(upstream_upgraded);
                                    if let Err(e) = tokio::io::copy_bidirectional(
                                        &mut client_io,
                                        &mut upstream_io,
                                    )
                                    .await
                                    {
                                        tracing::debug!(
                                            error = %e,
                                            "upgraded connection ended"
                                        );
                                    }
                                }
                                Err(e) => {
                                    tracing::error!(
                                        error = %e,
                                        "failed to establish upgraded tunnel"
                                    );
                                }
                            }
                        });
                    }
                }

                let (mut parts, body) = resp.into_parts();
                sanitize_response_headers(
                    &mut parts.headers,
                    is_upgrade && status == StatusCode::SWITCHING_PROTOCOLS,
                );
                let boxed = body.map_err(|e| e).boxed();
                return Ok((
                    Response::from_parts(parts, boxed),
                    pool.name.clone(),
                    Some(backend.addr),
                    attempt - 1,
                ));
            }
            Ok(Err(e)) => {
                backend.inc_errors();
                tracing::warn!(backend = %backend.addr, error = %e, attempt, "upstream connect/io error");
                backend.record_passive_check(
                    false,
                    state.config.passive_health_check.unhealthy_threshold,
                );

                if can_retry && attempt < max_attempts && state.retry_budget.try_acquire_retry() {
                    excluded_backends.push(backend.addr);
                    let backoff = state.retry_budget.compute_backoff(attempt);
                    tracing::debug!(
                        backend = %backend.addr,
                        attempt,
                        backoff_ms = backoff.as_millis(),
                        "retrying upstream request after connect error"
                    );
                    tokio::time::sleep(backoff).await;
                    continue;
                }

                return Ok((
                    error_response(
                        StatusCode::BAD_GATEWAY,
                        "502 Bad Gateway: Upstream connection failed\n",
                    ),
                    pool.name.clone(),
                    Some(backend.addr),
                    attempt - 1,
                ));
            }
            Err(_) => {
                backend.inc_errors();
                tracing::warn!(backend = %backend.addr, attempt, "upstream response header timeout");
                backend.record_passive_check(
                    false,
                    state.config.passive_health_check.unhealthy_threshold,
                );

                if can_retry && attempt < max_attempts && state.retry_budget.try_acquire_retry() {
                    excluded_backends.push(backend.addr);
                    let backoff = state.retry_budget.compute_backoff(attempt);
                    tracing::debug!(
                        backend = %backend.addr,
                        attempt,
                        backoff_ms = backoff.as_millis(),
                        "retrying upstream request after timeout"
                    );
                    tokio::time::sleep(backoff).await;
                    continue;
                }

                return Ok((
                    error_response(
                        StatusCode::GATEWAY_TIMEOUT,
                        "504 Gateway Timeout: Upstream response timed out\n",
                    ),
                    pool.name.clone(),
                    Some(backend.addr),
                    attempt - 1,
                ));
            }
        }
    }
}
