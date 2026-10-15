//! The updater (PLAN, "Updater"). Release builds only: once a day, while
//! Settings' "Check for updates" is on, it reads the latest published
//! release's `latest.json` and remembers a newer version. Nothing installs
//! until the Update button: then the installer is downloaded, its signature
//! checked, and Tauri runs it (passive, `/UPDATE`: hooks, rules, settings and
//! history stay) and quits Bouncer. Never while a card waits or a chat or
//! explanation runs. Any failure: nothing happens, no message.

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use bouncer_core::update::newer;
use serde_json::{Value, json};
use tauri::{AppHandle, Manager};
use tauri_plugin_updater::{Update, UpdaterExt};

use crate::{Island, chat, show};

/// The first check, a minute after start (not during launch).
const FIRST: Duration = Duration::from_secs(60);
/// How often the thread looks whether a check is due.
const WAKE: Duration = Duration::from_secs(60 * 60);
/// A check is due a day after the last one.
const EVERY: Duration = Duration::from_secs(24 * 60 * 60);
/// Each request gives up after this.
const TIMEOUT: Duration = Duration::from_secs(30);

/// The newer release found, if any, and whether it is being installed.
#[derive(Default)]
pub struct Updates {
    found: Mutex<Option<Update>>,
    installing: AtomicBool,
}

impl Updates {
    /// What the island shows: the version found and whether it's installing.
    pub fn view(&self) -> Value {
        match self.found.lock().unwrap().as_ref() {
            Some(update) => json!({
                "version": update.version,
                "installing": self.installing.load(Ordering::SeqCst),
            }),
            None => Value::Null,
        }
    }

    /// The switch went off: a found release is no longer offered.
    pub fn forget(&self) {
        self.found.lock().unwrap().take();
    }
}

/// Starts the daily check. Bouncer-dev (a debug build) never checks.
pub fn start(app: AppHandle) {
    if cfg!(debug_assertions) {
        return;
    }
    std::thread::spawn(move || {
        std::thread::sleep(FIRST);
        let mut last: Option<Instant> = None;
        loop {
            if checking(&app) && last.is_none_or(|t| t.elapsed() >= EVERY) {
                last = Some(Instant::now());
                check(&app);
            }
            std::thread::sleep(WAKE);
        }
    });
}

fn checking(app: &AppHandle) -> bool {
    app.state::<Island>().prefs.lock().unwrap().updates
}

/// One check. Only a plain X.Y.Z newer than ours counts; anything failing
/// (offline, GitHub down, bad JSON) leaves things as they were.
fn check(app: &AppHandle) {
    let Ok(updater) = app
        .updater_builder()
        .timeout(TIMEOUT)
        .version_comparator(|current, release| {
            newer(&current.to_string(), &release.version.to_string())
        })
        .build()
    else {
        return;
    };
    let Ok(Some(update)) = tauri::async_runtime::block_on(updater.check()) else {
        return;
    };
    // The switch may have gone off while it ran.
    if !checking(app) {
        return;
    }
    *app.state::<Updates>().found.lock().unwrap() = Some(update);
    show(app, app.state::<Island>().desk.view());
}

/// Something the user is in the middle of: a card or question waiting, or a
/// chat or explanation running. The update waits for it.
fn busy(app: &AppHandle) -> bool {
    app.state::<Island>().desk.has_cards() || app.state::<chat::Chats>().running()
}

/// The Update button: downloads the release found (its signature is checked
/// against our public key), then runs its installer, which quits Bouncer.
/// Refused in a dev build, with nothing found, while busy (before and after
/// the download) or while already installing. Errors are short codes.
#[tauri::command]
pub async fn update_install(app: AppHandle) -> Result<(), String> {
    if cfg!(debug_assertions) {
        return Err("dev".into());
    }
    let updates = app.state::<Updates>();
    let update = updates.found.lock().unwrap().clone().ok_or("none")?;
    if busy(&app) {
        return Err("busy".into());
    }
    if updates.installing.swap(true, Ordering::SeqCst) {
        return Err("installing".into());
    }
    show(&app, app.state::<Island>().desk.view());
    let result = async {
        let bytes = update
            .download(|_, _| {}, || {})
            .await
            .map_err(|_| "download")?;
        if busy(&app) {
            return Err("busy");
        }
        app.state::<chat::Chats>().end_all();
        // On Windows this starts the installer and exits Bouncer.
        update.install(bytes).map_err(|_| "install")
    }
    .await;
    updates.installing.store(false, Ordering::SeqCst);
    show(&app, app.state::<Island>().desk.view());
    result.map_err(str::to_owned)
}

#[cfg(test)]
mod tests {
    /// The updater reads only the latest published release of our repo over
    /// HTTPS (no version or machine variables in the URL), checks our
    /// public key, installs passively; normal builds never sign (only the
    /// release workflow adds `createUpdaterArtifacts`).
    #[test]
    fn the_updater_reads_one_fixed_https_file_and_checks_our_key() {
        let conf: serde_json::Value =
            serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        let updater = &conf["plugins"]["updater"];
        assert_eq!(
            updater["endpoints"],
            serde_json::json!([
                "https://github.com/charanp11/bouncer-app/releases/latest/download/latest.json"
            ])
        );
        assert_eq!(updater["windows"]["installMode"], "passive");
        assert!(updater.get("dangerousInsecureTransportProtocol").is_none());
        let key = updater["pubkey"].as_str().unwrap();
        // A minisign public key, base64 of its two text lines.
        assert!(key.starts_with("dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6"));
        let release: serde_json::Value =
            serde_json::from_str(include_str!("../tauri.release.json")).unwrap();
        for c in [&conf, &release] {
            assert!(c["bundle"].get("createUpdaterArtifacts").is_none());
        }
    }

    /// Bouncer-dev never checks or installs; the install waits while the
    /// user is busy (before and after the download); nothing but the
    /// Update button installs.
    #[test]
    fn dev_builds_never_update_and_a_busy_island_waits() {
        let all = include_str!("update.rs");
        let src = &all[..all.find("#[cfg(test)]").unwrap()];
        let body = |name: &str| {
            let start = src.find(name).unwrap();
            &src[start..start + src[start..].find("\n}\n").unwrap()]
        };
        for f in ["pub fn start(", "pub async fn update_install("] {
            let b = body(f);
            let dev = b.find("if cfg!(debug_assertions)").unwrap();
            assert!(
                dev < b.find("app.state").unwrap_or(usize::MAX),
                "{f}: dev first"
            );
        }
        let install = body("pub async fn update_install(");
        assert_eq!(install.matches("if busy(&app)").count(), 2);
        let download = install.find(".download(").unwrap();
        assert!(install.find("if busy(&app)").unwrap() < download);
        assert!(install.rfind("if busy(&app)").unwrap() > download);
        assert!(install.rfind("if busy(&app)").unwrap() < install.find(".install(").unwrap());
        // `install` appears only in the command.
        assert_eq!(src.matches(".install(bytes)").count(), 1);
        let main = include_str!("main.rs");
        assert!(
            !main.contains(".download_and_install(") && !src.contains(".download_and_install(")
        );
    }
}
