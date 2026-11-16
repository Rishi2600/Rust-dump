//! The resolver, tested against a small fake internet held in memory.

use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr};

use mini_dns_resolver::error::{DnsError, DnsResult};
use mini_dns_resolver::header::{Header, Rcode};
use mini_dns_resolver::message::Message;
use mini_dns_resolver::name::Name;
use mini_dns_resolver::record::{RData, Record};
use mini_dns_resolver::resolver::{Config, Resolver};
use mini_dns_resolver::rtype::RecordType;
use mini_dns_resolver::transport::Transport;

// Every fake server lives at 192.0.2.x and is known by that last byte.
const ROOT: u8 = 1;
const COM: u8 = 2;
const NET: u8 = 3;
const EXAMPLE_NS1: u8 = 10;
const EXAMPLE_NS2: u8 = 11;
const DNS_HOST: u8 = 20;
const GLUELESS: u8 = 30;
const STUCK: u8 = 41;
const BOUNCE: u8 = 42;

fn name(text: &str) -> Name {
    Name::parse(text).expect("valid name")
}

fn ip(last: u8) -> IpAddr {
    IpAddr::V4(Ipv4Addr::new(192, 0, 2, last))
}

fn a(owner: &str, address: [u8; 4]) -> Record {
    let data = RData::A(Ipv4Addr::from(address));
    Record::new(name(owner), RecordType::A, 300, data)
}

fn cname(owner: &str, target: &str) -> Record {
    let data = RData::Cname(name(target));
    Record::new(name(owner), RecordType::Cname, 300, data)
}

/// One authoritative server: the records it holds and the child zones it
/// has handed to other servers.
#[derive(Clone, Default)]
struct Server {
    records: Vec<Record>,
    /// Zone, then each nameserver with the last byte of its glue address.
    delegations: Vec<(Name, Vec<(Name, Option<u8>)>)>,
}

impl Server {
    fn with(mut self, record: Record) -> Self {
        self.records.push(record);
        self
    }

    fn delegate(mut self, zone: &str, nameservers: &[(&str, Option<u8>)]) -> Self {
        let nameservers = nameservers.iter().map(|(host, glue)| (name(host), *glue));
        self.delegations.push((name(zone), nameservers.collect()));
        self
    }

    fn answer(&self, query: &Message) -> Message {
        let question = &query.questions[0];
        let mut reply = Message {
            header: Header {
                id: query.header.id,
                response: true,
                ..Header::default()
            },
            questions: query.questions.clone(),
            ..Message::default()
        };

        // Names under a delegation belong to someone else: refer the client on.
        let delegated = self
            .delegations
            .iter()
            .find(|(zone, _)| question.name.is_subdomain_of(zone));
        if let Some((zone, nameservers)) = delegated {
            for (host, glue) in nameservers {
                let ns = RData::Ns(host.clone());
                reply
                    .authorities
                    .push(Record::new(zone.clone(), RecordType::Ns, 3600, ns));
                if let Some(last) = glue {
                    let address = RData::A(Ipv4Addr::new(192, 0, 2, *last));
                    reply
                        .additionals
                        .push(Record::new(host.clone(), RecordType::A, 3600, address));
                }
            }
            return reply;
        }

        // Otherwise answer from our own records, chasing CNAMEs we hold.
        reply.header.authoritative = true;
        let mut owner = question.name.clone();
        loop {
            let held = self.records.iter().filter(|record| record.name == owner);
            let found: Vec<Record> = held
                .clone()
                .filter(|record| record.rtype == question.rtype)
                .cloned()
                .collect();
            if !found.is_empty() {
                reply.answers.extend(found);
                break;
            }
            match held
                .clone()
                .find(|record| record.rtype == RecordType::Cname)
            {
                Some(alias) if reply.answers.len() < 4 => {
                    reply.answers.push(alias.clone());
                    if let RData::Cname(target) = &alias.data {
                        owner = target.clone();
                    }
                }
                _ => break,
            }
        }
        let known = self
            .records
            .iter()
            .any(|record| record.name == question.name);
        if !known {
            reply.header.rcode = Rcode::NxDomain;
        }
        reply
    }
}

/// An in-memory network of authoritative servers.
#[derive(Default)]
struct FakeNet {
    servers: HashMap<IpAddr, Server>,
    /// Every query received: the server's last address byte and the question.
    queries: Vec<(u8, String)>,
    last_query: Option<Message>,
}

impl FakeNet {
    /// The servers that were asked, in order.
    fn asked(&self) -> Vec<u8> {
        self.queries.iter().map(|(server, _)| *server).collect()
    }
}

