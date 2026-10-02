//! Windows transport: a named pipe, with the peer process's SID checked.
//!
//! Pipe names are machine-wide, so any account could create ours first. The
//! name carries our SID, the server sets a user-only DACL, and each end checks
//! the other end's process token before trusting it. A check that can't answer
//! counts as "not ours".

use std::fs::File;
use std::os::windows::io::AsRawHandle;
use std::path::Path;
use std::time::{Duration, Instant};

use windows_sys::Win32::Foundation::{CloseHandle, ERROR_PIPE_BUSY, HANDLE, LocalFree};
use windows_sys::Win32::Security::Authorization::ConvertSidToStringSidW;
use windows_sys::Win32::Security::{GetTokenInformation, TOKEN_QUERY, TOKEN_USER, TokenUser};
use windows_sys::Win32::System::Pipes::GetNamedPipeServerProcessId;
use windows_sys::Win32::System::Threading::{
    GetCurrentProcess, OpenProcess, OpenProcessToken, PROCESS_QUERY_LIMITED_INFORMATION,
};

pub type Stream = File;

/// How long to keep retrying while every pipe instance is busy.
const BUSY_RETRY: Duration = Duration::from_millis(300);

/// Connects to the app. `None` if it isn't running or isn't ours.
pub fn connect(path: &Path) -> Option<File> {
    let deadline = Instant::now() + BUSY_RETRY;
    loop {
        match File::options().read(true).write(true).open(path) {
            Ok(pipe) => {
                let mut pid = 0;
                // SAFETY: `pipe` owns a valid pipe handle; `pid` is a valid out-pointer.
                let ok = unsafe { GetNamedPipeServerProcessId(pipe.as_raw_handle(), &mut pid) };
                return (ok != 0 && is_same_user(pid)).then_some(pipe);
            }
            Err(e) if e.raw_os_error() == Some(ERROR_PIPE_BUSY as i32) => {
                if Instant::now() >= deadline {
                    return None;
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(_) => return None,
        }
    }
}

/// True when process `pid` runs as the current user.
pub fn is_same_user(pid: u32) -> bool {
    if pid == 0 {
        return false;
    }
    let Some(mine) = current_user_sid() else {
        return false;
    };
    // SAFETY: plain Win32 call; the handle is checked and closed below.
    let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if process.is_null() {
        return false;
    }
    let theirs = token_sid(process);
    // SAFETY: we own `process`.
    unsafe { CloseHandle(process) };
    theirs.as_deref() == Some(mine.as_str())
}

/// The current user's SID as `S-1-5-21-…`.
pub fn current_user_sid() -> Option<String> {
    // SAFETY: GetCurrentProcess returns a pseudo-handle that needs no closing.
    token_sid(unsafe { GetCurrentProcess() })
}

/// The user SID of `process`'s token. Borrows `process`; doesn't close it.
fn token_sid(process: HANDLE) -> Option<String> {
    // SAFETY: every out-pointer is valid; the buffer is sized by the first
    // GetTokenInformation call and u64-aligned for TOKEN_USER; handles and the
    // LocalAlloc'd string are released on every path.
    unsafe {
        let mut token: HANDLE = std::ptr::null_mut();
        if OpenProcessToken(process, TOKEN_QUERY, &mut token) == 0 {
            return None;
        }
        let mut needed = 0u32;
        GetTokenInformation(token, TokenUser, std::ptr::null_mut(), 0, &mut needed);
        let mut buf = vec![0u64; (needed as usize).div_ceil(8)];
        let ok = needed != 0
            && GetTokenInformation(
                token,
                TokenUser,
                buf.as_mut_ptr().cast(),
                needed,
                &mut needed,
            ) != 0;
        CloseHandle(token);
        if !ok {
            return None;
        }
        let user = &*(buf.as_ptr() as *const TOKEN_USER);
        let mut text: *mut u16 = std::ptr::null_mut();
        if ConvertSidToStringSidW(user.User.Sid, &mut text) == 0 {
            return None;
        }
        let len = (0..).take_while(|&i| *text.add(i) != 0).count();
        let sid = String::from_utf16(std::slice::from_raw_parts(text, len)).ok();
        LocalFree(text.cast());
        sid
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn own_process_is_same_user() {
        assert!(current_user_sid().unwrap().starts_with("S-1-5-"));
        assert!(is_same_user(std::process::id()));
        assert!(!is_same_user(0));
    }
}
