use crate::state::AppState;
use bytes::Bytes;
use http_body_util::combinators::BoxBody;
use http_body_util::{BodyExt, Full};
use hyper::body::Incoming;
use hyper::header::{HeaderName, HeaderValue};
use hyper::{Request, Response, StatusCode, Uri};
use std::net::SocketAddr;
use std::sync::Arc;

pub type BoxedBody = BoxBody<Bytes, hyper::Error>;

fn x_forwarded_for() -> HeaderName {
    HeaderName::from_static("x-forwarded-for")
}

fn full_body<T: Into<Bytes>>(chunk: T) -> BoxedBody {
    Full::new(chunk.into())
        .map_err(|never| match never {})
        .boxed()
}

fn error_response(status: StatusCode, msg: &'static str) -> Response<BoxedBody> {
    Response::builder()
        .status(status)
        .body(full_body(msg))
        .expect("static error response is always valid")
}

pub async fn handle(
    state: Arc<AppState>,
    client_addr: SocketAddr,
    mut req: Request<Incoming>,
) -> Result<Response<BoxedBody>, hyper::Error> {
    let backend = match state.balancer.select(&state.backends) {
        Some(b) => b,
        None => {
            tracing::warn!("no healthy backends available");
            return Ok(error_response(
                StatusCode::BAD_GATEWAY,
                "no healthy backends available",
            ));
        }
    };

    let path_and_query = req
        .uri()
        .path_and_query()
        .map(|pq| pq.as_str())
        .unwrap_or("/");
    let new_uri = match format!("http://{}{}", backend.addr, path_and_query).parse::<Uri>() {
        Ok(u) => u,
        Err(e) => {
            tracing::error!(error = %e, "failed to build upstream uri");
            return Ok(error_response(StatusCode::INTERNAL_SERVER_ERROR, "bad upstream uri"));
        }
    };
    *req.uri_mut() = new_uri;

    let header_name = x_forwarded_for();
    let forwarded_value = match req.headers().get(&header_name) {
        Some(existing) => format!("{}, {}", existing.to_str().unwrap_or(""), client_addr.ip()),
        None => client_addr.ip().to_string(),
    };
    if let Ok(hv) = HeaderValue::from_str(&forwarded_value) {
        req.headers_mut().insert(header_name, hv);
    }

    backend.inc_connections();
    let result = state.client.request(req).await;
    backend.dec_connections();

    match result {
        Ok(resp) => {
            let (parts, body) = resp.into_parts();
            let boxed = body.map_err(|e| e).boxed();
            Ok(Response::from_parts(parts, boxed))
        }
        Err(e) => {
            tracing::error!(backend = %backend.addr, error = %e, "upstream request failed");
            Ok(error_response(StatusCode::BAD_GATEWAY, "upstream request failed"))
        }
    }
}
