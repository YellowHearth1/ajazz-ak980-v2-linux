//! Settings and profiles (`~/.config/ak980/config.json`, the same file as the Python version).
//!
//! A profile is everything the keyboard can hold plus the PC-side lighting: lighting source and
//! effect, per-key colours, both key layers, macros and keyboard options. Global settings (clock,
//! language, auto-switching rules, launcher keys, the app's look) live next to the profiles.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize, Serializer};
use serde_json::{Map, Value};

use crate::error::{Error, Result};
use crate::i18n::tr;
use crate::keymap::{ActionKind, KeyAction};
use crate::lighting::{KeyboardOptions, LightSettings};
use crate::macros::Macro;
use crate::{paths, tr};

pub const VERSION: u32 = 2;

/// Where the keyboard's light comes from.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase", from = "String")]
pub enum LightSource {
    /// A firmware effect, stored in the keyboard.
    #[default]
    Firmware,
    /// Per-key colours, stored in the keyboard.
    Custom,
    /// An effect computed on the PC and streamed while the agent or the app runs.
    Host,
}

impl From<String> for LightSource {
    fn from(s: String) -> Self {
        match s.as_str() {
            "custom" => Self::Custom,
            "host" => Self::Host,
            _ => Self::Firmware,
        }
    }
}

pub fn default_name() -> String {
    tr("Main")
}

