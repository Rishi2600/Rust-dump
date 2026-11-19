//! The server on real sockets, talked to with raw bytes.

use std::io::{Read, Write};
use std::net::{Shutdown, SocketAddr, TcpStream};
use std::thread;
use std::time::{Duration, Instant};

use mini_http::request::Request;
use mini_http::response::Response;
use mini_http::server::{Server, ServerConfig, ServerHandle};
use mini_http::service::Service;
use mini_http::status::StatusCode;

fn start(service: impl Service + 'static) -> ServerHandle {
    let server = Server::bind("127.0.0.1:0", service, ServerConfig::default()).unwrap();
    server.spawn().unwrap()
}

fn start_impatient(service: impl Service + 'static) -> ServerHandle {
    let config = ServerConfig {
        timeout: Duration::from_millis(200),
        ..ServerConfig::default()
    };
    Server::bind("127.0.0.1:0", service, config)
        .unwrap()
        .spawn()
        .unwrap()
}

fn echo_path(request: &Request) -> Response {
    Response::text(StatusCode::OK, format!("path={}", request.path()))
}

/// Send `raw`, say we are done sending, and read until the server closes.
fn roundtrip(address: SocketAddr, raw: &str) -> String {
    let mut stream = TcpStream::connect(address).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    stream.write_all(raw.as_bytes()).unwrap();
    stream.shutdown(Shutdown::Write).unwrap();
    let mut reply = String::new();
    stream.read_to_string(&mut reply).unwrap();
    reply
}

#[test]
fn a_request_over_tcp_gets_a_response() {
    let server = start(echo_path);
    let reply = roundtrip(
        server.address(),
        "GET /hello HTTP/1.1\r\nHost: localhost\r\n\r\n",
    );
    assert!(reply.starts_with("HTTP/1.1 200 OK\r\n"));
    assert!(reply.ends_with("\r\n\r\npath=/hello"));
}

#[test]
fn one_connection_carries_several_requests() {
    let server = start(echo_path);
    let raw = "GET /a HTTP/1.1\r\nHost: x\r\n\r\nGET /b HTTP/1.1\r\nHost: x\r\n\r\nGET /c HTTP/1.1\r\nHost: x\r\n\r\n";
    let reply = roundtrip(server.address(), raw);
    assert_eq!(reply.matches("HTTP/1.1 200 OK").count(), 3);
    let (a, b, c) = (
        reply.find("path=/a"),
        reply.find("path=/b"),
        reply.find("path=/c"),
    );
    assert!(a < b && b < c, "responses come back in the order asked");
}

#[test]
fn many_clients_are_served_at_once() {
    let server = start(echo_path);
    let address = server.address();
    let clients: Vec<_> = (0..32)
        .map(|index| {
            thread::spawn(move || {
                let raw = format!("GET /client/{index} HTTP/1.1\r\nHost: x\r\n\r\n");
                (index, roundtrip(address, &raw))
            })
        })
        .collect();
    for client in clients {
        let (index, reply) = client.join().unwrap();
        assert!(
            reply.ends_with(&format!("path=/client/{index}")),
            "client {index}"
        );
    }
}

#[test]
fn a_slow_request_does_not_hold_up_a_fast_one() {
    let server = start(|request: &Request| {
        if request.path() == "/slow" {
            thread::sleep(Duration::from_millis(600));
        }
        echo_path(request)
    });
    let address = server.address();
    let slow = thread::spawn(move || roundtrip(address, "GET /slow HTTP/1.1\r\nHost: x\r\n\r\n"));
    // Give the slow request time to reach its handler.
    thread::sleep(Duration::from_millis(100));

    let started = Instant::now();
    let fast = roundtrip(address, "GET /fast HTTP/1.1\r\nHost: x\r\n\r\n");
    assert!(fast.ends_with("path=/fast"));
    assert!(
        started.elapsed() < Duration::from_millis(400),
        "the fast request had to wait"
    );
    assert!(slow.join().unwrap().ends_with("path=/slow"));
}

#[test]
fn after_shutdown_the_port_no_longer_answers() {
    let server = start(echo_path);
    let address = server.address();
    assert!(roundtrip(address, "GET / HTTP/1.1\r\nHost: x\r\n\r\n").contains("200 OK"));
    server.shutdown();
    assert!(TcpStream::connect(address).is_err());
}

/// Send `raw` but keep the connection open, then read what the server does.
fn send_and_wait(address: SocketAddr, raw: &str) -> String {
    let mut stream = TcpStream::connect(address).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    stream.write_all(raw.as_bytes()).unwrap();
    let mut reply = String::new();
    stream.read_to_string(&mut reply).unwrap();
    reply
}

#[test]
fn an_idle_connection_is_closed_without_a_word() {
    let server = start_impatient(echo_path);
    let started = Instant::now();
    assert_eq!(send_and_wait(server.address(), ""), "");
    assert!(started.elapsed() >= Duration::from_millis(150));
}

#[test]
fn a_kept_alive_connection_is_closed_once_it_goes_quiet() {
    let server = start_impatient(echo_path);
    // One complete request, then nothing more, without closing our side.
    let reply = send_and_wait(server.address(), "GET /only HTTP/1.1\r\nHost: x\r\n\r\n");
    assert_eq!(reply.matches("HTTP/1.1 ").count(), 1);
    assert!(reply.ends_with("path=/only"));
}

#[test]
fn a_request_that_stalls_half_way_gets_a_408() {
    let server = start_impatient(echo_path);
    let reply = send_and_wait(
        server.address(),
        "GET /slow HTTP/1.1\r\nHost: x\r\nX-Unfinished",
    );
    assert!(reply.starts_with("HTTP/1.1 408 Request Timeout\r\n"));
    assert!(reply.contains("Connection: close"));
}
