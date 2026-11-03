#![no_main]

use hyper::header::{HeaderMap, HeaderName, HeaderValue};
use l7_load_balancer::proxy::{sanitize_request_headers_with_tls, sanitize_response_headers};
use libfuzzer_sys::fuzz_target;
use std::net::SocketAddr;

fuzz_target!(|data: &[u8]| {
    let mut headers = HeaderMap::new();
    let mut cursor = data;

    while cursor.len() >= 4 {
        let k_len = (cursor[0] as usize % 32) + 1;
        cursor = &cursor[1..];
        if cursor.len() < k_len {
            break;
        }
        let k_bytes = &cursor[..k_len];
        cursor = &cursor[k_len..];

        if cursor.is_empty() {
            break;
        }
        let v_len = cursor[0] as usize % 64;
        cursor = &cursor[1..];
        if cursor.len() < v_len {
            break;
        }
        let v_bytes = &cursor[..v_len];
        cursor = &cursor[v_len..];

        if let (Ok(name), Ok(val)) = (
            HeaderName::from_bytes(k_bytes),
            HeaderValue::from_bytes(v_bytes),
        ) {
            headers.append(name, val);
        }
    }

    let is_upgrade = !data.is_empty() && (data[0] & 1 != 0);
    let is_tls = !data.is_empty() && (data[0] & 2 != 0);
    let client_addr: SocketAddr = "127.0.0.1:8080".parse().unwrap();

    let mut req_headers = headers.clone();
    sanitize_request_headers_with_tls(&mut req_headers, client_addr, is_upgrade, is_tls);

    let mut resp_headers = headers;
    sanitize_response_headers(&mut resp_headers, is_upgrade);
});

