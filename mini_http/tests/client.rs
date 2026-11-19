//! The client talking to this project's own server over real sockets.

use std::time::Duration;

use mini_http::client::{Client, LiteralResolver};
use mini_http::error::HttpError;
use mini_http::response::Response;
use mini_http::router::Router;
use mini_http::server::{Server, ServerConfig, ServerHandle};
use mini_http::status::StatusCode;

/// A server with a route for each kind of response worth fetching.
fn start() -> ServerHandle {
    let router = Router::new()
        .get("/hello/:name", |_, params| {
            Response::text(
                StatusCode::OK,
                format!("hello, {}", params.get("name").unwrap()),
            )
        })
        .get("/stream", |_, _| {
            Response::stream(StatusCode::OK, &b"sent in chunks, length unknown"[..])
        })
        .get("/empty", |_, _| Response::new(StatusCode::NO_CONTENT))
        .get("/big", |_, _| {
            Response::text(StatusCode::OK, "x".repeat(50_000))
        })
        .post("/echo", |request, _| {
            let kind = request.header("Content-Type").unwrap_or("none");
            Response::new(StatusCode::OK)
                .header("Content-Type", kind)
                .body(request.body.clone())
        });
    Server::bind("127.0.0.1:0", router, ServerConfig::default())
        .unwrap()
        .spawn()
        .unwrap()
}

fn client() -> Client<LiteralResolver> {
    Client::new(LiteralResolver)
}

#[test]
fn a_get_returns_status_headers_and_body() {
    let server = start();
    let response = client()
        .get(&format!("http://{}/hello/world", server.address()))
        .unwrap();
    assert_eq!(
        (response.status, response.reason.as_str()),
        (StatusCode::OK, "OK")
    );
    assert_eq!(response.text(), "hello, world");
    assert_eq!(
        response.headers.get("Content-Type"),
        Some("text/plain; charset=utf-8")
    );
    assert_eq!(response.headers.get("Connection"), Some("close"));
    assert!(response.headers.contains("Date"));
}

#[test]
fn error_statuses_are_responses_not_failures() {
    let server = start();
    let base = format!("http://{}", server.address());
    let missing = client().get(&format!("{base}/nope")).unwrap();
    assert_eq!(
        (missing.status, missing.text().as_str()),
        (StatusCode::NOT_FOUND, "404 Not Found\n")
    );
    let wrong = client()
        .post(&format!("{base}/stream"), "text/plain", b"x")
        .unwrap();
    assert_eq!(wrong.status, StatusCode::METHOD_NOT_ALLOWED);
    assert_eq!(wrong.headers.get("Allow"), Some("GET, HEAD"));
}

#[test]
fn a_chunked_response_arrives_whole() {
    let server = start();
    let response = client()
        .get(&format!("http://{}/stream", server.address()))
        .unwrap();
    assert_eq!(response.headers.get("Transfer-Encoding"), Some("chunked"));
    assert!(!response.headers.contains("Content-Length"));
    assert_eq!(response.text(), "sent in chunks, length unknown");
}

#[test]
fn a_post_carries_a_body_there_and_back() {
    let server = start();
    let url = format!("http://{}/echo", server.address());
    let response = client()
        .post(&url, "application/json", b"{\"n\": 1}")
        .unwrap();
    assert_eq!(response.status, StatusCode::OK);
    assert_eq!(
        response.headers.get("Content-Type"),
        Some("application/json")
    );
    assert_eq!(response.body, b"{\"n\": 1}");
}

#[test]
fn a_head_gets_the_headers_of_a_get_and_no_body() {
    let server = start();
    let response = client()
        .head(&format!("http://{}/hello/world", server.address()))
        .unwrap();
    assert_eq!(response.status, StatusCode::OK);
    assert_eq!(response.headers.get("Content-Length"), Some("12"));
    assert!(response.body.is_empty());

    let empty = client()
        .get(&format!("http://{}/empty", server.address()))
        .unwrap();
    assert_eq!(empty.status, StatusCode::NO_CONTENT);
    assert!(empty.body.is_empty());
}

#[test]
fn localhost_is_understood_without_any_lookup() {
    let server = start();
    let url = format!("http://localhost:{}/hello/you", server.address().port());
    assert_eq!(client().get(&url).unwrap().text(), "hello, you");
}

#[test]
fn failures_to_reach_a_server_are_reported() {
    let mut client = client();
    client.timeout = Duration::from_secs(2);
    // A name needs a resolver, and this client was given none.
    let named = client.get("http://example.com/");
    assert!(matches!(named, Err(HttpError::Resolve(_))));
    assert!(matches!(client.get("not a url"), Err(HttpError::BadUrl(_))));

    // A port where nothing is listening.
    let server = start();
    let address = server.address();
    server.shutdown();
    assert!(matches!(
        client.get(&format!("http://{address}/")),
        Err(HttpError::Io(_))
    ));
}

#[test]
fn a_response_larger_than_the_client_allows_is_refused() {
    let server = start();
    let mut client = client();
    client.max_body = 1000;
    let big = client.get(&format!("http://{}/big", server.address()));
    assert!(matches!(big, Err(HttpError::BadResponse(_))));
}