fn serialize_layer<S: Serializer>(layer: &BTreeMap<u8, KeyAction>, s: S) -> Result<S::Ok, S::Error> {
    let kept: BTreeMap<u8, &KeyAction> = layer.iter().filter(|(_, a)| !a.is_default()).map(|(k, v)| (*k, v)).collect();
    kept.serialize(s)
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Profile {
    pub name: String,
    pub light_source: LightSource,
    pub lighting: LightSettings,
    pub custom_colors: BTreeMap<u8, [u8; 3]>,
    pub custom_brightness: u8,
    pub host_effect: String,
    /// Settings of every PC effect the profile has used: effect id → setting id → value.
    pub host_values: BTreeMap<String, Map<String, Value>>,
    pub host_brightness: u8,
    #[serde(serialize_with = "serialize_layer")]
    pub keymap_top: BTreeMap<u8, KeyAction>,
    #[serde(serialize_with = "serialize_layer")]
    pub keymap_fn: BTreeMap<u8, KeyAction>,
    pub macros: Vec<Macro>,
    pub options: KeyboardOptions,
}

impl Default for Profile {
    fn default() -> Self {
        Self {
            name: default_name(),
            light_source: LightSource::Firmware,
            lighting: LightSettings::default(),
            custom_colors: BTreeMap::new(),
            custom_brightness: 5,
            host_effect: "wave".into(),
            host_values: BTreeMap::new(),
            host_brightness: 100,
            keymap_top: BTreeMap::new(),
            keymap_fn: BTreeMap::new(),
            macros: Vec::new(),
            options: KeyboardOptions::default(),
        }
    }
}

impl Profile {
    pub fn named(name: impl Into<String>) -> Self {
        Self { name: name.into(), ..Self::default() }
    }

    pub fn macro_names(&self) -> Vec<String> {
        self.macros.iter().map(|m| m.name.clone()).collect()
    }

    pub fn layer(&self, fn_layer: bool) -> &BTreeMap<u8, KeyAction> {
        if fn_layer { &self.keymap_fn } else { &self.keymap_top }
    }

    pub fn layer_mut(&mut self, fn_layer: bool) -> &mut BTreeMap<u8, KeyAction> {
        if fn_layer { &mut self.keymap_fn } else { &mut self.keymap_top }
    }

    /// Removes macro `i`. Keys refer to macros by slot, so the keys that played it go back to their
    /// default and the keys of the macros after it follow them one slot up. Returns the macro and
    /// how many keys were reset.
    pub fn remove_macro(&mut self, i: usize) -> Option<(Macro, usize)> {
        if i >= self.macros.len() {
            return None;
        }
        let removed = self.macros.remove(i);
        let mut reset = 0;
        for layer in [&mut self.keymap_top, &mut self.keymap_fn] {
            layer.retain(|_, a| {
                let gone = a.kind == ActionKind::Macro && usize::from(a.macro_slot) == i;
                reset += usize::from(gone);
                !gone
            });
            for a in layer.values_mut() {
                if a.kind == ActionKind::Macro && usize::from(a.macro_slot) > i {
                    a.macro_slot -= 1;
                }
            }
        }
        Some((removed, reset))
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rule {
    /// Process name as in /proc/<pid>/comm or the executable's file name.
    pub process: String,
    pub profile: String,
}

/// A launcher command that switches to the next profile, in the order of the list.
pub const PROFILE_NEXT: &str = "profile:next";
/// The start of a launcher command that switches to the profile named after it.
pub const PROFILE_USE: &str = "profile:use:";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Launcher {
    /// F13..F24
    pub key: String,
    /// Shell command or URL.
    pub command: String,
    #[serde(default)]
    pub label: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub language: String,
    pub clock_auto: bool,
    pub clock_slot: u8,
    pub sync_interval_min: u32,
    pub lcd_slot: u8,
    pub last_image: String,
    pub auto_switch: bool,
    pub default_profile: String,
    pub rules: Vec<Rule>,
    pub launchers: Vec<Launcher>,
    pub host_fps: u32,
    pub animations: bool,
    pub close_to_tray: bool,
    /// "auto" (follow the system), "graphite", "lavender" or a Catppuccin flavour ("latte",
    /// "frappe", "macchiato", "mocha"); the window only.
    pub theme: String,
    /// The Catppuccin accent ("mauve", "blue", …); Graphite and Lavender keep their own.
    pub accent: String,
    /// How the keyboard is drawn: "auto" (the colourway that suits the theme), "graphite" or
    /// "lavender".
    pub colorway: String,
    /// Desktop notifications from the agent (battery, lost 2.4 GHz link).
    pub notifications: bool,
    /// Over any PC effect: Caps Lock and Num Lock light up while they are on.
    pub overlay_locks: bool,
    pub overlay_lock_color: [u8; 3],
    /// Over any PC effect: the keyboard flashes when a desktop notification comes.
    pub overlay_notifications: bool,
    pub overlay_notify_color: [u8; 3],
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            language: "auto".into(),
            clock_auto: true,
            clock_slot: 1,
            sync_interval_min: 60,
            lcd_slot: 1,
            last_image: String::new(),
            auto_switch: false,
            default_profile: String::new(),
            rules: Vec::new(),
            launchers: Vec::new(),
            host_fps: 30,
            animations: true,
            close_to_tray: false,
            theme: "auto".into(),
            accent: "mauve".into(),
            colorway: "auto".into(),
            notifications: true,
            overlay_locks: false,
            overlay_lock_color: [255, 255, 255],
            overlay_notifications: false,
            overlay_notify_color: [80, 160, 255],
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Config {
    pub settings: Settings,
    pub profiles: Vec<Profile>,
    pub active: String,
    pub path: PathBuf,
}

#[derive(Serialize)]
struct ConfigFile<'a> {
    version: u32,
    active: &'a str,
    settings: &'a Settings,
    profiles: &'a [Profile],
}

#[derive(Deserialize)]
struct ConfigIn {
    #[serde(default)]
    active: String,
    #[serde(default)]
    settings: Settings,
    profiles: Vec<Profile>,
}

impl Default for Config {
    fn default() -> Self {
        Self::empty(paths::config_path())
    }
}

impl Config {
    fn empty(path: PathBuf) -> Self {
        let p = Profile::default();
        Self { settings: Settings::default(), active: p.name.clone(), profiles: vec![p], path }
    }

    // ------------------------------------------------------------ profiles

    /// The active profile (the first one when the active name is unknown).
    pub fn profile(&self) -> &Profile {
        self.profiles.iter().find(|p| p.name == self.active).unwrap_or(&self.profiles[0])
    }

    pub fn profile_mut(&mut self) -> &mut Profile {
        let i = self.profiles.iter().position(|p| p.name == self.active).unwrap_or(0);
        self.active = self.profiles[i].name.clone();
        &mut self.profiles[i]
    }

    pub fn get(&self, name: &str) -> Option<&Profile> {
        self.profiles.iter().find(|p| p.name == name)
    }

    pub fn get_mut(&mut self, name: &str) -> Option<&mut Profile> {
        self.profiles.iter_mut().find(|p| p.name == name)
    }

    pub fn names(&self) -> Vec<String> {
        self.profiles.iter().map(|p| p.name.clone()).collect()
    }

    pub fn unique_name(&self, base: &str) -> String {
        if self.get(base).is_none() {
            return base.to_string();
        }
        (2..).map(|n| format!("{base} {n}")).find(|n| self.get(n).is_none()).unwrap_or_default()
    }

    /// Adds a profile under a free name and returns that name.
    pub fn add(&mut self, mut profile: Profile) -> String {
        let base = if profile.name.trim().is_empty() { tr("Profile") } else { profile.name.trim().to_string() };
        profile.name = self.unique_name(&base);
        let name = profile.name.clone();
        self.profiles.push(profile);
        name
    }

    pub fn remove(&mut self, name: &str) -> Result<()> {
        if self.profiles.len() <= 1 {
            return Err(Error::invalid(tr("The last profile cannot be deleted")));
        }
        self.profiles.retain(|p| p.name != name);
        if self.active == name {
            self.active = self.profiles[0].name.clone();
        }
        self.settings.rules.retain(|r| r.profile != name);
        if self.settings.default_profile == name {
            self.settings.default_profile.clear();
        }
        Ok(())
    }

    pub fn rename(&mut self, old: &str, new: &str) -> Result<String> {
        let new = new.trim().to_string();
        if self.get(old).is_none() {
            return Err(Error::invalid(tr!("No profile named “{name}”", name = old)));
        }
        if new.is_empty() {
            return Err(Error::invalid(tr("A profile needs a name")));
        }
        if new != old && self.get(&new).is_some() {
            return Err(Error::invalid(tr!("A profile named “{name}” already exists", name = new)));
        }
        if let Some(p) = self.get_mut(old) {
            p.name = new.clone();
        }
        if self.active == old {
            self.active = new.clone();
        }
        for r in &mut self.settings.rules {
            if r.profile == old {
                r.profile = new.clone();
            }
        }
        if self.settings.default_profile == old {
            self.settings.default_profile = new.clone();
        }
        Ok(new)
    }

    // ------------------------------------------------------------ files

    pub fn load() -> Self {
        Self::load_from(&paths::config_path())
    }

    pub fn load_from(path: &Path) -> Self {
        let Ok(text) = fs::read_to_string(path) else { return Self::empty(path.to_path_buf()) };
        let value: Value = match serde_json::from_str(&text) {
            Ok(v) => v,
            Err(_) => return Self::set_aside(path),
        };
        let Value::Object(obj) = &value else { return Self::empty(path.to_path_buf()) };
        if !obj.contains_key("profiles") {
            return Self::migrate_v1(obj, path);
        }
        match serde_json::from_value::<ConfigIn>(value) {
            Ok(c) => {
                let mut cfg = Self { settings: c.settings, profiles: c.profiles, active: c.active,
                                     path: path.to_path_buf() };
                if cfg.profiles.is_empty() {
                    cfg.profiles.push(Profile::default());
                }
                if cfg.active.is_empty() || cfg.get(&cfg.active).is_none() {
                    cfg.active = cfg.profiles[0].name.clone();
                }
                cfg.settings.rules.retain(|r| !r.process.is_empty() && !r.profile.is_empty());
                cfg.settings.launchers.retain(|l| !l.key.is_empty() && !l.command.is_empty());
                cfg
            }
            Err(e) => {
                log::warn!("{}: {e}", path.display());
                Self::set_aside(path)
            }
        }
    }

    /// Unreadable file: keep it next to the new one rather than overwrite it later.
    fn set_aside(path: &Path) -> Self {
        let secs = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        let mut broken = path.as_os_str().to_owned();
        broken.push(format!(".broken-{secs}"));
        let _ = fs::rename(path, PathBuf::from(broken));
        Self::empty(path.to_path_buf())
    }

    /// Version 1 kept one flat profile next to the settings.
    fn migrate_v1(obj: &Map<String, Value>, path: &Path) -> Self {
        let mut d = obj.clone();
        d.insert("name".into(), Value::String(default_name()));
        let mut p: Profile = serde_json::from_value(Value::Object(d)).unwrap_or_default();
        let has = |k: &str| obj.get(k).is_some_and(|v| v.as_object().is_some_and(|m| !m.is_empty()));
        if has("custom_colors") && !has("lighting") {
            p.light_source = LightSource::Custom;
        }
        let mut s = Map::new();
        for k in ["clock_slot", "lcd_slot", "sync_interval_min", "last_image"] {
            if let Some(v) = obj.get(k) {
                s.insert(k.into(), v.clone());
            }
        }
        let cfg = Self {
            settings: serde_json::from_value(Value::Object(s)).unwrap_or_default(),
            active: p.name.clone(),
            profiles: vec![p],
            path: path.to_path_buf(),
        };
        let _ = cfg.save(); // migrate once, so every process sees the same profile names
        cfg
    }

    pub fn to_json(&self) -> String {
        let file = ConfigFile { version: VERSION, active: &self.active, settings: &self.settings,
                                profiles: &self.profiles };
        serde_json::to_string_pretty(&file).expect("config serializes")
    }

    /// Writes atomically: a temporary file renamed over the old one. The window, the agent and the
    /// CLI save the same file, so each process writes its own temporary file; a shared one could be
    /// renamed into place by another process while it is half-written.
    pub fn save(&self) -> Result<()> {
        if let Some(dir) = self.path.parent() {
            fs::create_dir_all(dir)?;
        }
        let mut tmp = self.path.as_os_str().to_owned();
        tmp.push(format!(".{}.tmp", std::process::id()));
        let tmp = PathBuf::from(tmp);
        fs::write(&tmp, self.to_json() + "\n")?;
        fs::rename(&tmp, &self.path)?;
        Ok(())
    }
}

/// The language setting, read before anything else so new profiles get translated names.
pub fn peek_language() -> String {
    fs::read_to_string(paths::config_path())
        .ok()
        .and_then(|t| serde_json::from_str::<Value>(&t).ok())
        .and_then(|v| v.get("settings")?.get("language")?.as_str().map(str::to_string))
        .unwrap_or_else(|| "auto".into())
}

/// Modification time of the config file, to notice changes made by another process.
pub fn mtime(path: &Path) -> Option<SystemTime> {
    fs::metadata(path).and_then(|m| m.modified()).ok()
}

/// Settings for the system service (`/etc/ak980/agent.conf`, simple `key = value` lines).
pub fn load_system() -> Settings {
    let mut s = Settings::default();
    let Ok(text) = fs::read_to_string(paths::SYSTEM_CONFIG) else { return s };
    for line in text.lines() {
        let line = line.split('#').next().unwrap_or("").trim();
        let Some((key, value)) = line.split_once('=') else { continue };
        let (key, value) = (key.trim(), value.trim());
        match (key, value.parse::<u32>()) {
            ("clock_slot", Ok(v)) => s.clock_slot = v.min(255) as u8,
            ("sync_interval_min", Ok(v)) => s.sync_interval_min = v,
            _ => {}
        }
    }
    s
}

pub fn export_profile(p: &Profile, path: &Path) -> Result<()> {
    let mut v = serde_json::to_value(p).map_err(|e| Error::invalid(e.to_string()))?;
    if let Value::Object(m) = &mut v {
        let mut out = Map::new();
        out.insert("ak980_profile".into(), Value::from(1));
        out.extend(std::mem::take(m));
        v = Value::Object(out);
    }
    fs::write(path, serde_json::to_string_pretty(&v).unwrap_or_default() + "\n")?;
    Ok(())
}

pub fn import_profile(path: &Path) -> Result<Profile> {
    let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    let not_profile = || Error::invalid(tr!("{path} is not an exported AK980 profile", path = name));
    let v: Value = serde_json::from_str(&fs::read_to_string(path)?).map_err(|_| not_profile())?;
    if v.get("ak980_profile").is_none() {
        return Err(not_profile());
    }
    serde_json::from_value(v).map_err(|_| not_profile())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ak980-test-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir.join("config.json")
    }

    /// A file written by the Python version 0.0.1.
    const PYTHON_CONFIG: &str = r#"{
  "version": 2,
  "active": "Основний",
  "settings": {"language": "uk", "clock_auto": true, "clock_slot": 1, "sync_interval_min": 60,
    "lcd_slot": 1, "last_image": "/tmp/x.gif", "auto_switch": true, "default_profile": "",
    "rules": [{"process": "steam", "profile": "Ігри"}, {"process": "", "profile": "x"}],
    "launchers": [{"key": "F13", "command": "firefox", "label": ""}], "host_fps": 30,
    "animations": true, "close_to_tray": false},
  "profiles": [
    {"name": "Основний", "light_source": "firmware",
     "lighting": {"mode": 2, "r": 255, "g": 210, "b": 0, "rainbow": false, "brightness": 5, "speed": 3, "direction": 0},
     "custom_colors": {"1": [255, 0, 0]}, "custom_brightness": 5, "host_effect": "wave",
     "host_values": {"drift": {"a": [255, 143, 31], "speed": 25, "direction": "diagonal"}},
     "host_brightness": 100,
     "keymap_top": {"119": {"kind": "key", "mods": 0, "code": 78, "mouse": "left", "usage": 0, "macro": 0,
                            "macro_mode": "once", "macro_count": 1, "keys": []}},
     "keymap_fn": {},
     "macros": [{"name": "hi", "steps": [["down", 11, 10], ["up", 11, 10]]}],
     "options": {"disable_alt_tab": false, "disable_alt_f4": false, "disable_win": true, "fn_switch": false,
                 "sleep_time": 0, "key_response": 2}},
    {"name": "Ігри", "light_source": "nonsense"}
  ]
}"#;

    #[test]
    fn reads_the_python_file_and_round_trips() {
        let path = temp("python");
        fs::write(&path, PYTHON_CONFIG).unwrap();
        let cfg = Config::load_from(&path);
        assert_eq!(cfg.active, "Основний");
        assert_eq!(cfg.profiles.len(), 2);
        let p = cfg.profile();
        assert_eq!(p.lighting.rgb(), [255, 210, 0]);
        assert_eq!(p.custom_colors[&1], [255, 0, 0]);
        assert_eq!(p.keymap_top[&119].code, 78);
        assert_eq!(p.macros[0].steps.len(), 2);
        assert!(p.options.disable_win && p.options.sleep_time == 0);
        assert_eq!(p.host_values["drift"]["speed"], 25);
        assert_eq!(cfg.profiles[1].light_source, LightSource::Firmware);
        assert_eq!(cfg.settings.rules.len(), 1, "a rule without a process is dropped");
        assert_eq!(cfg.settings.theme, "auto");
        cfg.save().unwrap();
        let again = Config::load_from(&path);
        assert_eq!(again, cfg);
    }

    #[test]
    fn default_entries_are_not_written() {
        let mut p = Profile::named("x");
        p.keymap_top.insert(5, KeyAction::default());
        p.keymap_top.insert(6, KeyAction::off());
        let v = serde_json::to_value(&p).unwrap();
        assert_eq!(v["keymap_top"].as_object().unwrap().keys().collect::<Vec<_>>(), ["6"]);
    }

    #[test]
    fn broken_file_is_set_aside() {
        let path = temp("broken");
        fs::write(&path, "{ not json").unwrap();
        let cfg = Config::load_from(&path);
        assert_eq!(cfg.profiles.len(), 1);
        let dir = path.parent().unwrap();
        assert!(fs::read_dir(dir).unwrap().any(|e| e.unwrap().file_name().to_string_lossy().contains(".broken-")));
    }

    #[test]
    fn removing_a_macro_keeps_keys_on_the_macros_they_play() {
        let mut p = Profile::default();
        for name in ["A", "B", "C"] {
            p.macros.push(Macro::parse("text:x", Some(name)).unwrap());
        }
        let play = |slot| KeyAction::macro_(slot, crate::keymap::MacroMode::Once, 1);
        p.keymap_top.insert(10, play(0));
        p.keymap_top.insert(11, play(1));
        p.keymap_fn.insert(12, play(2));
        p.keymap_fn.insert(13, KeyAction::off());
        let (removed, reset) = p.remove_macro(1).unwrap();
        assert_eq!((removed.name.as_str(), reset), ("B", 1));
        assert_eq!(p.macro_names(), ["A", "C"]);
        assert_eq!(p.keymap_top.get(&10), Some(&play(0)));
        assert_eq!(p.keymap_top.get(&11), None, "the key that played B does its own thing again");
        assert_eq!(p.keymap_fn.get(&12), Some(&play(1)), "C moved up, and so did its key");
        assert_eq!(p.keymap_fn.get(&13), Some(&KeyAction::off()));
        assert!(p.remove_macro(5).is_none());
    }

    #[test]
    fn profile_management() {
        let mut cfg = Config::empty(temp("profiles"));
        let first = cfg.active.clone();
        let name = cfg.add(Profile::named(first.clone()));
        assert_eq!(name, format!("{first} 2"));
        cfg.settings.rules.push(Rule { process: "game".into(), profile: name.clone() });
        cfg.rename(&name, "Games").unwrap();
        assert_eq!(cfg.settings.rules[0].profile, "Games");
        assert!(cfg.rename("Games", &first).is_err());
        cfg.remove("Games").unwrap();
        assert!(cfg.settings.rules.is_empty());
        assert!(cfg.remove(&first).is_err(), "the last profile stays");
    }

    #[test]
    fn export_and_import() {
        let path = temp("export").with_file_name("p.json");
        let mut p = Profile::named("Ігри");
        p.lighting.mode = 4;
        export_profile(&p, &path).unwrap();
        let back = import_profile(&path).unwrap();
        assert_eq!(back, p);
        fs::write(&path, "{}").unwrap();
        assert!(import_profile(&path).is_err());
    }

    #[test]
    fn migrates_version_1() {
        let path = temp("v1");
        fs::write(&path, r#"{"clock_slot": 2, "custom_colors": {"3": [1, 2, 3]}}"#).unwrap();
        let cfg = Config::load_from(&path);
        assert_eq!(cfg.settings.clock_slot, 2);
        assert_eq!(cfg.profile().light_source, LightSource::Custom);
        assert!(fs::read_to_string(&path).unwrap().contains("\"profiles\""));
    }
}
