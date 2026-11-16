use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, UdpSocket};
use std::time::Duration;

use crate::error::DnsResult;

/// The well-known port DNS servers listen on.
pub const DNS_PORT: u16 = 53;

/// Room for the largest UDP reply this resolver ever asks for.
const MAX_UDP_REPLY: usize = 4096;

/// A way of getting one query to one server and its reply back.
///
/// The resolver only ever talks through this trait, so tests can swap the
/// network for an in-memory fake.
pub trait Transport {
    fn exchange(&mut self, server: IpAddr, query: &[u8]) -> DnsResult<Vec<u8>>;
}

/// One datagram out, one datagram back.
#[derive(Debug, Clone)]
pub struct UdpTransport {
    /// How long to wait for the reply.
    pub timeout: Duration,
    pub port: u16,
}

impl UdpTransport {
    pub fn new(timeout: Duration) -> Self {
        Self {
            timeout,
            port: DNS_PORT,
        }
    }
}

impl Transport for UdpTransport {
    fn exchange(&mut self, server: IpAddr, query: &[u8]) -> DnsResult<Vec<u8>> {
        // Port 0 lets the operating system pick a free source port.
        let local: SocketAddr = match server {
            IpAddr::V4(_) => (Ipv4Addr::UNSPECIFIED, 0).into(),
            IpAddr::V6(_) => (Ipv6Addr::UNSPECIFIED, 0).into(),
        };
        let socket = UdpSocket::bind(local)?;
        socket.set_read_timeout(Some(self.timeout))?;
        // "Connecting" a UDP socket sends nothing. It tells the kernel to
        // discard datagrams that come from any other address.
        socket.connect((server, self.port))?;
        socket.send(query)?;
        let mut buffer = [0; MAX_UDP_REPLY];
        let received = socket.recv(&mut buffer)?;
        Ok(buffer[..received].to_vec())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::DnsError;
    use std::thread;

    const LOCALHOST: IpAddr = IpAddr::V4(Ipv4Addr::LOCALHOST);

    /// Answer the first datagram with `reply` followed by an echo of the
    /// query, or stay silent when there is no reply.
    fn serve_udp(socket: UdpSocket, reply: Option<&'static [u8]>) {
        thread::spawn(move || {
            let mut buffer = [0; 512];
            let (len, peer) = socket.recv_from(&mut buffer).unwrap();
            match reply {
                Some(reply) => {
                    let datagram = [reply, &buffer[..len]].concat();
                    socket.send_to(&datagram, peer).unwrap();
                }
                // Keep the socket open so the client sees silence, not a refusal.
                None => thread::sleep(Duration::from_secs(1)),
            }
        });
    }

    fn local_udp() -> (UdpSocket, u16) {
        let socket = UdpSocket::bind((LOCALHOST, 0)).unwrap();
        let port = socket.local_addr().unwrap().port();
        (socket, port)
    }

    #[test]
    fn udp_exchanges_one_datagram_each_way() {
        let (socket, port) = local_udp();
        serve_udp(socket, Some(b"reply:"));
        let mut transport = UdpTransport {
            timeout: Duration::from_secs(2),
            port,
        };
        let reply = transport.exchange(LOCALHOST, b"query");
        assert_eq!(reply, Ok(b"reply:query".to_vec()));
    }

    #[test]
    fn udp_silence_is_reported_as_a_timeout() {
        let (socket, port) = local_udp();
        serve_udp(socket, None);
        let mut transport = UdpTransport {
            timeout: Duration::from_millis(50),
            port,
        };
        assert_eq!(
            transport.exchange(LOCALHOST, b"query"),
            Err(DnsError::Timeout)
        );
    }
}
