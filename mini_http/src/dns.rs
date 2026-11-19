use std::net::{IpAddr, Ipv4Addr};
use std::time::Duration;

use mini_dns_resolver::name::Name;
use mini_dns_resolver::record::RData;
use mini_dns_resolver::resolver::{Config, Resolver};
use mini_dns_resolver::rtype::RecordType;
use mini_dns_resolver::transport::{NetTransport, Transport};

use crate::client::Resolve;
use crate::error::{HttpError, HttpResult};

/// Finds addresses with the sibling `mini_dns_resolver` project.
///
/// This is the whole meeting point between DNS and HTTP: one function that
/// takes a name and returns an address. The resolver walks the DNS hierarchy
/// from the root servers to find it, and the HTTP client then connects to
/// whatever address comes back. Neither knows how the other works.
pub struct DnsResolver<T> {
    resolver: Resolver<T>,
}

impl DnsResolver<NetTransport> {
    /// A resolver that asks the real DNS servers of the internet.
    pub fn on_the_internet(timeout: Duration) -> Self {
        Self::new(Resolver::new(NetTransport::new(timeout), Config::default()))
    }
}

impl<T: Transport> DnsResolver<T> {
    pub fn new(resolver: Resolver<T>) -> Self {
        Self { resolver }
    }

    pub fn inner(&self) -> &Resolver<T> {
        &self.resolver
    }
}

impl<T: Transport> Resolve for DnsResolver<T> {
    fn resolve(&mut self, host: &str) -> HttpResult<IpAddr> {
        // An address needs no looking up, and `localhost` is this machine
        // by definition, not something to ask the internet about.
        if let Ok(address) = host.parse() {
            return Ok(address);
        }
        if host == "localhost" {
            return Ok(IpAddr::V4(Ipv4Addr::LOCALHOST));
        }
        let failed = |reason: String| HttpError::Resolve(reason);
        let name = Name::parse(host).map_err(|error| failed(error.to_string()))?;
        let records = self
            .resolver
            .resolve(&name, RecordType::A)
            .map_err(|error| failed(error.to_string()))?;
        // The answer may start with CNAME records. The first address wins.
        let address = records.iter().find_map(|record| match record.data {
            RData::A(address) => Some(IpAddr::V4(address)),
            _ => None,
        });
        address.ok_or_else(|| failed(format!("{host} has no IPv4 address")))
    }
}
