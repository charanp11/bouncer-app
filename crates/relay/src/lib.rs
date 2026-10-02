//! Where the relay and the app meet, and who may be on the other end.
//!
//! Shared by the `bouncer-hook` relay (client side) and `bouncer-core`'s server.
//! Windows: a named pipe whose name carries the user's SID. macOS: a Unix socket
//! in a per-user `0700` folder. Both ends check that the peer is the same user.

use std::path::PathBuf;

#[cfg(unix)]
pub mod unix;
#[cfg(windows)]
pub mod win;

#[cfg(unix)]
pub use unix::{Stream, connect};
#[cfg(windows)]
pub use win::{Stream, connect};

/// Overrides the endpoint path (tests). Peer checks still apply.
pub const ENDPOINT_ENV: &str = "BOUNCER_ENDPOINT";

/// The pipe / socket path for the current user, or `None` if the user can't be
/// identified (then nothing connects, and Claude Code asks as usual).
pub fn endpoint() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os(ENDPOINT_ENV) {
        return Some(path.into());
    }
    #[cfg(windows)]
    return Some(format!(r"\\.\pipe\bouncer-{}", win::current_user_sid()?).into());
    #[cfg(unix)]
    return Some(
        std::env::temp_dir()
            .join(format!("bouncer-{}", unix::uid()))
            .join("bouncer.sock"),
    );
}
