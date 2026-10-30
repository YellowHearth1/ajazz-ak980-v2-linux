//! The version in the files a release carries besides the programs follows Cargo.toml, so that a
//! release never ships a man page or AppStream data that still name the previous one.

use std::fs;
use std::path::Path;

const VERSION: &str = env!("CARGO_PKG_VERSION");

fn repo_file(path: &str) -> String {
    fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").join(path)).unwrap()
}

#[test]
fn man_page_names_this_version() {
    let man = repo_file("packaging/man/ak980.1");
    assert!(man.lines().next().is_some_and(|l| l.starts_with(".TH") && l.contains(&format!("\"ak980 {VERSION}\""))),
            "the .TH line of packaging/man/ak980.1 names another version");
}

#[test]
fn appstream_and_changelog_start_with_this_version() {
    let metainfo = repo_file("packaging/linux/io.github.yellowhearth1.AjazzAK980V2.metainfo.xml");
    let first = metainfo.lines().find(|l| l.contains("<release ")).unwrap_or_default();
    assert!(first.contains(&format!("version=\"{VERSION}\"")), "the newest AppStream release is not {VERSION}: {first}");
    let changelog = repo_file("CHANGELOG.md");
    let heading = changelog.lines().find(|l| l.starts_with("## ")).unwrap_or_default();
    assert!(heading.starts_with(&format!("## {VERSION} ")), "the newest changelog entry is not {VERSION}: {heading}");
}

#[test]
fn design_notes_describe_this_version() {
    let design = repo_file("docs/DESIGN.md");
    assert!(design.lines().any(|l| l == format!("version: {VERSION}")), "docs/DESIGN.md describes another version");
}

#[test]
fn package_recipe_builds_this_version() {
    let pkgbuild = repo_file("packaging/arch/PKGBUILD");
    assert!(pkgbuild.lines().any(|l| l == format!("pkgver={VERSION}")), "PKGBUILD builds another version");
    let srcinfo = repo_file("packaging/arch/.SRCINFO");
    assert!(srcinfo.contains(&format!("pkgver = {VERSION}")), ".SRCINFO is out of date: run makepkg --printsrcinfo");
}
