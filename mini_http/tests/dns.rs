//! A name looked up with `mini_dns_resolver`, then fetched over HTTP.
//!
//! Nothing here touches the internet. The DNS side is answered by a fake
//! name server in memory, and the HTTP side by this project's own server on
//! localhost. Together they exercise the whole path a browser takes.

use std::net::{IpAddr, Ipv4Addr};

use mini_dns_resolver::error::DnsResult;
use mini_dns_resolver::header::{Header, Rcode};
use mini_dns_resolver::message::Message;
use mini_dns_resolver::name::Name;
use mini_dns_resolver::record::{RData, Record};
use mini_dns_resolver::resolver::{Config, Resolver};
use mini_dns_resolver::rtype::RecordType;
use mini_dns_resolver::transport::Transport;

use mini_http::client::{Client, Resolve};
use mini_http::dns::DnsResolver;
use mini_http::error::HttpError;
use mini_http::request::Request;
use mini_http::response::Response;
use mini_http::server::{Server, ServerConfig, ServerHandle};
use mini_http::status::StatusCode;

/// A name server that knows one name, `shop.test`, and says it lives on
/// this machine. Every question it receives is counted.
#[derive(Default)]
struct OneNameServer {
    questions: usize,
}

impl Transport for OneNameServer {
    fn exchange(&mut self, _server: IpAddr, query: &[u8]) -> DnsResult<Vec<u8>> {
        self.questions += 1;
        let query = Message::decode(query)?;
        let question = &query.questions[0];
        let mut reply = Message {
            header: Header {
                id: query.header.id,
                response: true,
                authoritative: true,
                ..Header::default()
            },
            questions: query.questions.clone(),
            ..Message::default()
        };
        if question.name == Name::parse("shop.test").unwrap() && question.rtype == RecordType::A {
            let address = RData::A(Ipv4Addr::LOCALHOST);
            reply.answers.push(Record::new(
                question.name.clone(),
                RecordType::A,
                300,
                address,
            ));
        } else {
            reply.header.rcode = Rcode::NxDomain;
        }
        Ok(reply.encode())
    }
}

fn resolver() -> DnsResolver<OneNameServer> {
    let config = Config {
        roots: vec![IpAddr::V4(Ipv4Addr::new(192, 0, 2, 1))],
        ..Config::default()
    };
    DnsResolver::new(Resolver::new(OneNameServer::default(), config))
}

fn start() -> ServerHandle {
    let service = |request: &Request| {
        // Echo the Host header: the server sees the name, not the address.
        let host = request.header("Host").unwrap_or("?");
        Response::text(StatusCode::OK, format!("welcome to {host}"))
    };
    Server::bind("127.0.0.1:0", service, ServerConfig::default())
        .unwrap()
        .spawn()
        .unwrap()
}

#[test]
fn a_name_is_resolved_and_then_fetched() {
    let server = start();
    let port = server.address().port();
    let mut client = Client::new(resolver());

    let response = client.get(&format!("http://shop.test:{port}/")).unwrap();
    assert_eq!(response.status, StatusCode::OK);
    // DNS turned the name into 127.0.0.1 for the connection, but the request
    // itself still said which site it wanted.
    assert_eq!(response.text(), format!("welcome to shop.test:{port}"));
    assert_eq!(client.resolver().inner().transport().questions, 1);
}

#[test]
fn a_second_request_reuses_the_cached_address() {
    let server = start();
    let port = server.address().port();
    let mut client = Client::new(resolver());
    for _ in 0..3 {
        client.get(&format!("http://shop.test:{port}/")).unwrap();
    }
    // Three HTTP requests, one DNS question: the answer's TTL has not run out.
    assert_eq!(client.resolver().inner().transport().questions, 1);
}

#[test]
fn a_name_that_does_not_exist_never_reaches_http() {
    let mut client = Client::new(resolver());
    let result = client.get("http://nowhere.test/");
    let Err(HttpError::Resolve(reason)) = result else {
        panic!("expected a resolution failure, got {result:?}");
    };
    assert!(reason.contains("nowhere.test. does not exist"), "{reason}");
}

#[test]
fn addresses_and_localhost_skip_dns_entirely() {
    let mut resolver = resolver();
    assert_eq!(
        resolver.resolve("203.0.113.9"),
        Ok("203.0.113.9".parse().unwrap())
    );
    assert_eq!(
        resolver.resolve("localhost"),
        Ok(IpAddr::V4(Ipv4Addr::LOCALHOST))
    );
    assert_eq!(resolver.inner().transport().questions, 0);
    assert!(matches!(
        resolver.resolve("not a name"),
        Err(HttpError::Resolve(_))
    ));
}
