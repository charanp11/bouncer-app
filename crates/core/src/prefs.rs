//! Bouncer's own preferences (island size, sounds) in `preferences.json`, next
//! to `rules.toml`. Never security policy: that stays in the rules file.
//!
//! Read with the rules file's checks (ours, private, not a link, small,
//! UTF-8); a missing, refused or broken file, or an unknown value, gives the
//! default for that field. Written with the rules file's atomic, owner-only
//! replace.

use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use crate::rules;

/// How big the island is drawn (the page maps these to 100 / 125 / 150%).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Size {
    Small,
    #[default]
    Medium,
    Large,
}

impl Size {
    pub fn name(self) -> &'static str {
        match self {
            Size::Small => "small",
            Size::Medium => "medium",
            Size::Large => "large",
        }
    }

    pub fn parse(name: &str) -> Option<Size> {
        [Size::Small, Size::Medium, Size::Large]
            .into_iter()
            .find(|s| s.name() == name)
    }
}

/// How the sounds are voiced: the same sounds, gentler in Soft.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Style {
    #[default]
    Soft,
    Playful,
}

impl Style {
    pub fn name(self) -> &'static str {
        match self {
            Style::Soft => "soft",
            Style::Playful => "playful",
        }
    }

    pub fn parse(name: &str) -> Option<Style> {
        [Style::Soft, Style::Playful]
            .into_iter()
            .find(|s| s.name() == name)
    }
}

/// Every sound, by the name the page plays it by.
pub const SOUNDS: [&str; 17] = [
    "launch", "session", "needs", "risky", "allowed", "denied", "vip", "auto", "done", "fail",
    "back", "poke", "dizzy", "paused", "resumed", "autoon", "wiped",
];
/// On until turned off (Charan, 2026-10-06); the rest start off.
const ON_BY_DEFAULT: [&str; 6] = ["launch", "needs", "risky", "done", "fail", "back"];

