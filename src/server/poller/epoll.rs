use std::io;
use std::os::unix::io::RawFd;
use super::{PollEvent, PollInterest, Poller};

pub type SocketHandle = RawFd;

pub struct EpollPoller {
    epoll_fd: RawFd,
}

impl EpollPoller {
    pub fn new() -> io::Result<Self> {
        let fd = unsafe { libc::epoll_create1(libc::EPOLL_CLOEXEC) };
        if fd < 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(EpollPoller { epoll_fd: fd })
    }
}

impl Drop for EpollPoller {
    fn drop(&mut self) {
        unsafe {
            libc::close(self.epoll_fd);
        }
    }
}

impl Poller for EpollPoller {
    fn register(&mut self, token: usize, handle: SocketHandle, interest: PollInterest) -> io::Result<()> {
        let mut event = libc::epoll_event {
            events: interest_to_epoll(interest),
            u64: token as u64,
        };
        let ret = unsafe { libc::epoll_ctl(self.epoll_fd, libc::EPOLL_CTL_ADD, handle, &mut event) };
        if ret < 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(())
        }
    }

    fn reregister(&mut self, token: usize, handle: SocketHandle, interest: PollInterest) -> io::Result<()> {
        let mut event = libc::epoll_event {
            events: interest_to_epoll(interest),
            u64: token as u64,
        };
        let ret = unsafe { libc::epoll_ctl(self.epoll_fd, libc::EPOLL_CTL_MOD, handle, &mut event) };
        if ret < 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(())
        }
    }

    fn deregister(&mut self, handle: SocketHandle) -> io::Result<()> {
        let ret = unsafe { libc::epoll_ctl(self.epoll_fd, libc::EPOLL_CTL_DEL, handle, std::ptr::null_mut()) };
        if ret < 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(())
        }
    }

    fn poll(&mut self, events: &mut Vec<PollEvent>, timeout_ms: i32) -> io::Result<usize> {
        events.clear();
        let mut ep_events: [libc::epoll_event; 1024] = unsafe { std::mem::zeroed() };

        let nfds = unsafe {
            libc::epoll_wait(
                self.epoll_fd,
                ep_events.as_mut_ptr(),
                ep_events.len() as libc::c_int,
                timeout_ms,
            )
        };

        if nfds < 0 {
            let err = io::Error::last_os_error();
            if err.raw_os_error() == Some(libc::EINTR) {
                return Ok(0);
            }
            return Err(err);
        }

        for i in 0..nfds as usize {
            let ev = ep_events[i];
            let raw_events = ev.events as u32;

            let readable = (raw_events & (libc::EPOLLIN as u32 | libc::EPOLLRDHUP as u32)) != 0;
            let writable = (raw_events & (libc::EPOLLOUT as u32)) != 0;
            let hangup = (raw_events & (libc::EPOLLHUP as u32 | libc::EPOLLRDHUP as u32)) != 0;
            let error = (raw_events & (libc::EPOLLERR as u32)) != 0;

            events.push(PollEvent {
                token: ev.u64 as usize,
                readable,
                writable,
                hangup,
                error,
            });
        }

        Ok(nfds as usize)
    }
}

fn interest_to_epoll(interest: PollInterest) -> u32 {
    let mut flags = libc::EPOLLET as u32 | libc::EPOLLRDHUP as u32;
    if interest.readable {
        flags |= libc::EPOLLIN as u32;
    }
    if interest.writable {
        flags |= libc::EPOLLOUT as u32;
    }
    flags
}
