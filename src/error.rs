use hyper::StatusCode;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ProxyError {
    #[error("no healthy backends available")]
    NoHealthyBackends,

    #[error("circuit breaker open for backend: {0}")]
    CircuitBreakerOpen(String),

    #[error("upstream connect error: {0}")]
    UpstreamConnect(String),

    #[error("upstream connect timeout")]
    ConnectTimeout,

    #[error("upstream response timeout")]
    ResponseTimeout,

    #[error("client request header too large")]
    HeaderTooLarge,

    #[error("client payload too large")]
    PayloadTooLarge,

    #[error("client error: {0}")]
    ClientError(String),

    #[error("bad upstream URI: {0}")]
    BadUri(String),

    #[error("hyper error: {0}")]
    Hyper(#[from] hyper::Error),

    #[error("upgrade error: {0}")]
    Upgrade(String),
}

impl ProxyError {
    pub fn status_code(&self) -> StatusCode {
        match self {
            Self::NoHealthyBackends | Self::CircuitBreakerOpen(_) => {
                StatusCode::SERVICE_UNAVAILABLE
            }
            Self::UpstreamConnect(_) | Self::BadUri(_) | Self::Upgrade(_) | Self::Hyper(_) => {
                StatusCode::BAD_GATEWAY
            }
            Self::ConnectTimeout | Self::ResponseTimeout => StatusCode::GATEWAY_TIMEOUT,
            Self::HeaderTooLarge => StatusCode::REQUEST_HEADER_FIELDS_TOO_LARGE,
            Self::PayloadTooLarge => StatusCode::PAYLOAD_TOO_LARGE,
            Self::ClientError(_) => StatusCode::BAD_REQUEST,
        }
    }

    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            Self::UpstreamConnect(_) | Self::ConnectTimeout | Self::ResponseTimeout
        )
    }
}
