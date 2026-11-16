use std::net::IpAddr;

use crate::classify::{Outcome, Referral, classify};
use crate::error::{DnsError, DnsResult};
use crate::ids::IdGenerator;
use crate::message::Message;
use crate::name::Name;
use crate::record::Record;
use crate::roots::root_addresses;
use crate::rtype::RecordType;
use crate::transport::Transport;

/// Limits and starting points for resolution.
#[derive(Debug, Clone)]
pub struct Config {
    /// Servers every lookup starts from.
    pub roots: Vec<IpAddr>,
    /// Referrals followed for one name before giving up.
    pub max_referrals: usize,
    /// CNAME redirections followed for one question before giving up.
    pub max_cnames: usize,
    /// How deeply lookups of nameserver addresses may nest.
    pub max_depth: usize,
    /// UDP payload size to advertise with EDNS, or `None` to not use EDNS.
    pub edns_payload: Option<u16>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            roots: root_addresses(),
            max_referrals: 16,
            max_cnames: 8,
            max_depth: 4,
            // Small enough to avoid IP fragmentation on almost any path.
            edns_payload: Some(1232),
        }
    }
}

/// Turns names into records by asking servers through a [`Transport`].
pub struct Resolver<T> {
    transport: T,
    config: Config,
    ids: IdGenerator,
}

impl<T: Transport> Resolver<T> {
    pub fn new(transport: T, config: Config) -> Self {
        Self {
            transport,
            config,
            ids: IdGenerator::new(),
        }
    }

    pub fn transport(&self) -> &T {
        &self.transport
    }

    /// Send one question to one server and return its reply, once the reply
    /// has been checked against the query.
    fn query(
        &mut self,
        server: IpAddr,
        name: &Name,
        rtype: RecordType,
        recursion: bool,
    ) -> DnsResult<Message> {
        let mut query = Message::query(self.ids.next_id(), name.clone(), rtype);
        query.header.recursion_desired = recursion;
        if let Some(payload) = self.config.edns_payload {
            query = query.with_edns(payload);
        }
        let reply = self.transport.exchange(server, &query.encode())?;
        let response = Message::decode(&reply)?;
        response.check_reply_to(&query)?;
        Ok(response)
    }

    /// Hand the whole job to a recursive resolver, the way the stub resolver
    /// in an operating system does.
    pub fn forward(
        &mut self,
        server: IpAddr,
        name: &Name,
        rtype: RecordType,
    ) -> DnsResult<Vec<Record>> {
        let response = self.query(server, name, rtype, true)?;
        match classify(&response, name, rtype) {
            Outcome::Answer(records) => Ok(records),
            Outcome::Alias { chain, .. } => Ok(chain),
            Outcome::NoData | Outcome::Referral(_) => Ok(Vec::new()),
            Outcome::NxDomain => Err(DnsError::NxDomain(name.to_string())),
            Outcome::Failure(rcode) => Err(DnsError::ServerFailure(format!(
                "{server} answered {rcode}"
            ))),
        }
    }
}

impl<T: Transport> Resolver<T> {
    /// Resolve a name ourselves, starting at the root servers and following
    /// referrals down the tree until an authoritative server answers.
    ///
    /// An empty list means the name exists but has no records of this type.
    pub fn resolve(&mut self, name: &Name, rtype: RecordType) -> DnsResult<Vec<Record>> {
        match self.lookup(name, rtype)? {
            Outcome::Answer(records) => Ok(records),
            Outcome::Alias { chain, .. } => Ok(chain),
            _ => Ok(Vec::new()),
        }
    }

    /// Follow referrals until some server gives a final reply for `name`.
    fn lookup(&mut self, name: &Name, rtype: RecordType) -> DnsResult<Outcome> {
        let (mut zone, mut servers) = (Name::root(), self.config.roots.clone());
        for _ in 0..self.config.max_referrals {
            match self.ask(&servers, &zone, name, rtype)? {
                Outcome::Referral(referral) => {
                    servers = self.nameserver_addresses(&referral)?;
                    zone = referral.zone;
                }
                Outcome::NxDomain => return Err(DnsError::NxDomain(name.to_string())),
                outcome => return Ok(outcome),
            }
        }
        Err(DnsError::TooManyReferrals)
    }

    /// Put the question to the servers of `zone`.
    fn ask(
        &mut self,
        servers: &[IpAddr],
        zone: &Name,
        name: &Name,
        rtype: RecordType,
    ) -> DnsResult<Outcome> {
        match servers.first() {
            Some(&server) => self.ask_one(server, zone, name, rtype),
            None => Err(DnsError::NoNameservers(zone.to_string())),
        }
    }

    /// Put the question to one server of `zone` and work out what its reply
    /// means. A server that does not behave like an authority for the zone
    /// is treated as having failed.
    fn ask_one(
        &mut self,
        server: IpAddr,
        zone: &Name,
        name: &Name,
        rtype: RecordType,
    ) -> DnsResult<Outcome> {
        let mut response = self.query(server, name, rtype, false)?;
        keep_in_bailiwick(&mut response, zone);
        let lame = |reason: String| Err(DnsError::ServerFailure(format!("{server} {reason}")));
        match classify(&response, name, rtype) {
            Outcome::Failure(rcode) => lame(format!("answered {rcode}")),
            // Each referral must lead further down the tree, or a hostile or
            // broken server could send us round in circles.
            Outcome::Referral(referral) if referral.zone.label_count() <= zone.label_count() => {
                lame(format!("referred {name} back to {}", referral.zone))
            }
            Outcome::NoData if !response.header.authoritative => {
                lame(format!("is not authoritative for {zone}"))
            }
            outcome => Ok(outcome),
        }
    }

    /// Addresses of the servers a referral points at.
    fn nameserver_addresses(&mut self, referral: &Referral) -> DnsResult<Vec<IpAddr>> {
        let addresses = referral.glue_addresses();
        if addresses.is_empty() {
            return Err(DnsError::NoNameservers(referral.zone.to_string()));
        }
        Ok(addresses)
    }
}

/// Discard records about names outside the zone a server was asked about.
///
/// A server for `example.com.` has no say over `bank.com.`. Believing what it
/// volunteers about other zones is how a resolver gets poisoned, so anything
/// outside its bailiwick is dropped before the reply is looked at.
fn keep_in_bailiwick(response: &mut Message, zone: &Name) {
    let inside = |record: &Record| record.name.is_subdomain_of(zone);
    response.answers.retain(inside);
    response.authorities.retain(inside);
    response.additionals.retain(inside);
}
