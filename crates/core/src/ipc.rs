//! The app's end of the relay connection.
//!
//! Wire format: the relay sends one JSON line (the hook event). For a
//! `PermissionRequest` the server may answer one line, exactly `allow` or
//! `deny`; closing without an answer means "ask in the terminal".
//!
//! Only the current user can open the endpoint (user-only DACL on Windows,
//! `0700` folder on macOS), and every connection's peer is checked again.

use std::io::{self, BufRead, BufReader, Read, Write};
use std::path::Path;
use std::sync::Arc;

use crate::event::Event;

pub use bouncer_relay::endpoint;

/// Longest event line accepted; the relay already drops bigger events.
const MAX_LINE: u64 = 1024 * 1024 + 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    Allow,
    Deny,
}

/// Called once per event, on its own thread. The return value is sent back
/// only for permission requests; `None` leaves the decision to the terminal.
pub type Handler = Arc<dyn Fn(Event) -> Option<Decision> + Send + Sync>;

/// Reads one event, asks the handler, writes the answer if there is one.
fn handle(mut stream: impl Read + Write, handler: &Handler) {
    let mut line = Vec::new();
    if BufReader::new(&mut stream)
        .take(MAX_LINE)
        .read_until(b'\n', &mut line)
        .is_err()
    {
        return;
    }
    let Some(event) = serde_json::from_slice(&line)
        .ok()
        .and_then(|v| Event::from_claude_code(&v))
    else {
        return;
    };
    let asks = event.is_permission_request();
    let answer = match handler(event) {
        Some(_) if !asks => return,
        Some(Decision::Allow) => "allow\n",
        Some(Decision::Deny) => "deny\n",
        None => return,
    };
    let _ = stream.write_all(answer.as_bytes());
    let _ = stream.flush();
}

#[cfg(unix)]
pub use unix::Server;
#[cfg(windows)]
pub use win::Server;

#[cfg(unix)]
mod unix {
    use super::*;
    use std::fs::{self, DirBuilder};
    use std::os::unix::fs::{DirBuilderExt, MetadataExt};
    use std::os::unix::net::{UnixListener, UnixStream};

    use bouncer_relay::unix::{peer_is_same_user, uid};

    pub struct Server(UnixListener);

    impl Server {
        /// Creates the private folder if needed and listens. Fails if the folder
        /// isn't a real directory owned by us and closed to others, or if
        /// another server is already answering.
        pub fn bind(path: &Path) -> io::Result<Server> {
            let dir = path.parent().ok_or(io::ErrorKind::InvalidInput)?;
            match fs::symlink_metadata(dir) {
                Err(e) if e.kind() == io::ErrorKind::NotFound => {
                    DirBuilder::new().mode(0o700).create(dir)?
                }
                Err(e) => return Err(e),
                Ok(_) => {}
            }
            let meta = fs::symlink_metadata(dir)?;
            if !meta.is_dir() || meta.uid() != uid() || meta.mode() & 0o077 != 0 {
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    format!("{} must be a 0700 folder owned by you", dir.display()),
                ));
            }
            if UnixStream::connect(path).is_ok() {
                return Err(io::ErrorKind::AddrInUse.into());
            }
            let _ = fs::remove_file(path);
            Ok(Server(UnixListener::bind(path)?))
        }

        /// Serves forever, one thread per connection.
        pub fn run(self, handler: Handler) -> io::Result<()> {
            for stream in self.0.incoming() {
                let Ok(stream) = stream else { continue };
                if peer_is_same_user(&stream) {
                    let handler = handler.clone();
                    std::thread::spawn(move || handle(stream, &handler));
                }
            }
            Ok(())
        }
    }
}

