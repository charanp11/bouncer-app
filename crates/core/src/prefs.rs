//! Bouncer's own preferences (island size, sound) in `preferences.json`, next
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Prefs {
    pub size: Size,
    pub sound: bool,
}

impl Default for Prefs {
    fn default() -> Prefs {
        Prefs {
            size: Size::Medium,
            sound: true,
        }
    }
}

impl Prefs {
    pub fn to_json(self) -> Value {
        json!({ "size": self.size.name(), "sound": self.sound })
    }

    /// Known fields with known values; anything else keeps the default.
    fn from_json(value: &Value) -> Prefs {
        let default = Prefs::default();
        Prefs {
            size: value["size"]
                .as_str()
                .and_then(Size::parse)
                .unwrap_or(default.size),
            sound: value["sound"].as_bool().unwrap_or(default.sound),
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
        };
        save(&path, prefs).unwrap();
        assert_eq!(load(&path), prefs);
        save(&path, Prefs::default()).unwrap();
        assert_eq!(load(&path), Prefs::default());
    }

    #[test]
    fn unknown_or_broken_values_fall_back_per_field() {
        let v = |text: &str| Prefs::from_json(&serde_json::from_str(text).unwrap());
        assert_eq!(
            v(r#"{"size":"small","sound":false}"#),
            Prefs {
                size: Size::Small,
                sound: false
            }
        );
        assert_eq!(
            v(r#"{"size":"huge","sound":false}"#),
            Prefs {
                size: Size::Medium,
                sound: false
            }
        );
        assert_eq!(v(r#"{"size":1.75,"sound":"yes"}"#), Prefs::default());
        assert_eq!(v("[]"), Prefs::default());
        let path = temp_dir("prefs-broken").join("preferences.json");
        rules::replace(&path, "{ not json").unwrap();
        assert_eq!(load(&path), Prefs::default());
    }
}