impl Transport for FakeNet {
    fn exchange(&mut self, server: IpAddr, query: &[u8]) -> DnsResult<Vec<u8>> {
        let query = Message::decode(query)?;
        let question = &query.questions[0];
        let last = match server {
            IpAddr::V4(address) => address.octets()[3],
            IpAddr::V6(_) => 0,
        };
        self.queries
            .push((last, format!("{} {}", question.name, question.rtype)));
        // An address nobody listens on behaves like a dead server.
        let reply = self.servers.get(&server).map(|zone| zone.answer(&query));
        self.last_query = Some(query);
        reply
            .map(|message| message.encode())
            .ok_or(DnsError::Timeout)
    }
}

/// The fake internet:
///
/// ```text
/// .  ->  com  ->  example.com   (two nameservers, with glue)
///            ->  glueless.com  (nameserver lives in dns-host.net, no glue)
///            ->  stuck.com, bounce.com  (misbehaving servers)
///    ->  net  ->  dns-host.net
/// ```
fn internet() -> FakeNet {
    let root = Server::default()
        .delegate("com", &[("a.gtld-servers.net", Some(COM))])
        .delegate("net", &[("b.gtld-servers.net", Some(NET))]);
    let com = Server::default()
        .delegate(
            "example.com",
            &[
                ("ns1.example.com", Some(EXAMPLE_NS1)),
                ("ns2.example.com", Some(EXAMPLE_NS2)),
            ],
        )
        .delegate("glueless.com", &[("ns1.dns-host.net", None)])
        .delegate("stuck.com", &[("ns.stuck.com", Some(STUCK))])
        .delegate("bounce.com", &[("ns.bounce.com", Some(BOUNCE))]);
    let net = Server::default().delegate("dns-host.net", &[("ns.dns-host.net", Some(DNS_HOST))]);
    let example = Server::default()
        .with(a("example.com", [203, 0, 113, 1]))
        .with(a("www.example.com", [203, 0, 113, 10]))
        .with(cname("blog.example.com", "www.example.com"))
        .with(cname("shop.example.com", "store.glueless.com"))
        // A lie: this server has no authority over glueless.com.
        .with(a("store.glueless.com", [6, 6, 6, 6]))
        .with(cname("loop1.example.com", "loop2.example.com"))
        .with(cname("loop2.example.com", "loop1.example.com"));
    let dns_host = Server::default().with(a("ns1.dns-host.net", [192, 0, 2, GLUELESS]));
    let glueless = Server::default().with(a("store.glueless.com", [203, 0, 113, 50]));
    // Refers every question about its own zone back to itself.
    let stuck = Server::default().delegate("stuck.com", &[("ns.stuck.com", Some(STUCK))]);
    // Refers every question back up to the com servers.
    let bounce = Server::default().delegate("com", &[("a.gtld-servers.net", Some(COM))]);

    let servers = [
        (ROOT, root),
        (COM, com),
        (NET, net),
        (EXAMPLE_NS1, example.clone()),
        (EXAMPLE_NS2, example),
        (DNS_HOST, dns_host),
        (GLUELESS, glueless),
        (STUCK, stuck),
        (BOUNCE, bounce),
    ];
    FakeNet {
        servers: servers
            .into_iter()
            .map(|(last, server)| (ip(last), server))
            .collect(),
        ..FakeNet::default()
    }
}

fn config() -> Config {
    Config {
        roots: vec![ip(ROOT)],
        ..Config::default()
    }
}

fn resolver() -> Resolver<FakeNet> {
    Resolver::new(internet(), config())
}

#[test]
fn forward_asks_one_server_with_recursion_requested() {
    let mut resolver = resolver();
    let records = resolver.forward(ip(EXAMPLE_NS1), &name("www.example.com"), RecordType::A);
    assert_eq!(records, Ok(vec![a("www.example.com", [203, 0, 113, 10])]));

    let net = resolver.transport();
    assert_eq!(
        net.queries,
        [(EXAMPLE_NS1, "www.example.com. A".to_string())]
    );
    let sent = net.last_query.as_ref().unwrap();
    assert!(sent.header.recursion_desired);
    assert_eq!(sent.edns_payload(), Some(1232));
}

#[test]
fn forward_reports_missing_names_and_missing_data_differently() {
    let mut resolver = resolver();
    let missing = resolver.forward(ip(EXAMPLE_NS1), &name("nope.example.com"), RecordType::A);
    assert_eq!(missing, Err(DnsError::NxDomain("nope.example.com.".into())));
    // The name exists, it just has no IPv6 address.
    let empty = resolver.forward(ip(EXAMPLE_NS1), &name("www.example.com"), RecordType::Aaaa);
    assert_eq!(empty, Ok(Vec::new()));
}

