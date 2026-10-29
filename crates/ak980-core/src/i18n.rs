//! Interface translations.
//!
//! Source strings are English and go through [`tr`] or the [`tr!`](crate::tr) macro. A language
//! is one JSON file in `locale/`, mapping the English string to its translation; plural forms are
//! arrays of three (1, 2–4, 5+). A missing entry falls back to English. The `i18n` test fails when
//! a literal passed to `tr!` has no Ukrainian translation or its placeholders differ.

use std::collections::HashMap;
use std::fmt::Display;
use std::sync::{OnceLock, RwLock};

pub const LANGUAGES: &[(&str, &str)] = &[("en", "English"), ("uk", "Українська")];

#[derive(Debug)]
pub enum Entry {
    One(String),
    Plural([String; 3]),
}

static UK: OnceLock<HashMap<String, Entry>> = OnceLock::new();
static LANG: RwLock<&'static str> = RwLock::new("en");

fn parse_catalog(json: &str) -> HashMap<String, Entry> {
    let raw: HashMap<String, serde_json::Value> = serde_json::from_str(json).expect("valid locale JSON");
    raw.into_iter()
        .filter_map(|(k, v)| {
            let entry = match v {
                serde_json::Value::String(s) => Entry::One(s),
                serde_json::Value::Array(a) if a.len() == 3 => {
                    let f = |i: usize| a[i].as_str().unwrap_or_default().to_string();
                    Entry::Plural([f(0), f(1), f(2)])
                }
                _ => return None,
            };
            Some((k, entry))
        })
        .collect()
}

/// The catalog of a language, `None` for English.
pub fn catalog(code: &str) -> Option<&'static HashMap<String, Entry>> {
    match code {
        "uk" => Some(UK.get_or_init(|| parse_catalog(include_str!("../locale/uk.json")))),
        _ => None,
    }
}

fn canonical(code: &str) -> Option<&'static str> {
    LANGUAGES.iter().map(|(c, _)| *c).find(|c| *c == code)
}

/// LANG and friends from locale.conf: services often start without them in the environment.
fn locale_conf() -> HashMap<String, String> {
    let mut values = HashMap::new();
    let user = crate::paths::config_home().join("locale.conf");
    for path in [std::path::PathBuf::from("/etc/locale.conf"), user] {
        let Ok(text) = std::fs::read_to_string(&path) else { continue };
        for line in text.lines() {
            if let Some((k, v)) = line.trim().split_once('=')
                && !k.is_empty() && !v.is_empty() {
                    values.insert(k.to_string(), v.trim_matches('"').to_string());
                }
        }
    }
    values
}

/// Language from the environment (LANGUAGE, LC_ALL, LC_MESSAGES, LANG) or locale.conf, else English.
pub fn detect() -> &'static str {
    let mut conf: Option<HashMap<String, String>> = None;
    for var in ["LANGUAGE", "LC_ALL", "LC_MESSAGES", "LANG"] {
        let mut value = std::env::var(var).unwrap_or_default();
        if value.is_empty() {
            value = conf.get_or_insert_with(locale_conf).get(var).cloned().unwrap_or_default();
        }
        for part in value.split(':') {
            let code = part.split('_').next().unwrap_or("").split('.').next().unwrap_or("").to_lowercase();
            if let Some(c) = canonical(&code) {
                return c;
            }
        }
    }
    "en"
}

/// Selects the interface language; "auto" or an empty string follows the environment.
pub fn set_language(code: &str) -> &'static str {
    let chosen = if code.is_empty() || code == "auto" { detect() } else { canonical(code).unwrap_or("en") };
    *LANG.write().unwrap_or_else(|e| e.into_inner()) = chosen;
    chosen
}

pub fn language() -> &'static str {
    *LANG.read().unwrap_or_else(|e| e.into_inner())
}

/// Translates `text` into the current language.
pub fn tr(text: &str) -> String {
    match catalog(language()).and_then(|c| c.get(text)) {
        Some(Entry::One(s)) => s.clone(),
        Some(Entry::Plural(forms)) => forms[0].clone(),
        None => text.to_string(),
    }
}

/// Replaces `{name}` placeholders.
pub fn format(text: &str, args: &[(&str, &dyn Display)]) -> String {
    let mut out = String::with_capacity(text.len() + 16);
    let mut rest = text;
    while let Some(start) = rest.find('{') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        match after.find('}') {
            Some(end) if args.iter().any(|(n, _)| *n == &after[..end]) => {
                let (_, value) = args.iter().find(|(n, _)| *n == &after[..end]).unwrap();
                out.push_str(&value.to_string());
                rest = &after[end + 1..];
            }
            _ => {
                out.push('{');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// Translates `text` and fills in its placeholders.
pub fn tr_args(text: &str, args: &[(&str, &dyn Display)]) -> String {
    format(&tr(text), args)
}

/// Count-dependent text, e.g. `plural(3, "{n} frame", "{n} frames")`.
///
/// A catalog stores the forms under the English singular. Ukrainian uses three forms (1 кадр,
/// 2 кадри, 5 кадрів), chosen by the usual Slavic rule.
pub fn plural(n: i64, one: &str, other: &str) -> String {
    let form = match catalog(language()).and_then(|c| c.get(one)) {
        Some(Entry::Plural(forms)) => {
            let (n10, n100) = (n.rem_euclid(10), n.rem_euclid(100));
            if n10 == 1 && n100 != 11 {
                forms[0].clone()
            } else if (2..=4).contains(&n10) && !(12..=14).contains(&n100) {
                forms[1].clone()
            } else {
                forms[2].clone()
            }
        }
        _ => if n == 1 { one.to_string() } else { other.to_string() },
    };
    format(&form, &[("n", &n)])
}

/// A number with the decimal separator of the interface language (1.5 / 1,5).
pub fn decimal(value: f64, digits: usize) -> String {
    let text = format!("{value:.digits$}");
    if language() == "uk" { text.replace('.', ",") } else { text }
}

/// `tr!("Battery: {level}", level = 80)`: translates a literal and fills in its placeholders.
#[macro_export]
macro_rules! tr {
    ($text:literal) => {
        $crate::i18n::tr($text)
    };
    ($text:literal, $($name:ident = $value:expr),+ $(,)?) => {
        $crate::i18n::tr_args($text, &[$((stringify!($name), &$value as &dyn ::std::fmt::Display)),+])
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn placeholders_and_plurals() {
        assert_eq!(format("{a} and {b} {c}", &[("a", &1), ("b", &"x")]), "1 and x {c}");
        set_language("uk");
        assert_eq!(plural(1, "{n} frame", "{n} frames"), "1 кадр");
        assert_eq!(plural(3, "{n} frame", "{n} frames"), "3 кадри");
        assert_eq!(plural(11, "{n} frame", "{n} frames"), "11 кадрів");
        assert_eq!(decimal(1.5, 1), "1,5");
        set_language("en");
        assert_eq!(plural(2, "{n} frame", "{n} frames"), "2 frames");
    }
}
