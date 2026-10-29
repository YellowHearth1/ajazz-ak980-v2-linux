//! Where things live on disk (XDG base directories).

use std::env;
use std::path::PathBuf;

fn home() -> PathBuf {
    env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("/"))
}

fn xdg(var: &str, fallback: &str) -> PathBuf {
    match env::var_os(var) {
        Some(v) if !v.is_empty() => PathBuf::from(v),
        _ => home().join(fallback),
    }
}

pub fn config_home() -> PathBuf {
    xdg("XDG_CONFIG_HOME", ".config")
}

/// `~/.config/ak980`
pub fn config_dir() -> PathBuf {
    config_home().join("ak980")
}

/// `~/.config/ak980/config.json`
pub fn config_path() -> PathBuf {
    config_dir().join("config.json")
}

/// `~/.local/share/ak980`
pub fn data_dir() -> PathBuf {
    xdg("XDG_DATA_HOME", ".local/share").join("ak980")
}

/// Every animation sent to the screen is kept here: the keyboard cannot give one back.
pub fn history_dir() -> PathBuf {
    data_dir().join("history")
}

/// `~/.local/state`
pub fn state_home() -> PathBuf {
    xdg("XDG_STATE_HOME", ".local/state")
}

/// `~/.local/state/ak980`
pub fn state_dir() -> PathBuf {
    state_home().join("ak980")
}

/// The 2.4 GHz link record written by the agent.
pub fn link_log() -> PathBuf {
    state_dir().join("link.log")
}

pub const SYSTEM_CONFIG: &str = "/etc/ak980/agent.conf";
