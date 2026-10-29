//! Every interface string of the Rust code has a Ukrainian translation with the same placeholders,
//! as tests/test_i18n.py checks for the Python version: a missing entry would show up in English.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use ak980_core::effects::{EFFECTS, ParamKind};
use ak980_core::i18n::{Entry, catalog};
use ak980_core::keymap::{MacroMode, MouseFn};
use ak980_core::lighting::{DIRECTIONS, LIGHT_MODES, SLEEP_CHOICES};
use ak980_core::macros::MOUSE_BUTTONS;

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for e in fs::read_dir(dir).into_iter().flatten().flatten() {
        let p = e.path();
        if p.is_dir() {
            rust_files(&p, out);
        } else if p.extension().is_some_and(|x| x == "rs") {
            out.push(p);
        }
    }
}

/// The string literal at the start of `s` (after whitespace), unescaped; None if there is none.
fn literal(s: &str) -> Option<String> {
    let mut chars = s.trim_start().strip_prefix('"')?.chars().peekable();
    let mut out = String::new();
    while let Some(c) = chars.next() {
        match c {
            '"' => return Some(out),
            '\\' => match chars.next()? {
                'n' => out.push('\n'),
                't' => out.push('\t'),
                '\\' => out.push('\\'),
                '"' => out.push('"'),
                '\'' => out.push('\''),
                '\n' => {
                    // A line continuation drops the next line's indentation
                    while chars.peek().is_some_and(|c| c.is_whitespace()) {
                        chars.next();
                    }
                }
                _ => return None,
            },
            c => out.push(c),
        }
    }
    None
}

/// (text, is_plural) for literals given to tr!, tr, tr_args and plural (whose singular keys the forms),
/// and to the `title:`/`detail:` fields of the start-up methods.
fn translated_literals(code: &str) -> Vec<(String, bool)> {
    let mut found = Vec::new();
    for (pattern, plural) in [("tr!(", false), ("tr(", false), ("tr_args(", false), ("plural(", true),
                              ("title: ", false), ("detail: ", false)] {
        let mut rest = code;
        while let Some(i) = rest.find(pattern) {
            let before = rest[..i].chars().last();
            rest = &rest[i + pattern.len()..];
            if before.is_some_and(|c| c.is_alphanumeric() || c == '_') {
                continue; // only part of a longer name
            }
            let arg = if plural { rest.split_once(',').map(|(_, r)| r) } else { Some(rest) };
            if let Some(text) = arg.and_then(literal) {
                found.push((text, plural));
            }
        }
    }
    found
}

fn placeholders(s: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let mut rest = s;
    while let Some(i) = rest.find('{') {
        rest = &rest[i + 1..];
        if let Some(j) = rest.find('}') {
            let name = &rest[..j];
            if !name.is_empty() && name.chars().all(|c| c.is_alphanumeric() || c == '_') {
                out.insert(name.to_string());
            }
        }
    }
    out
}

#[test]
fn every_string_is_translated() {
    let uk = catalog("uk").expect("the Ukrainian catalog");
    let mut problems = BTreeSet::new();
    let mut check = |text: &str, plural: bool, place: &str| match uk.get(text) {
        None => {
            problems.insert(format!("missing: {text:?} ({place})"));
        }
        Some(Entry::One(t)) if plural => {
            problems.insert(format!("needs three plural forms: {text:?} (has {t:?})"));
        }
        Some(Entry::One(t)) => {
            if placeholders(t) != placeholders(text) {
                problems.insert(format!("placeholders differ: {text:?} → {t:?}"));
            }
        }
        Some(Entry::Plural(forms)) => {
            for f in forms {
                if placeholders(f) != placeholders(text) {
                    problems.insert(format!("placeholders differ: {text:?} → {f:?}"));
                }
            }
        }
    };

    let crates = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let mut files = Vec::new();
    rust_files(crates, &mut files);
    let mut scanned = 0;
    for file in files.iter().filter(|f| !f.components().any(|c| c.as_os_str() == "tests")) {
        let code = fs::read_to_string(file).unwrap();
        let place = file.strip_prefix(crates).unwrap().display().to_string();
        for (text, plural) in translated_literals(&code) {
            check(&text, plural, &place);
            scanned += 1;
        }
    }
    // A scanner that silently found nothing would pass; the core alone has about 60 such strings
    assert!(scanned >= 50, "only {scanned} translated strings found in the sources");
    // Tables that are translated where they are shown
    for m in &LIGHT_MODES {
        check(m.name, false, "LIGHT_MODES");
    }
    for s in DIRECTIONS.iter().chain(&SLEEP_CHOICES) {
        check(s, false, "lighting");
    }
    for e in &EFFECTS {
        check(e.name, false, e.id);
        check(e.description, false, e.id);
        for p in e.params {
            check(p.label, false, e.id);
            if let ParamKind::Choice { choices, .. } = &p.kind {
                for (_, label) in *choices {
                    check(label, false, e.id);
                }
            }
        }
    }
    for (_, label) in &MOUSE_BUTTONS {
        check(label, false, "MOUSE_BUTTONS");
    }
    for m in MouseFn::ALL {
        check(m.label(), false, "MouseFn");
    }
    for m in MacroMode::ALL {
        check(m.label(), false, "MacroMode");
    }
    assert!(problems.is_empty(), "{} problems:\n{}", problems.len(), problems.into_iter().collect::<Vec<_>>().join("\n"));
}

#[test]
fn the_scanner_finds_what_it_should() {
    let code = r#"tr!("A {x}", x = 1); i18n::tr("B"); plural(n, "{n} key", "{n} keys"); str("no"); title: "T","#;
    let found: Vec<String> = translated_literals(code).into_iter().map(|(t, _)| t).collect();
    assert_eq!(found, ["A {x}", "B", "{n} key", "T"]); // pattern by pattern
}
