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

pub async fn handle(
    state: Arc<AppState>,
    client_addr: SocketAddr,
    req: Request<Incoming>,
) -> Result<Response<BoxedBody>, hyper::Error> {
    // 1. Global concurrency limit with load shedding (503)
    let max_conn = state.config.limits.max_concurrent_connections;
    if max_conn > 0 {
        let mut current = state.in_flight_requests.load(Ordering::Relaxed);
        loop {
            if current >= max_conn {
                state.load_shed_count.fetch_add(1, Ordering::Relaxed);
                tracing::warn!(
                    client = %client_addr,
                    current,
                    max_conn,
                    "load shedding: global concurrency limit reached"
                );
                return Ok(error_response(
                    StatusCode::SERVICE_UNAVAILABLE,
                    "503 Service Unavailable: Server Overloaded\n",
                ));
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
            tracing::warn!(client = %client_addr, retry_after, "rate limit exceeded");
            return Ok(Response::builder()
                .status(StatusCode::TOO_MANY_REQUESTS)
                .header(hyper::header::CONTENT_TYPE, "text/plain; charset=utf-8")
                .header(hyper::header::RETRY_AFTER, retry_after.to_string())
                .body(full_body("429 Too Many Requests\n"))
                .unwrap());
        }
    }

    handle_inner(state.clone(), client_addr, req).await
}

async fn handle_inner(
    state: Arc<AppState>,
    client_addr: SocketAddr,
    mut req: Request<Incoming>,
) -> Result<Response<BoxedBody>, hyper::Error> {
    // 1. Check max header size limit
    if headers_byte_size(req.headers()) > state.config.limits.max_request_header_size_bytes {
        tracing::warn!(client = %client_addr, "request header size exceeded limit");
        return Ok(error_response(
            StatusCode::REQUEST_HEADER_FIELDS_TOO_LARGE,
            "431 Request Header Fields Too Large\n",
        ));
    }

    // 2. Check max body size limit from Content-Length header if present
    if state.config.limits.max_body_size_bytes > 0 {
        if let Some(cl) = req.headers().get(hyper::header::CONTENT_LENGTH) {
            if let Ok(cl_str) = cl.to_str() {
                if let Ok(len) = cl_str.parse::<usize>() {
                    if len > state.config.limits.max_body_size_bytes {
                        tracing::warn!(client = %client_addr, len, "request payload size exceeded limit");
                        return Ok(error_response(
                            StatusCode::PAYLOAD_TOO_LARGE,
                            "413 Payload Too Large\n",
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
                    return Ok(error_response(
                        StatusCode::SERVICE_UNAVAILABLE,
                        "503 Service Unavailable: No healthy backends\n",
                    ));
                } else {
                    tracing::warn!(pool = %pool.name, "no additional healthy backends available for retry");
                    return Ok(error_response(
                        StatusCode::BAD_GATEWAY,
                        "502 Bad Gateway: Upstream request failed\n",
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
                return Ok(error_response(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "500 Internal Server Error: Bad upstream URI\n",
                ));
            }
        };

        let mut headers = original_headers.clone();
        sanitize_request_headers_with_tls(&mut headers, client_addr, is_upgrade, is_client_tls);

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
                return Ok(error_response(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "500 Internal Server Error\n",
                ));
            }
        };

        let start_time = Instant::now();
        backend.inc_connections();
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
                return Ok(Response::from_parts(parts, boxed));
            }
            Ok(Err(e)) => {
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

                return Ok(error_response(
                    StatusCode::BAD_GATEWAY,
                    "502 Bad Gateway: Upstream connection failed\n",
                ));
            }
            Err(_) => {
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

                return Ok(error_response(
                    StatusCode::GATEWAY_TIMEOUT,
                    "504 Gateway Timeout: Upstream response timed out\n",
                ));
            }
        }
    }
}
