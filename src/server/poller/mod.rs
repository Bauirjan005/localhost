use std::io;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PollInterest {
    pub readable: bool,
    pub writable: bool,
}

impl PollInterest {
    pub const READABLE: PollInterest = PollInterest {
        readable: true,
        writable: false,
    };
    pub const WRITABLE: PollInterest = PollInterest {
        readable: false,
        writable: true,
    };
    pub const BOTH: PollInterest = PollInterest {
        readable: true,
        writable: true,
    };
    pub const NONE: PollInterest = PollInterest {
        readable: false,
        writable: false,
    };
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PollEvent {
    pub token: usize,
    pub readable: bool,
    pub writable: bool,
    pub hangup: bool,
    pub error: bool,
}

#[cfg(target_os = "linux")]
pub mod epoll;
#[cfg(target_os = "linux")]
pub use epoll::{EpollPoller as DefaultPoller, SocketHandle};

#[cfg(not(target_os = "linux"))]
pub mod windows;
#[cfg(not(target_os = "linux"))]
pub use windows::{WindowsPoller as DefaultPoller, SocketHandle};

pub trait AsSocketHandle {
    fn as_socket_handle(&self) -> SocketHandle;
}

#[cfg(target_os = "linux")]
impl<T: std::os::unix::io::AsRawFd> AsSocketHandle for T {
    fn as_socket_handle(&self) -> SocketHandle {
        self.as_raw_fd()
    }
}

#[cfg(not(target_os = "linux"))]
impl<T: std::os::windows::io::AsRawSocket> AsSocketHandle for T {
    fn as_socket_handle(&self) -> SocketHandle {
        self.as_raw_socket()
    }
}

pub trait Poller {
    fn register(&mut self, token: usize, handle: SocketHandle, interest: PollInterest) -> io::Result<()>;
    fn reregister(&mut self, token: usize, handle: SocketHandle, interest: PollInterest) -> io::Result<()>;
    fn deregister(&mut self, handle: SocketHandle) -> io::Result<()>;
    fn poll(&mut self, events: &mut Vec<PollEvent>, timeout_ms: i32) -> io::Result<usize>;
}
