use std::io::{Read, Write};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, TcpStream, UdpSocket};
use std::time::Duration;

use crate::error::{DnsError, DnsResult};

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

/// One query and reply over a TCP connection.
///
/// TCP is a stream of bytes with no message boundaries, so each DNS message
/// is sent with a two-byte length in front of it.
#[derive(Debug, Clone)]
pub struct TcpTransport {
    pub timeout: Duration,
    pub port: u16,
}

impl TcpTransport {
    pub fn new(timeout: Duration) -> Self {
        Self {
            timeout,
            port: DNS_PORT,
        }
    }
}

impl Transport for TcpTransport {
    fn exchange(&mut self, server: IpAddr, query: &[u8]) -> DnsResult<Vec<u8>> {
        let length = u16::try_from(query.len())
            .map_err(|_| DnsError::Io("query is too large to send".into()))?;
        let address = SocketAddr::new(server, self.port);
        let mut stream = TcpStream::connect_timeout(&address, self.timeout)?;
        stream.set_read_timeout(Some(self.timeout))?;
        stream.set_write_timeout(Some(self.timeout))?;
        stream.write_all(&length.to_be_bytes())?;
        stream.write_all(query)?;

        let mut prefix = [0; 2];
        stream.read_exact(&mut prefix)?;
        let mut reply = vec![0; usize::from(u16::from_be_bytes(prefix))];
        stream.read_exact(&mut reply)?;
        Ok(reply)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;
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

    /// Answer the first connection with `reply` followed by an echo of the query.
    fn serve_tcp(listener: TcpListener, reply: &'static [u8]) {
        thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut prefix = [0; 2];
            stream.read_exact(&mut prefix).unwrap();
            let mut query = vec![0; usize::from(u16::from_be_bytes(prefix))];
            stream.read_exact(&mut query).unwrap();
            let message = [reply, &query].concat();
            stream
                .write_all(&(message.len() as u16).to_be_bytes())
                .unwrap();
            stream.write_all(&message).unwrap();
        });
    }

    #[test]
    fn tcp_frames_each_message_with_its_length() {
        let listener = TcpListener::bind((LOCALHOST, 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        serve_tcp(listener, b"reply:");
        let mut transport = TcpTransport {
            timeout: Duration::from_secs(2),
            port,
        };
        let reply = transport.exchange(LOCALHOST, b"query");
        assert_eq!(reply, Ok(b"reply:query".to_vec()));
    }

    #[test]
    fn tcp_reports_a_closed_port_as_a_network_error() {
        // Bind and immediately drop, so the port is known to be closed.
        let port = TcpListener::bind((LOCALHOST, 0))
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        let mut transport = TcpTransport {
            timeout: Duration::from_secs(2),
            port,
        };
        assert!(matches!(
            transport.exchange(LOCALHOST, b"query"),
            Err(DnsError::Io(_))
        ));
    }
}
