use std::net::IpAddr;

use crate::header::Rcode;
use crate::message::Message;
use crate::name::Name;
use crate::record::{RData, Record};
use crate::rtype::RecordType;

/// What a reply means for the question that was asked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// Records that answer the question, preceded by any CNAMEs that led to them.
    Answer(Vec<Record>),
    /// The name is an alias and the reply stops there: the lookup has to
    /// start again at `target`.
    Alias { target: Name, chain: Vec<Record> },
    /// The server does not have the answer but knows who is closer to it.
    Referral(Referral),
    /// The name does not exist.
    NxDomain,
    /// The name exists but has no records of the requested type.
    NoData,
    /// The server refused or failed. Another server may do better.
    Failure(Rcode),
}

/// A pointer down the tree: "ask the nameservers of this zone instead".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Referral {
    /// The zone being delegated.
    pub zone: Name,
    /// NS records naming that zone's servers.
    pub nameservers: Vec<Record>,
    /// Address records the parent supplied for those servers ("glue").
    pub glue: Vec<Record>,
}

impl Referral {
    pub fn nameserver_names(&self) -> Vec<Name> {
        let names = self
            .nameservers
            .iter()
            .filter_map(|record| match &record.data {
                RData::Ns(host) => Some(host.clone()),
                _ => None,
            });
        names.collect()
    }

    pub fn glue_addresses(&self) -> Vec<IpAddr> {
        let addresses = self.glue.iter().filter_map(|record| match record.data {
            RData::A(address) => Some(IpAddr::V4(address)),
            _ => None,
        });
        addresses.collect()
    }
}

