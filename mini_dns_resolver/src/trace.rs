use std::fmt;
use std::net::IpAddr;

use crate::classify::{Outcome, classify};
use crate::error::DnsResult;
use crate::message::Message;
use crate::transport::Transport;

/// One query that was sent, and a summary of what came back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TraceEntry {
    pub server: IpAddr,
    pub question: String,
    pub outcome: String,
}

impl fmt::Display for TraceEntry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "asked {} for {}: {}",
            self.server, self.question, self.outcome
        )
    }
}

/// Wraps a transport and records every exchange that passes through it.
///
/// The resolver does not know it is being watched: tracing is added from the
/// outside, by handing it this in place of the real transport.
pub struct Tracing<T> {
    inner: T,
    entries: Vec<TraceEntry>,
}

impl<T> Tracing<T> {
    pub fn new(inner: T) -> Self {
        Self {
            inner,
            entries: Vec::new(),
        }
    }

    pub fn entries(&self) -> &[TraceEntry] {
        &self.entries
    }

    pub fn inner(&self) -> &T {
        &self.inner
    }
}

impl<T: Transport> Transport for Tracing<T> {
    fn exchange(&mut self, server: IpAddr, query: &[u8]) -> DnsResult<Vec<u8>> {
        let result = self.inner.exchange(server, query);
        let sent = Message::decode(query).ok();
        let question = sent.as_ref().and_then(|message| message.questions.first());
        let outcome = match (&result, question) {
            (Err(error), _) => format!("failed, {error}"),
            (Ok(_), None) => "reply to an unreadable query".to_string(),
            (Ok(reply), Some(question)) => match Message::decode(reply) {
                Ok(response) => describe(classify(&response, &question.name, question.rtype)),
                Err(error) => format!("unreadable reply, {error}"),
            },
        };
        self.entries.push(TraceEntry {
            server,
            question: question.map_or_else(
                || "?".to_string(),
                |question| format!("{} {}", question.name, question.rtype),
            ),
            outcome,
        });
        result
    }
}

fn describe(outcome: Outcome) -> String {
    match outcome {
        Outcome::Answer(records) => format!("answer, {}", count(records.len(), "record")),
        Outcome::Alias { target, .. } => format!("alias for {target}"),
        Outcome::Referral(referral) => format!(
            "referral to {} ({})",
            referral.zone,
            count(referral.nameservers.len(), "nameserver")
        ),
        Outcome::NxDomain => "no such name".to_string(),
        Outcome::NoData => "no records of that type".to_string(),
        Outcome::Failure(rcode) => format!("{rcode}"),
    }
}

fn count(number: usize, noun: &str) -> String {
    let plural = if number == 1 { "" } else { "s" };
    format!("{number} {noun}{plural}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::DnsError;
    use crate::header::Header;
    use crate::name::Name;
    use crate::record::{RData, Record};
    use crate::rtype::RecordType;
    use std::net::Ipv4Addr;

    const SERVER: IpAddr = IpAddr::V4(Ipv4Addr::new(192, 0, 2, 1));

    /// Replies to everything with the same canned result.
    struct Canned(DnsResult<Message>);

    impl Transport for Canned {
        fn exchange(&mut self, _server: IpAddr, _query: &[u8]) -> DnsResult<Vec<u8>> {
            self.0.clone().map(|message| message.encode())
        }
    }

    fn name(text: &str) -> Name {
        Name::parse(text).expect("valid name")
    }

    fn trace_of(canned: DnsResult<Message>) -> String {
        let mut transport = Tracing::new(Canned(canned));
        let query = Message::query(7, name("www.example.com"), RecordType::A).encode();
        let _ = transport.exchange(SERVER, &query);
        assert_eq!(transport.entries().len(), 1);
        transport.entries()[0].to_string()
    }

    fn response() -> Message {
        Message {
            header: Header {
                response: true,
                ..Header::default()
            },
            ..Message::default()
        }
    }

    #[test]
    fn summarises_a_referral() {
        let mut reply = response();
        for host in ["a.gtld-servers.net", "b.gtld-servers.net"] {
            let ns = RData::Ns(name(host));
            reply
                .authorities
                .push(Record::new(name("com"), RecordType::Ns, 60, ns));
        }
        assert_eq!(
            trace_of(Ok(reply)),
            "asked 192.0.2.1 for www.example.com. A: referral to com. (2 nameservers)"
        );
    }

    #[test]
    fn summarises_an_answer_and_a_failure() {
        let mut reply = response();
        let address = RData::A(Ipv4Addr::new(203, 0, 113, 10));
        reply.answers.push(Record::new(
            name("www.example.com"),
            RecordType::A,
            60,
            address,
        ));
        assert!(trace_of(Ok(reply)).ends_with(": answer, 1 record"));
        assert!(
            trace_of(Err(DnsError::Timeout)).ends_with(": failed, timed out waiting for a reply")
        );
    }

    #[test]
    fn passes_the_reply_through_untouched() {
        let reply = response();
        let mut transport = Tracing::new(Canned(Ok(reply.clone())));
        let query = Message::query(7, name("example.com"), RecordType::Mx).encode();
        assert_eq!(transport.exchange(SERVER, &query), Ok(reply.encode()));
        assert!(transport.inner().0.is_ok());
    }
}