#[cfg(windows)]
mod win {
    use super::*;
    use std::fs::File;
    use std::os::windows::ffi::OsStrExt;
    use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};

    use bouncer_relay::win::{current_user_sid, is_same_user};
    use windows_sys::Win32::Foundation::{ERROR_PIPE_CONNECTED, INVALID_HANDLE_VALUE, LocalFree};
    use windows_sys::Win32::Security::Authorization::{
        ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
    };
    use windows_sys::Win32::Security::SECURITY_ATTRIBUTES;
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_FLAG_FIRST_PIPE_INSTANCE, PIPE_ACCESS_DUPLEX,
    };
    use windows_sys::Win32::System::Pipes::{
        ConnectNamedPipe, CreateNamedPipeW, GetNamedPipeClientProcessId, PIPE_READMODE_BYTE,
        PIPE_REJECT_REMOTE_CLIENTS, PIPE_TYPE_BYTE, PIPE_UNLIMITED_INSTANCES, PIPE_WAIT,
    };

    pub struct Server {
        name: Vec<u16>,
        next: OwnedHandle,
    }

    impl Server {
        /// Creates the first pipe instance. Fails if any instance of this name
        /// already exists (another Bouncer, or someone squatting the name).
        pub fn bind(path: &Path) -> io::Result<Server> {
            let name: Vec<u16> = path.as_os_str().encode_wide().chain([0]).collect();
            let next = instance(&name, true)?;
            Ok(Server { name, next })
        }

        /// Serves forever, one thread per connection.
        pub fn run(mut self, handler: Handler) -> io::Result<()> {
            loop {
                // SAFETY: `next` is a valid pipe handle; no OVERLAPPED (blocking).
                let ok =
                    unsafe { ConnectNamedPipe(self.next.as_raw_handle(), std::ptr::null_mut()) };
                let connected = ok != 0
                    || io::Error::last_os_error().raw_os_error()
                        == Some(ERROR_PIPE_CONNECTED as i32);
                let conn = std::mem::replace(&mut self.next, instance(&self.name, false)?);
                if !connected {
                    continue;
                }
                let handler = handler.clone();
                std::thread::spawn(move || {
                    let mut pid = 0;
                    // SAFETY: `conn` is a connected pipe handle; `pid` is a valid out-pointer.
                    let ok = unsafe { GetNamedPipeClientProcessId(conn.as_raw_handle(), &mut pid) };
                    if ok != 0 && is_same_user(pid) {
                        let pipe = File::from(conn);
                        handle(&pipe, &handler);
                        // Wait until the relay has read the answer before closing.
                        let _ = pipe.sync_all();
                    }
                });
            }
        }
    }

    /// One pipe instance that only the current user can open.
    fn instance(name: &[u16], first: bool) -> io::Result<OwnedHandle> {
        let sid = current_user_sid().ok_or_else(|| io::Error::other("cannot read user SID"))?;
        // Protected DACL, one entry: full access for our SID. Nobody else.
        let sddl: Vec<u16> = format!("D:P(A;;GA;;;{sid})")
            .encode_utf16()
            .chain([0])
            .collect();
        let first_flag = if first {
            FILE_FLAG_FIRST_PIPE_INSTANCE
        } else {
            0
        };
        // SAFETY: strings are NUL-terminated; the descriptor is freed after the
        // pipe has copied it; the returned handle is checked before wrapping.
        unsafe {
            let mut sd = std::ptr::null_mut();
            if ConvertStringSecurityDescriptorToSecurityDescriptorW(
                sddl.as_ptr(),
                SDDL_REVISION_1,
                &mut sd,
                std::ptr::null_mut(),
            ) == 0
            {
                return Err(io::Error::last_os_error());
            }
            let attrs = SECURITY_ATTRIBUTES {
                nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
                lpSecurityDescriptor: sd,
                bInheritHandle: 0,
            };
            let handle = CreateNamedPipeW(
                name.as_ptr(),
                PIPE_ACCESS_DUPLEX | first_flag,
                PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
                PIPE_UNLIMITED_INSTANCES,
                64 * 1024,
                64 * 1024,
                0,
                &attrs,
            );
            let err = io::Error::last_os_error();
            LocalFree(sd);
            if handle == INVALID_HANDLE_VALUE {
                return Err(err);
            }
            Ok(OwnedHandle::from_raw_handle(handle))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    /// An in-memory stream: reads `input`, collects what's written.
    struct Fake(Cursor<Vec<u8>>, Vec<u8>);
    impl Read for Fake {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            self.0.read(buf)
        }
    }
    impl Write for Fake {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.1.write(buf)
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    fn reply(input: &str, decision: Option<Decision>) -> String {
        let handler: Handler = Arc::new(move |_| decision);
        let mut fake = Fake(Cursor::new(input.as_bytes().to_vec()), Vec::new());
        handle(&mut fake, &handler);
        String::from_utf8(fake.1).unwrap()
    }

    const ASK: &str = r#"{"session_id":"s","cwd":"/p","hook_event_name":"PermissionRequest"}
"#;
    const PRE: &str = r#"{"session_id":"s","cwd":"/p","hook_event_name":"PreToolUse"}
"#;

    #[test]
    fn answers_only_permission_requests() {
        assert_eq!(reply(ASK, Some(Decision::Allow)), "allow\n");
        assert_eq!(reply(ASK, Some(Decision::Deny)), "deny\n");
        assert_eq!(reply(ASK, None), "");
        assert_eq!(reply(PRE, Some(Decision::Allow)), "");
    }

    #[test]
    fn ignores_garbage() {
        assert_eq!(reply("nope\n", Some(Decision::Allow)), "");
        assert_eq!(reply("{}\n", Some(Decision::Allow)), "");
    }
}
