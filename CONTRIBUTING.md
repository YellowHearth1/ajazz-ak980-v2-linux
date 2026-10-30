# Contributing

Thanks for helping. Issues with what you saw on your own keyboard are as welcome as code.

## Running from a checkout

```bash
cargo run -p ak980-gui              # the window
cargo run -p ak980 -- --help        # the command line
cargo test --workspace              # unit tests, parity with the Python version, translations
cargo clippy --all-targets -- -D warnings
make screenshots                    # every page, both themes and languages, without a window
cargo clean                         # target/ grows to gigabytes
```

Run heavy builds under `nice -n 19 ionice -c3` if your desktop is sensitive to load.

The screenshot test also takes `AK980_SHOTS_PICK=page:theme[:colourway[:accent]]`,
`AK980_SHOTS_LANG` and `AK980_SHOTS_WIDTH=980` (the narrowest window): look at a page you changed in
both languages at that width before calling it done.

`ak980/` holds the Python implementation of 0.0.1, which was verified on the keyboard. It stays as
the reference: `make golden` records what it sends for a profile with every kind of setting into
`crates/ak980-core/tests/golden.json`, and `tests/parity.rs` requires the Rust code to send the same
bytes. A difference is a bug in the port, not in the reference.

## Layout of the code

| Path | What it is |
|---|---|
| `crates/ak980-core/src/hid.rs` | hidraw enumeration, feature/output reports, the cross-process device lock |
| `crates/ak980-core/src/device.rs` | the keyboard protocol (wired and dongle) |
| `lcd.rs`, `keymap.rs`, `macros.rs`, `lighting.rs` | encoders for the screen, keys, macros, effects |
| `effects.rs`, `engine.rs`, `audio.rs`, `keyinput.rs` | PC lighting effects and their inputs |
| `config.rs`, `profiles.rs` | settings, profiles, sending a profile |
| `agent.rs`, `service.rs`, `permissions.rs` | background agent, init systems, udev rule |
| `crates/ak980/` | the `ak980` command |
| `crates/ak980-gui/` | the window (iced); `theme.rs` holds every design token |
| `crates/ak980-core/locale/` | translations |
| `ak980/data/` | layout, udev rule, init templates, fonts, icon (shared with 0.0.1) |
| `packaging/` | PKGBUILD, AppImage, desktop entry, man page, completions |

## Rules for hardware tests

- Never send a command that does not come from the vendor driver or a documented source. Some
  opcodes may put the firmware into its bootloader.
- Writing a key layer or the macro memory replaces what is stored. Note the current state first and
  restore it after the test.
- An uploaded animation cannot be read back. Do not upload test images over someone's animation
  without asking; the app keeps copies in `~/.local/share/ak980/history`, use them to restore.
- Real-time frames need at least ~1 ms between reports, or the keyboard stalls the pipe.
- Record what you verified in `docs/PROTOCOL.md` (the table at the end), with the date.

## Translations

Interface strings are English and go through `tr()` / `tr!()` (`crates/ak980-core/src/i18n.rs`). A
language is one JSON file, `crates/ak980-core/locale/<code>.json`, from the English string to the
translation; plural forms are lists of three (one, few, many). Add the language to `LANGUAGES` in
`i18n.rs`. `tests/i18n.rs` fails when a string the code translates is missing or a placeholder
differs.

## Design

Read `docs/DESIGN.md` before changing the interface. Styles read colour roles, never colours; sizes
and radii come from `crates/ak980-gui/src/theme.rs`. If a token is missing, add it there first.

## Releases

1. Update the version in `Cargo.toml`, `packaging/arch/PKGBUILD` and the metainfo file, and add a
   `CHANGELOG.md` entry; a heading with "(pre-release)" publishes a pre-release.
2. Tag `vX.Y.Z` and push the tag: GitHub Actions builds the AppImage and the source archive.
3. Run `updpkgsums` in `packaging/arch` and regenerate `.SRCINFO` with `makepkg --printsrcinfo`.
