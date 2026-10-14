//! Where the relay and the app meet, and who may be on the other end.
//!
//! Shared by the `bouncer-hook` relay (client side) and `bouncer-core`'s server.
//! Windows: a named pipe whose name carries the user's SID. macOS: a Unix socket
//! in a per-user `0700` folder under the home folder. Both ends check that the peer is the same user.

use std::ffi::OsString;
use std::path::PathBuf;
use std::time::Duration;

pub mod questions;
#[cfg(unix)]
pub mod unix;
#[cfg(windows)]
pub mod win;

#[cfg(unix)]
pub use unix::{Stream, connect};
#[cfg(windows)]
pub use win::{Stream, connect};

/// The relay's whole-run budget for a permission request (under the hook's
/// 120 s timeout). The app must answer well before it.
pub const DECISION_BUDGET: Duration = Duration::from_secs(110);

/// "Bouncer" in release builds, "Bouncer-dev" in debug ones: the pipe /
/// socket, the data folder and the autostart value, so a dev build never
/// shares anything with an installed Bouncer (Charan, 2026-10-13).
pub const NAME: &str = if cfg!(debug_assertions) {
    "Bouncer-dev"
} else {
    "Bouncer"
};

/// Overrides the endpoint path in debug and test builds only; release builds
/// ignore it, so the environment can't point the relay elsewhere. Peer checks
/// still apply.
pub const ENDPOINT_ENV: &str = "BOUNCER_ENDPOINT";

/// The pipe / socket path for the current user, or `None` if the user can't be
/// identified (then nothing connects, and Claude Code asks as usual).
pub fn endpoint() -> Option<PathBuf> {
    endpoint_with(std::env::var_os(ENDPOINT_ENV))
}

fn endpoint_with(over: Option<OsString>) -> Option<PathBuf> {
    if let Some(path) = over.filter(|_| cfg!(debug_assertions)) {
        return Some(path.into());
    }
    #[cfg(windows)]
    return Some(
        format!(
            r"\\.\pipe\{}-{}",
            NAME.to_lowercase(),
            win::current_user_sid()?
        )
        .into(),
    );
    #[cfg(unix)]
    return unix::socket_path(std::env::var_os("HOME"));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn override_is_honored_only_in_debug_builds() {
        let over = endpoint_with(Some("x-test-endpoint".into()));
        assert_eq!(
            over == Some("x-test-endpoint".into()),
            cfg!(debug_assertions)
        );
        assert_eq!(over.is_some(), endpoint_with(None).is_some());
    }

    /// Dev and release builds never share a pipe or socket (nor, through
    /// `NAME`, a data folder or autostart value). CI runs this in both.
    #[test]
    fn dev_and_release_builds_have_their_own_names() {
        let want = if cfg!(debug_assertions) {
            "Bouncer-dev"
        } else {
            "Bouncer"
        };
        assert_eq!(NAME, want);
        let path = endpoint_with(None).unwrap();
        let path = path.to_string_lossy();
        #[cfg(windows)]
        assert!(
            path.starts_with(&format!(r"\\.\pipe\{}-S-", want.to_lowercase())),
            "{path}"
        );
        #[cfg(unix)]
        assert!(path.ends_with(&format!("/{want}/bouncer.sock")), "{path}");
        assert_eq!(path.contains("-dev"), cfg!(debug_assertions), "{path}");
    }
}
