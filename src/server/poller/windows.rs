use std::collections::HashMap;
use std::io;
use std::os::windows::io::RawSocket;
use super::{PollEvent, PollInterest, Poller};

pub type SocketHandle = RawSocket;

#[repr(C)]
#[derive(Debug, Copy, Clone)]
struct WSAPOLLFD {
    fd: usize,
    events: i16,
    revents: i16,
}

const POLLRDNORM: i16 = 0x0100;
const POLLRDBAND: i16 = 0x0200;
const POLLIN: i16 = POLLRDNORM | POLLRDBAND;
const POLLWRNORM: i16 = 0x0010;
const POLLOUT: i16 = POLLWRNORM;
const POLLERR: i16 = 0x0001;
const POLLHUP: i16 = 0x0002;
const POLLNVAL: i16 = 0x0004;

#[link(name = "ws2_32")]
unsafe extern "system" {
    fn WSAPoll(fdArray: *mut WSAPOLLFD, fds: u32, timeout: i32) -> i32;
}

pub struct WindowsPoller {
    // Map: RawSocket -> (token, PollInterest)
    registered: HashMap<RawSocket, (usize, PollInterest)>,
}

impl WindowsPoller {
    pub fn new() -> io::Result<Self> {
        Ok(WindowsPoller {
            registered: HashMap::new(),
        })
    }
}

impl Poller for WindowsPoller {
    fn register(&mut self, token: usize, handle: SocketHandle, interest: PollInterest) -> io::Result<()> {
        self.registered.insert(handle, (token, interest));
        Ok(())
    }

    fn reregister(&mut self, token: usize, handle: SocketHandle, interest: PollInterest) -> io::Result<()> {
        self.registered.insert(handle, (token, interest));
        Ok(())
    }

    fn deregister(&mut self, handle: SocketHandle) -> io::Result<()> {
        self.registered.remove(&handle);
        Ok(())
    }

    fn poll(&mut self, events: &mut Vec<PollEvent>, timeout_ms: i32) -> io::Result<usize> {
        events.clear();
        if self.registered.is_empty() {
            if timeout_ms > 0 {
                std::thread::sleep(std::time::Duration::from_millis(timeout_ms as u64));
            }
            return Ok(0);
        }

        let mut poll_fds: Vec<WSAPOLLFD> = Vec::with_capacity(self.registered.len());
        let mut tokens: Vec<usize> = Vec::with_capacity(self.registered.len());

        for (&sock, &(token, interest)) in &self.registered {
            let mut ev: i16 = 0;
            if interest.readable {
                ev |= POLLIN;
            }
            if interest.writable {
                ev |= POLLOUT;
            }
            poll_fds.push(WSAPOLLFD {
                fd: sock as usize,
                events: ev,
                revents: 0,
            });
            tokens.push(token);
        }

        let ret = unsafe {
            WSAPoll(
                poll_fds.as_mut_ptr(),
                poll_fds.len() as u32,
                timeout_ms,
            )
        };

        if ret < 0 {
            return Err(io::Error::last_os_error());
        }

        let mut ready_count = 0;
        for (i, pfd) in poll_fds.iter().enumerate() {
            if pfd.revents != 0 {
                let readable = (pfd.revents & POLLIN) != 0;
                let writable = (pfd.revents & POLLOUT) != 0;
                let hangup = (pfd.revents & POLLHUP) != 0;
                let error = (pfd.revents & (POLLERR | POLLNVAL)) != 0;

                events.push(PollEvent {
                    token: tokens[i],
                    readable,
                    writable,
                    hangup,
                    error,
                });
                ready_count += 1;
            }
        }

        Ok(ready_count)
    }
}