/// The bit for `name` in [`Prefs::sounds`].
fn bit(name: &str) -> Option<u32> {
    SOUNDS.iter().position(|s| *s == name).map(|i| 1 << i)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Prefs {
    pub size: Size,
    /// All sounds on / off.
    pub sound: bool,
    pub style: Style,
    /// 0 to 100.
    pub volume: u8,
    /// Which sounds are on: one bit per [`SOUNDS`] entry.
    pub sounds: u32,
}

impl Default for Prefs {
    fn default() -> Prefs {
        Prefs {
            size: Size::Medium,
            sound: true,
            style: Style::Soft,
            volume: 50,
            sounds: ON_BY_DEFAULT.iter().filter_map(|n| bit(n)).sum(),
        }
    }
}

impl Prefs {
    pub fn to_json(self) -> Value {
        let sounds: serde_json::Map<String, Value> = SOUNDS
            .iter()
            .map(|n| ((*n).to_owned(), Value::Bool(self.on(n))))
            .collect();
        json!({
            "size": self.size.name(),
            "sound": self.sound,
            "style": self.style.name(),
            "volume": self.volume,
            "sounds": sounds,
        })
    }

    /// Whether sound `name` is on (an unknown name is off).
    pub fn on(self, name: &str) -> bool {
        bit(name).is_some_and(|b| self.sounds & b != 0)
    }

    /// The mask with the sounds named in `on` on and every other one off;
    /// `None` if a name isn't a sound.
    pub fn sounds_from(on: &[String]) -> Option<u32> {
        on.iter().try_fold(0, |mask, n| Some(mask | bit(n)?))
    }

    /// Known fields with known values; anything else keeps the default.
    fn from_json(value: &Value) -> Prefs {
        let default = Prefs::default();
        let mut sounds = default.sounds;
        for (name, on) in value["sounds"].as_object().into_iter().flatten() {
            if let (Some(b), Some(on)) = (bit(name), on.as_bool()) {
                sounds = if on { sounds | b } else { sounds & !b };
            }
        }
        Prefs {
            size: value["size"]
                .as_str()
                .and_then(Size::parse)
                .unwrap_or(default.size),
            sound: value["sound"].as_bool().unwrap_or(default.sound),
            style: value["style"]
                .as_str()
                .and_then(Style::parse)
                .unwrap_or(default.style),
            volume: value["volume"]
                .as_u64()
                .and_then(|v| u8::try_from(v).ok())
                .filter(|v| *v <= 100)
                .unwrap_or(default.volume),
            sounds,
        }
    }
}

/// `preferences.json` in the folder holding the rules file.
pub fn path() -> Option<PathBuf> {
    rules::path().map(|p| p.with_file_name("preferences.json"))
}

pub fn load(path: &Path) -> Prefs {
    if !path.exists() {
        return Prefs::default();
    }
    rules::read(path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .map_or_else(Prefs::default, |v: Value| Prefs::from_json(&v))
}

pub fn save(path: &Path, prefs: Prefs) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        rules::create_private_dir(dir)
            .map_err(|e| format!("can't create {}: {e}", dir.display()))?;
    }
    let text = serde_json::to_string_pretty(&prefs.to_json()).map_err(|e| e.to_string())? + "\n";
    rules::replace(path, &text).map_err(|e| format!("can't save {}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::tests::temp_dir;

    #[test]
    fn missing_file_gives_defaults_and_save_reads_back() {
        let path = temp_dir("prefs-roundtrip").join("preferences.json");
        assert_eq!(load(&path), Prefs::default());
        let prefs = Prefs {
            size: Size::Large,
            sound: false,
            style: Style::Playful,
            volume: 80,
            sounds: Prefs::sounds_from(&["poke".into(), "wiped".into()]).unwrap(),
        };
        save(&path, prefs).unwrap();
        assert_eq!(load(&path), prefs);
        save(&path, Prefs::default()).unwrap();
        assert_eq!(load(&path), Prefs::default());
    }

    #[test]
    fn unknown_or_broken_values_fall_back_per_field() {
        let v = |text: &str| Prefs::from_json(&serde_json::from_str(text).unwrap());
        let default = Prefs::default();
        assert_eq!(
            v(r#"{"size":"small","sound":false}"#),
            Prefs {
                size: Size::Small,
                sound: false,
                ..default
            }
        );
        assert_eq!(
            v(r#"{"size":"huge","sound":false}"#),
            Prefs {
                sound: false,
                ..default
            }
        );
        assert_eq!(v(r#"{"size":1.75,"sound":"yes"}"#), default);
        assert_eq!(v("[]"), default);
        // Style and volume: known values only, volume 0 to 100.
        let p = v(r#"{"style":"playful","volume":0}"#);
        assert_eq!((p.style, p.volume), (Style::Playful, 0));
        for bad in [
            r#"{"style":"retro"}"#,
            r#"{"volume":101}"#,
            r#"{"volume":256}"#,
            r#"{"volume":-1}"#,
            r#"{"volume":2.5}"#,
        ] {
            assert_eq!(v(bad), default, "{bad}");
        }
        // Sounds: known names set to true / false change the default; unknown
        // names and other values are ignored.
        let p = v(r#"{"sounds":{"poke":true,"needs":false,"nope":true,"done":"no","risky":1}}"#);
        assert!(p.on("poke") && !p.on("needs") && p.on("done") && p.on("risky"));
        assert!(!p.on("nope"));
        assert_eq!(v(r#"{"sounds":["poke"]}"#), default);
        let path = temp_dir("prefs-broken").join("preferences.json");
        rules::replace(&path, "{ not json").unwrap();
        assert_eq!(load(&path), default);
    }

    /// On by default: exactly launch, needs you, risky, done, tool failed and
    /// welcome back (Charan, 2026-10-06); Soft at 50%.
    #[test]
    fn six_sounds_are_on_by_default() {
        let p = Prefs::default();
        let on: Vec<&str> = SOUNDS.into_iter().filter(|n| p.on(n)).collect();
        assert_eq!(on, ["launch", "needs", "risky", "done", "fail", "back"]);
        assert_eq!((p.style, p.volume), (Style::Soft, 50));
    }

    #[test]
    fn sounds_from_names_refuses_unknown_names() {
        let p = Prefs {
            sounds: Prefs::sounds_from(&["needs".into(), "risky".into()]).unwrap(),
            ..Prefs::default()
        };
        assert!(p.on("needs") && p.on("risky") && !p.on("done"));
        assert_eq!(Prefs::sounds_from(&[]), Some(0));
        assert_eq!(Prefs::sounds_from(&["needs".into(), "retro".into()]), None);
    }

    #[test]
    fn the_file_lists_every_sound() {
        let json = Prefs::default().to_json();
        let names: Vec<&String> = json["sounds"].as_object().unwrap().keys().collect();
        assert_eq!(names, SOUNDS);
        assert_eq!(Prefs::from_json(&json), Prefs::default());
    }
}
