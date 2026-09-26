
use bytes::Bytes;
use http_body_util::{BodyExt, Full};
use hyper::body::Incoming;
use hyper::service::service_fn;
use hyper::{Request, Response};
use hyper_util::rt::{TokioExecutor, TokioIo};
use hyper_util::server::conn::auto::Builder as ConnBuilder;
use std::convert::Infallible;
use tokio::net::TcpListener;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let port: u16 = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "9001".to_string())
        .parse()?;

    let addr = format!("0.0.0.0:{port}");
    let listener = TcpListener::bind(&addr).await?;
    println!("mock_backend listening on {addr}");

    loop {
        let (stream, _) = listener.accept().await?;
        let io = TokioIo::new(stream);

        tokio::spawn(async move {
            let service = service_fn(move |req: Request<Incoming>| async move {
                let body = if req.uri().path() == "/health" {
                    "ok".to_string()
                } else {
                    format!("Hi kaarthikeya : {port}\n")
                };
                Ok::<_, Infallible>(Response::new(Full::new(Bytes::from(body))))
            });

            if let Err(e) = ConnBuilder::new(TokioExecutor::new())
                .serve_connection(io, service)
                .await
            {
                eprintln!("connection error: {e}");
            }
        });
    }
}
