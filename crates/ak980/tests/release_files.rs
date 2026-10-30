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

/// `git clone`, `cd`, `makepkg -si`: one package from the checkout, whatever the init system.
#[test]
fn package_recipe_builds_this_version_as_one_package() {
    let pkgbuild = repo_file("PKGBUILD");
    assert!(pkgbuild.lines().any(|l| l == format!("pkgver={VERSION}")), "PKGBUILD builds another version");
    assert!(pkgbuild.lines().any(|l| l == "pkgname=ajazz-ak980-v2-linux"), "PKGBUILD makes more than one package");
    let install = pkgbuild.lines().find_map(|l| l.strip_prefix("install=")).expect("an install script");
    assert!(Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").join(install).is_file(), "no {install}");
}

#[test]
fn release_notes_say_how_to_install() {
    let install = repo_file("packaging/release-install.md");
    assert!(install.contains("makepkg -si") && install.contains("pacman -U") && install.contains("@VERSION@"));
}