#[test]
fn forward_surfaces_a_dead_server() {
    let mut resolver = resolver();
    let result = resolver.forward(ip(99), &name("www.example.com"), RecordType::A);
    assert_eq!(result, Err(DnsError::Timeout));
}

/// A network path that tampers with the ID of every reply.
struct WrongId(FakeNet);

impl Transport for WrongId {
    fn exchange(&mut self, server: IpAddr, query: &[u8]) -> DnsResult<Vec<u8>> {
        let mut reply = self.0.exchange(server, query)?;
        reply[0] ^= 0xFF;
        Ok(reply)
    }
}

#[test]
fn a_reply_with_the_wrong_id_is_not_believed() {
    let mut resolver = Resolver::new(WrongId(internet()), config());
    let result = resolver.forward(ip(EXAMPLE_NS1), &name("www.example.com"), RecordType::A);
    assert!(matches!(result, Err(DnsError::BadResponse(_))));
}

fn question(text: &str) -> String {
    format!("{text} A")
}

#[test]
fn resolve_walks_from_the_root_to_the_authoritative_server() {
    let mut resolver = resolver();
    let records = resolver.resolve(&name("www.example.com"), RecordType::A);
    assert_eq!(records, Ok(vec![a("www.example.com", [203, 0, 113, 10])]));

    let net = resolver.transport();
    let asked = question("www.example.com.");
    let expected = [
        (ROOT, asked.clone()),
        (COM, asked.clone()),
        (EXAMPLE_NS1, asked),
    ];
    assert_eq!(net.queries, expected);
    // An iterative resolver does the legwork itself.
    assert!(!net.last_query.as_ref().unwrap().header.recursion_desired);
}

#[test]
fn resolve_reports_missing_names_and_missing_data_differently() {
    let mut resolver = resolver();
    let missing = resolver.resolve(&name("nope.example.com"), RecordType::A);
    assert_eq!(missing, Err(DnsError::NxDomain("nope.example.com.".into())));
    let empty = resolver.resolve(&name("www.example.com"), RecordType::Aaaa);
    assert_eq!(empty, Ok(Vec::new()));
    // A whole top-level domain that does not exist is the root's call.
    let no_tld = resolver.resolve(&name("www.example.invalid"), RecordType::A);
    assert_eq!(
        no_tld,
        Err(DnsError::NxDomain("www.example.invalid.".into()))
    );
}

#[test]
fn resolve_gives_up_after_too_many_referrals() {
    let limited = Config {
        max_referrals: 2,
        ..config()
    };
    let mut resolver = Resolver::new(internet(), limited);
    let result = resolver.resolve(&name("www.example.com"), RecordType::A);
    assert_eq!(result, Err(DnsError::TooManyReferrals));
    assert_eq!(resolver.transport().asked(), [ROOT, COM]);
}

#[test]
fn a_server_that_refers_a_zone_to_itself_is_rejected() {
    let mut resolver = resolver();
    let result = resolver.resolve(&name("www.stuck.com"), RecordType::A);
    assert!(matches!(result, Err(DnsError::ServerFailure(_))));
    assert_eq!(resolver.transport().asked(), [ROOT, COM, STUCK]);
}

#[test]
fn a_server_that_refers_back_up_the_tree_is_rejected() {
    let mut resolver = resolver();
    let result = resolver.resolve(&name("www.bounce.com"), RecordType::A);
    assert!(matches!(result, Err(DnsError::ServerFailure(_))));
    assert_eq!(resolver.transport().asked(), [ROOT, COM, BOUNCE]);
}

#[test]
fn a_delegation_without_glue_triggers_a_lookup_of_the_nameserver() {
    let mut resolver = resolver();
    let records = resolver.resolve(&name("store.glueless.com"), RecordType::A);
    assert_eq!(
        records,
        Ok(vec![a("store.glueless.com", [203, 0, 113, 50])])
    );

    // The walk for the original name pauses at COM while a second walk
    // finds the address of ns1.dns-host.net, then resumes.
    let net = resolver.transport();
    assert_eq!(net.asked(), [ROOT, COM, ROOT, NET, DNS_HOST, GLUELESS]);
    assert_eq!(net.queries[4], (DNS_HOST, question("ns1.dns-host.net.")));
}

#[test]
fn nested_nameserver_lookups_are_bounded() {
    let shallow = Config {
        max_depth: 0,
        ..config()
    };
    let mut resolver = Resolver::new(internet(), shallow);
    let result = resolver.resolve(&name("store.glueless.com"), RecordType::A);
    assert_eq!(result, Err(DnsError::NoNameservers("glueless.com.".into())));
}
