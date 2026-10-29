//! Ajazz AK980 V2 | Software for Linux: everything except the window.
//!
//! Unofficial configuration software for the AJAZZ AK980 V2 keyboard: screen clock and GIFs,
//! lighting (firmware effects, per-key colours, real-time effects computed on the PC), key
//! remapping, macros, profiles and keyboard options. `docs/PROTOCOL.md` documents every command.

// pub mod agent;   // not ported yet
pub mod audio;
pub mod config;
pub mod device;
pub mod effects;
pub mod engine;
mod error;
pub mod hid;
pub mod i18n;
pub mod keyinput;
pub mod keymap;
pub mod layout;
pub mod lcd;
pub mod lighting;
pub mod macros;
pub mod paths;
pub mod permissions;
pub mod profiles;
pub mod runtime;
pub mod service;
pub mod time;

pub use error::{Error, Result};

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const APP_NAME: &str = "Ajazz AK980 V2";
pub const APP_TITLE: &str = "Ajazz AK980 V2 | Software for Linux";
pub const APP_ID: &str = "io.github.yellowhearth1.AjazzAK980V2";
pub const SLUG: &str = "ajazz-ak980-v2-linux";
pub const HOMEPAGE: &str = "https://github.com/yellowhearth1/ajazz-ak980-v2-linux";
pub const ISSUES: &str = "https://github.com/yellowhearth1/ajazz-ak980-v2-linux/issues";
