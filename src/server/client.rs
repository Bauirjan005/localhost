use std::io::{self, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::time::Instant;
use crate::http::request::{Request, RequestParser};
use crate::http::response::Response;
use crate::http::status::StatusCode;

#[derive(Debug)]
pub enum ClientIoError {
    WouldBlock,
    Disconnected,
    Io(io::Error),
    ProtocolError(StatusCode, String),
}

#[derive(Debug)]
pub enum ClientActivity {
    Reading,
    ReadyToHandle(Request),
    Writing,
    WriteComplete { keep_alive: bool },
    CloseImmediately,
}

pub struct Client {
    pub token: usize,
    pub stream: TcpStream,
    pub peer_addr: SocketAddr,
    pub server_port: u16,
    pub server_host: String,
    pub parser: RequestParser,
    pub outgoing_buffer: Vec<u8>,
    pub outgoing_cursor: usize,
    pub is_keep_alive: bool,
    pub last_active: Instant,
}

impl Client {
    pub fn new(
        token: usize,
        stream: TcpStream,
        peer_addr: SocketAddr,
        server_host: String,
        server_port: u16,
        max_body_size: usize,
    ) -> io::Result<Self> {
        stream.set_nonblocking(true)?;
        let _ = stream.set_nodelay(true);

        Ok(Client {
            token,
            stream,
            peer_addr,
            server_port,
            server_host,
            parser: RequestParser::new(max_body_size),
            outgoing_buffer: Vec::new(),
            outgoing_cursor: 0,
            is_keep_alive: true,
            last_active: Instant::now(),
        })
    }

    pub fn touch(&mut self) {
        self.last_active = Instant::now();
    }

    /// Reads available incoming data from the socket non-blocking.
    pub fn on_readable(&mut self) -> Result<ClientActivity, ClientIoError> {
        self.touch();
        let mut temp_buf = [0u8; 8192];

        loop {
            match self.stream.read(&mut temp_buf) {
                Ok(0) => {
                    // Client closed connection
                    return Err(ClientIoError::Disconnected);
                }
                Ok(n) => {
                    match self.parser.feed(&temp_buf[..n]) {
                        Ok(Some(request)) => {
                            self.is_keep_alive = request.is_keep_alive();
                            return Ok(ClientActivity::ReadyToHandle(request));
                        }
                        Ok(None) => {
                            // More data needed, continue reading if socket has more
                            continue;
                        }
                        Err((status, msg)) => {
                            return Err(ClientIoError::ProtocolError(status, msg));
                        }
                    }
                }
                Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                    return Ok(ClientActivity::Reading);
                }
                Err(ref e) if e.kind() == io::ErrorKind::Interrupted => {
                    continue;
                }
                Err(e) => {
                    return Err(ClientIoError::Io(e));
                }
            }
        }
    }

    /// Queues a response to be written out.
    pub fn queue_response(&mut self, response: Response) {
        self.is_keep_alive = self.is_keep_alive
            && response
                .headers
                .get("connection")
                .map(|c| !c.eq_ignore_ascii_case("close"))
                .unwrap_or(true);

        self.outgoing_buffer = response.to_bytes();
        self.outgoing_cursor = 0;
    }

    /// Writes queued outgoing data non-blocking to the socket.
    pub fn on_writable(&mut self) -> Result<ClientActivity, ClientIoError> {
        self.touch();

        while self.outgoing_cursor < self.outgoing_buffer.len() {
            let slice = &self.outgoing_buffer[self.outgoing_cursor..];
            match self.stream.write(slice) {
                Ok(0) => {
                    return Err(ClientIoError::Disconnected);
                }
                Ok(n) => {
                    self.outgoing_cursor += n;
                }
                Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                    return Ok(ClientActivity::Writing);
                }
                Err(ref e) if e.kind() == io::ErrorKind::Interrupted => {
                    continue;
                }
                Err(e) => {
                    return Err(ClientIoError::Io(e));
                }
            }
        }

        // Response fully written!
        self.outgoing_buffer.clear();
        self.outgoing_cursor = 0;

        if self.is_keep_alive {
            self.parser.reset_for_keep_alive();
            Ok(ClientActivity::WriteComplete { keep_alive: true })
        } else {
            Ok(ClientActivity::WriteComplete { keep_alive: false })
        }
    }
}
