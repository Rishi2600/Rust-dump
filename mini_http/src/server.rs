use std::net::{SocketAddr, TcpListener, TcpStream, ToSocketAddrs};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use crate::connection::serve_connection;
use crate::error::HttpResult;
use crate::pool::ThreadPool;
use crate::reader::Limits;
use crate::service::Service;

/// How the server is set up.
#[derive(Debug, Clone)]
pub struct ServerConfig {
    /// Worker threads, which is also how many connections are served at once.
    pub threads: usize,
    pub limits: Limits,
    /// How long a connection may stay silent, or refuse to take data, before
    /// it is closed.
    pub timeout: Duration,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            threads: 8,
            limits: Limits::default(),
            timeout: Duration::from_secs(10),
        }
    }
}

/// A listening socket and the service that answers what arrives on it.
pub struct Server {
    listener: TcpListener,
    service: Arc<dyn Service>,
    config: ServerConfig,
}

impl Server {
    /// Claim `address` and get ready to accept connections on it.
    ///
    /// Port 0 asks the operating system for any free port, which
    /// [`Server::local_addr`] then reports.
    pub fn bind(
        address: impl ToSocketAddrs,
        service: impl Service + 'static,
        config: ServerConfig,
    ) -> HttpResult<Self> {
        Ok(Self {
            listener: TcpListener::bind(address)?,
            service: Arc::new(service),
            config,
        })
    }

    pub fn local_addr(&self) -> HttpResult<SocketAddr> {
        Ok(self.listener.local_addr()?)
    }

    /// Serve connections on this thread, forever.
    pub fn run(self) {
        self.accept_loop(&AtomicBool::new(false));
    }

    /// Serve connections on a background thread. The returned handle stops
    /// the server when asked to, or when it is dropped.
    pub fn spawn(self) -> HttpResult<ServerHandle> {
        let address = self.local_addr()?;
        let stop = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&stop);
        let thread = thread::spawn(move || self.accept_loop(&flag));
        Ok(ServerHandle {
            address,
            stop,
            thread: Some(thread),
        })
    }

    /// Take connections off the listening socket and hand each to the pool.
    fn accept_loop(self, stop: &AtomicBool) {
        let pool = ThreadPool::new(self.config.threads);
        let config = Arc::new(self.config);
        for stream in self.listener.incoming() {
            if stop.load(Ordering::SeqCst) {
                break;
            }
            let Ok(stream) = stream else {
                continue;
            };
            let (service, config) = (Arc::clone(&self.service), Arc::clone(&config));
            pool.execute(move || handle_connection(stream, &*service, &config));
        }
        // Leaving this function drops the pool, which waits for the
        // connections still being served.
    }
}

fn handle_connection(stream: TcpStream, service: &dyn Service, config: &ServerConfig) {
    // Send each response as soon as it is written, instead of letting the
    // kernel hold small writes back in the hope of batching them.
    let _ = stream.set_nodelay(true);
    // A worker waits on one connection at a time. Without a deadline, a
    // client could occupy it forever by connecting and saying nothing, or by
    // sending a request one byte a minute, and a handful of such clients
    // would use up the whole pool.
    let _ = stream.set_read_timeout(Some(config.timeout));
    let _ = stream.set_write_timeout(Some(config.timeout));
    // One socket, two handles: reading and writing happen through separate
    // values, so the connection code can own one and borrow the other.
    let Ok(mut writer) = stream.try_clone() else {
        return;
    };
    serve_connection(stream, &mut writer, service, &config.limits);
}

/// A running server.
pub struct ServerHandle {
    address: SocketAddr,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl ServerHandle {
    pub fn address(&self) -> SocketAddr {
        self.address
    }

    /// Stop accepting connections and wait for those in progress to finish.
    pub fn shutdown(mut self) {
        self.stop_and_wait();
    }

    fn stop_and_wait(&mut self) {
        let Some(thread) = self.thread.take() else {
            return;
        };
        self.stop.store(true, Ordering::SeqCst);
        // The accept loop is asleep waiting for a connection. Making one
        // wakes it up so that it notices the flag.
        let _ = TcpStream::connect(self.address);
        let _ = thread.join();
    }
}

impl Drop for ServerHandle {
    fn drop(&mut self) {
        self.stop_and_wait();
    }
}
