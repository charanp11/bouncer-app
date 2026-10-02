//! macOS transport: a Unix socket, with the peer's uid checked on connect.

use std::ffi::OsString;
use std::os::fd::AsRawFd;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};

pub type Stream = UnixStream;

unsafe extern "C" {
    fn getuid() -> u32;
    fn getpeereid(fd: i32, euid: *mut u32, egid: *mut u32) -> i32;
}

pub fn uid() -> u32 {
    // SAFETY: getuid has no preconditions and cannot fail.
    unsafe { getuid() }
}

/// `~/Library/Application Support/Bouncer/bouncer.sock`. Fixed per user, unlike
/// `$TMPDIR`, which a GUI app and a terminal (ssh, tmux, sudo) can see
/// differently. Its own folder, not Tauri's app data folder, because the server
/// insists on `0700`. `None` without an absolute `HOME`.
pub fn socket_path(home: Option<OsString>) -> Option<PathBuf> {
    let home = PathBuf::from(home?);
    home.is_absolute().then(|| {
        home.join("Library/Application Support/Bouncer")
            .join("bouncer.sock")
    })
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn socket_lives_in_a_fixed_folder_under_home() {
        assert_eq!(
            socket_path(Some("/Users/dev".into())),
            Some("/Users/dev/Library/Application Support/Bouncer/bouncer.sock".into())
        );
        assert_eq!(socket_path(None), None);
        assert_eq!(socket_path(Some("".into())), None);
        assert_eq!(socket_path(Some("relative/home".into())), None);
    }
}
