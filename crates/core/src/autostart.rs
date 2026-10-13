//! "Start with Windows": one value, `Bouncer`, in the current user's `Run` key
//! (no admin). Off by default; the value itself is the setting, so it can't
//! drift from what Windows does. Turning it off deletes only our value: one
//! whose program has this app's file name. Other systems: not offered yet.

// Only Windows writes anything yet; elsewhere the helpers are only tested.
#![cfg_attr(not(windows), allow(dead_code))]

/// Debug builds may use another key under HKCU (tests, the test instance),
/// so the real `Run` key is never touched while developing.
pub const KEY_ENV: &str = "BOUNCER_AUTOSTART_KEY";

use std::path::Path;

const RUN: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const NAME: &str = "Bouncer";

fn key_with(over: Option<std::ffi::OsString>) -> String {
    over.filter(|v| cfg!(debug_assertions) && !v.is_empty())
        .and_then(|v| v.into_string().ok())
        .unwrap_or_else(|| RUN.into())
}

fn key() -> String {
    key_with(std::env::var_os(KEY_ENV))
}

/// What Windows runs: the program's path, quoted.
fn command(exe: &Path) -> String {
    format!("\"{}\"", exe.display())
}

/// Whether the value is there and runs `exe`. `None`: not offered here.
pub fn enabled(exe: &Path) -> Option<bool> {
    #[cfg(windows)]
    return Some(enabled_in(&key(), exe));
    #[cfg(not(windows))]
    {
        let _ = exe;
        None
    }
}

/// Turns it on (pointing at `exe`) or off (only if the value is ours).
pub fn set(on: bool, exe: &Path) -> Result<(), String> {
    #[cfg(windows)]
    return set_in(&key(), on, exe);
    #[cfg(not(windows))]
    {
        let _ = (on, exe);
        Err("starting with the system isn't offered here yet".into())
    }
}

#[cfg(windows)]
fn enabled_in(key: &str, exe: &Path) -> bool {
    win::get(key, NAME).is_some_and(|v| v == command(exe))
}

#[cfg(windows)]
fn set_in(key: &str, on: bool, exe: &Path) -> Result<(), String> {
    if on {
        return win::set(key, NAME, &command(exe));
    }
    let ours = exe.file_name().and_then(|n| n.to_str()).unwrap_or_default();
    match win::get(key, NAME) {
        Some(v) if is_ours(&v, ours) => win::delete(key, NAME),
        _ => Ok(()),
    }
}

/// A `Run` command whose program file is named `name` (any folder).
fn is_ours(command: &str, name: &str) -> bool {
    let program = command.trim().trim_matches('"');
    let file = program.rsplit(['/', '\\']).next().unwrap_or(program);
    !name.is_empty() && file.eq_ignore_ascii_case(name)
}

#[cfg(windows)]
mod win {
    use windows_sys::Win32::Foundation::ERROR_SUCCESS;
    use windows_sys::Win32::System::Registry::{
        HKEY_CURRENT_USER, REG_SZ, RRF_RT_REG_SZ, RegDeleteKeyValueW, RegGetValueW, RegSetKeyValueW,
    };

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain([0]).collect()
    }

    pub fn get(key: &str, name: &str) -> Option<String> {
        let (key, name) = (wide(key), wide(name));
        let mut buf = vec![0u16; 1024];
        let mut size = (buf.len() * 2) as u32;
        // SAFETY: the strings are NUL-terminated; buf holds `size` bytes.
        let status = unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                key.as_ptr(),
                name.as_ptr(),
                RRF_RT_REG_SZ,
                std::ptr::null_mut(),
                buf.as_mut_ptr().cast(),
                &mut size,
            )
        };
        if status != ERROR_SUCCESS {
            return None;
        }
        let len = (size as usize / 2).saturating_sub(1);
        String::from_utf16(&buf[..len]).ok()
    }

    pub fn set(key: &str, name: &str, value: &str) -> Result<(), String> {
        let (key, name, value) = (wide(key), wide(name), wide(value));
        // SAFETY: NUL-terminated strings; the data is `value`, its bytes counted.
        let status = unsafe {
            RegSetKeyValueW(
                HKEY_CURRENT_USER,
                key.as_ptr(),
                name.as_ptr(),
                REG_SZ,
                value.as_ptr().cast(),
                (value.len() * 2) as u32,
            )
        };
        if status == ERROR_SUCCESS {
            Ok(())
        } else {
            Err(format!("couldn't turn it on (error {status})"))
        }
    }

    pub fn delete(key: &str, name: &str) -> Result<(), String> {
        let (key, name) = (wide(key), wide(name));
        // SAFETY: NUL-terminated strings.
        let status = unsafe { RegDeleteKeyValueW(HKEY_CURRENT_USER, key.as_ptr(), name.as_ptr()) };
        if status == ERROR_SUCCESS {
            Ok(())
        } else {
            Err(format!("couldn't turn it off (error {status})"))
        }
    }

    #[cfg(test)]
    pub fn delete_key(key: &str) {
        use windows_sys::Win32::System::Registry::RegDeleteTreeW;
        let key = wide(key);
        // SAFETY: NUL-terminated string.
        unsafe { RegDeleteTreeW(HKEY_CURRENT_USER, key.as_ptr()) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn override_is_honored_only_in_debug_builds() {
        let over = key_with(Some(r"Software\BouncerTest".into()));
        assert_eq!(over == r"Software\BouncerTest", cfg!(debug_assertions));
        assert_eq!(key_with(Some("".into())), RUN);
        assert_eq!(key_with(None), RUN);
    }

    #[test]
    fn only_our_program_counts_as_ours() {
        assert!(is_ours(
            r#""C:\Users\dev\AppData\Local\Bouncer\Bouncer.exe""#,
            "bouncer.exe"
        ));
        assert!(is_ours(r"D:\old place\Bouncer.exe", "Bouncer.exe"));
        assert!(!is_ours(r#""C:\Tools\NotBouncer.exe""#, "Bouncer.exe"));
        assert!(!is_ours(r#""C:\Tools\Bouncer.exe" --flag"#, "Bouncer.exe"));
        assert!(!is_ours("anything", ""));
    }

    /// On, off, and someone else's `Bouncer` value left alone; in a test key
    /// under HKCU (deleted after), never the real `Run` key.
    #[cfg(windows)]
    #[test]
    fn on_off_and_someone_elses_value_left_alone() {
        let key = format!(r"Software\BouncerTest-{}", std::process::id());
        let exe = Path::new(r"C:\Users\dev\AppData\Local\Bouncer\Bouncer.exe");
        let is_on = || enabled_in(&key, exe);
        let run = |on| set_in(&key, on, exe);
        assert!(!is_on(), "off by default");
        run(true).unwrap();
        assert!(is_on());
        assert_eq!(win::get(&key, NAME).unwrap(), command(exe));
        run(false).unwrap();
        assert!(!is_on());
        assert_eq!(win::get(&key, NAME), None);
        run(false).unwrap(); // already off: fine

        win::set(&key, NAME, r#""C:\Tools\Other.exe""#).unwrap();
        run(false).unwrap();
        assert_eq!(
            win::get(&key, NAME).as_deref(),
            Some(r#""C:\Tools\Other.exe""#)
        );
        win::delete_key(&key);
    }
}