/// Work out what `response` says about the question `name` / `rtype`.
pub fn classify(response: &Message, name: &Name, rtype: RecordType) -> Outcome {
    match response.header.rcode {
        Rcode::NoError => {}
        Rcode::NxDomain => return Outcome::NxDomain,
        other => return Outcome::Failure(other),
    }

    // Walk the answer section from the name that was asked, hopping across
    // any CNAMEs the server has already followed for us.
    let answers = &response.answers;
    let mut owner = name.clone();
    let mut chain = Vec::new();
    loop {
        let matching = answers
            .iter()
            .filter(|record| record.name == owner && record.rtype == rtype);
        let before = chain.len();
        chain.extend(matching.cloned());
        if chain.len() > before {
            return Outcome::Answer(chain);
        }
        let alias = answers.iter().find_map(|record| match &record.data {
            RData::Cname(target) if record.name == owner => Some((record, target)),
            _ => None,
        });
        match alias {
            // A reply cannot hold more links than it has records, so this
            // bound also stops a CNAME loop inside one reply.
            Some((record, target)) if chain.len() < answers.len() => {
                chain.push(record.clone());
                owner = target.clone();
            }
            _ => break,
        }
    }
    if !chain.is_empty() {
        return Outcome::Alias {
            target: owner,
            chain,
        };
    }

    // A referral comes from a server that is not itself authoritative for
    // the name: it lists nameservers for a zone that contains the name.
    if !response.header.authoritative {
        let delegation = response
            .authorities
            .iter()
            .find(|record| record.rtype == RecordType::Ns && name.is_subdomain_of(&record.name));
        if let Some(first) = delegation {
            let zone = first.name.clone();
            let nameservers: Vec<Record> = response
                .authorities
                .iter()
                .filter(|record| record.rtype == RecordType::Ns && record.name == zone)
                .cloned()
                .collect();
            let hosts: Vec<&Name> = nameservers
                .iter()
                .filter_map(|record| match &record.data {
                    RData::Ns(host) => Some(host),
                    _ => None,
                })
                .collect();
            let glue = response
                .additionals
                .iter()
                .filter(|record| record.rtype == RecordType::A && hosts.contains(&&record.name))
                .cloned()
                .collect();
            return Outcome::Referral(Referral {
                zone,
                nameservers,
                glue,
            });
        }
    }

    Outcome::NoData
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::header::Header;
    use std::net::Ipv4Addr;

    fn name(text: &str) -> Name {
        Name::parse(text).expect("valid name")
    }

    fn a(owner: &str, last: u8) -> Record {
        let address = RData::A(Ipv4Addr::new(192, 0, 2, last));
        Record::new(name(owner), RecordType::A, 300, address)
    }

    fn cname(owner: &str, target: &str) -> Record {
        Record::new(
            name(owner),
            RecordType::Cname,
            300,
            RData::Cname(name(target)),
        )
    }

    fn ns(zone: &str, host: &str) -> Record {
        Record::new(name(zone), RecordType::Ns, 300, RData::Ns(name(host)))
    }

    fn reply(authoritative: bool, rcode: Rcode) -> Message {
        Message {
            header: Header {
                response: true,
                authoritative,
                rcode,
                ..Header::default()
            },
            ..Message::default()
        }
    }

    #[test]
    fn direct_answers_are_picked_out_by_name_and_type() {
        let mut response = reply(true, Rcode::NoError);
        response.answers = vec![a("www.example.com", 1), a("other.example.com", 2)];
        let outcome = classify(&response, &name("www.example.com"), RecordType::A);
        assert_eq!(outcome, Outcome::Answer(vec![a("www.example.com", 1)]));
    }

    #[test]
    fn a_cname_chain_inside_one_reply_is_followed_to_its_answer() {
        let mut response = reply(true, Rcode::NoError);
        // Deliberately out of order: servers are not required to sort them.
        response.answers = vec![
            a("www.example.com", 1),
            cname("alias.example.com", "www.example.com"),
            cname("blog.example.com", "alias.example.com"),
        ];
        let outcome = classify(&response, &name("blog.example.com"), RecordType::A);
        let expected = vec![
            cname("blog.example.com", "alias.example.com"),
            cname("alias.example.com", "www.example.com"),
            a("www.example.com", 1),
        ];
        assert_eq!(outcome, Outcome::Answer(expected));
    }

    #[test]
    fn a_cname_without_its_target_means_start_again_there() {
        let mut response = reply(true, Rcode::NoError);
        response.answers = vec![cname("shop.example.com", "store.other.net")];
        let outcome = classify(&response, &name("shop.example.com"), RecordType::A);
        let expected = Outcome::Alias {
            target: name("store.other.net"),
            chain: response.answers.clone(),
        };
        assert_eq!(outcome, expected);

        // Asking for the CNAME itself is a plain answer.
        let outcome = classify(&response, &name("shop.example.com"), RecordType::Cname);
        assert_eq!(outcome, Outcome::Answer(response.answers.clone()));
    }

    #[test]
    fn a_cname_loop_inside_one_reply_terminates() {
        let mut response = reply(true, Rcode::NoError);
        response.answers = vec![
            cname("a.example.com", "b.example.com"),
            cname("b.example.com", "a.example.com"),
        ];
        let outcome = classify(&response, &name("a.example.com"), RecordType::A);
        assert!(matches!(outcome, Outcome::Alias { .. }));
    }

    #[test]
    fn a_referral_names_the_zone_its_servers_and_their_glue() {
        let mut response = reply(false, Rcode::NoError);
        response.authorities = vec![
            ns("com", "a.gtld-servers.net"),
            ns("com", "b.gtld-servers.net"),
        ];
        response.additionals = vec![a("a.gtld-servers.net", 5), a("unrelated.example.org", 9)];
        let Outcome::Referral(referral) =
            classify(&response, &name("www.example.com"), RecordType::A)
        else {
            panic!("expected a referral");
        };
        assert_eq!(referral.zone, name("com"));
        assert_eq!(
            referral.nameserver_names(),
            [name("a.gtld-servers.net"), name("b.gtld-servers.net")]
        );
        // Only addresses of the listed nameservers count as glue.
        assert_eq!(
            referral.glue_addresses(),
            [IpAddr::V4(Ipv4Addr::new(192, 0, 2, 5))]
        );
    }

    #[test]
    fn nameservers_for_an_unrelated_zone_are_not_a_referral() {
        let mut response = reply(false, Rcode::NoError);
        response.authorities = vec![ns("org", "a.org-servers.net")];
        let outcome = classify(&response, &name("www.example.com"), RecordType::A);
        assert_eq!(outcome, Outcome::NoData);
    }

    #[test]
    fn empty_and_error_replies() {
        let question = name("www.example.com");
        let missing = reply(true, Rcode::NxDomain);
        assert_eq!(
            classify(&missing, &question, RecordType::A),
            Outcome::NxDomain
        );

        let broken = reply(false, Rcode::ServFail);
        assert_eq!(
            classify(&broken, &question, RecordType::A),
            Outcome::Failure(Rcode::ServFail)
        );

        // An authoritative reply with no answer means "no such data", even
        // if the server lists its own nameservers alongside.
        let mut empty = reply(true, Rcode::NoError);
        empty.authorities = vec![ns("example.com", "ns1.example.com")];
        assert_eq!(
            classify(&empty, &question, RecordType::Aaaa),
            Outcome::NoData
        );
    }
}
