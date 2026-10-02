//! macOS transport: a Unix socket, with the peer's uid checked on connect.

use std::os::fd::AsRawFd;
use std::os::unix::net::UnixStream;
use std::path::Path;

pub type Stream = UnixStream;

unsafe extern "C" {
    fn getuid() -> u32;
    fn getpeereid(fd: i32, euid: *mut u32, egid: *mut u32) -> i32;
}

pub fn uid() -> u32 {
    // SAFETY: getuid has no preconditions and cannot fail.
    unsafe { getuid() }
}

/// True only when the process on the other end runs as the current user.
pub fn peer_is_same_user(stream: &UnixStream) -> bool {
    let (mut euid, mut egid) = (u32::MAX, u32::MAX);
    // SAFETY: valid fd for the lifetime of `stream`; out-pointers are valid.
    let ok = unsafe { getpeereid(stream.as_raw_fd(), &mut euid, &mut egid) } == 0;
    ok && euid == uid()
}

/// Connects to the app. `None` if it isn't running or isn't ours.
pub fn connect(path: &Path) -> Option<UnixStream> {
    let stream = UnixStream::connect(path).ok()?;
    peer_is_same_user(&stream).then_some(stream)
}
