use std::io;
use std::net::{SocketAddr, TcpListener, TcpStream};

pub struct Listener {
    pub token: usize,
    pub host: String,
    pub port: u16,
    pub listener: TcpListener,
}

impl Listener {
    pub fn bind(token: usize, host: &str, port: u16) -> io::Result<Self> {
        let addr_str = format!("{}:{}", host, port);
        let listener = TcpListener::bind(&addr_str)?;
        listener.set_nonblocking(true)?;

        Ok(Listener {
            token,
            host: host.to_string(),
            port,
            listener,
        })
    }

    /// Accepts incoming connections non-blocking.
    /// Returns a list of (stream, peer_addr).
    pub fn accept_all(&self) -> io::Result<Vec<(TcpStream, SocketAddr)>> {
        let mut streams = Vec::new();

        loop {
            match self.listener.accept() {
                Ok((stream, addr)) => {
                    streams.push((stream, addr));
                }
                Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                    break;
                }
                Err(ref e) if e.kind() == io::ErrorKind::Interrupted => {
                    continue;
                }
                Err(e) => {
                    return Err(e);
                }
            }
        }

        Ok(streams)
    }
}
