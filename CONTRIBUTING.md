# Contributing

Thanks for helping. Issues with what you saw on your own keyboard are as welcome as code.

## Running from a checkout

```bash
python -m ak980            # the window
python -m ak980 --help     # the command line
QT_QPA_PLATFORM=offscreen python -m pytest -q
python -m pyflakes ak980 tests tools
```

`AK980_PREVIEW_ONLY=1` renders everything but sends nothing to the keyboard; `make screenshots`
uses it to regenerate `docs/screenshots` from a demo profile.

## Layout of the code

| Path | What it is |
|---|---|
| `ak980/hid.py` | hidraw enumeration, feature/output reports, the cross-process device lock |
| `ak980/device.py` | the keyboard protocol (wired and dongle) |
| `ak980/lcd.py`, `keymap.py`, `macro.py`, `lighting.py` | encoders for the screen, keys, macros, effects |
| `ak980/effects.py`, `engine.py`, `audio.py`, `keyinput.py` | PC lighting effects and their inputs |
| `ak980/config.py`, `profiles.py` | settings, profiles, sending a profile |
| `ak980/agent.py`, `service.py`, `permissions.py` | background agent, init systems, udev rule |
| `ak980/gui/` | the PyQt6 app; `theme.py` holds every design token |
| `ak980/locale/` | translations |
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

Interface strings are English and go through `tr()` (`ak980/i18n.py`). A language is one file,
`ak980/locale/<code>.py`, with a `MESSAGES` dict from the English string to the translation; plural
forms are tuples. Add the language to `LANGUAGES` in `i18n.py`. `tests/test_i18n.py` fails when a
string is missing or a placeholder differs.

## Design

Read `docs/DESIGN.md` before changing the interface. New colours, sizes and radii come from
`ak980/gui/theme.py`; if a token is missing, add it there first.

## Releases

1. Update the version in `ak980/__init__.py`, `packaging/arch/PKGBUILD` and the metainfo file, and
   add a `CHANGELOG.md` entry.
2. Tag `vX.Y.Z` and push the tag: GitHub Actions builds the AppImage and the source archive.
3. Run `updpkgsums` in `packaging/arch` and regenerate `.SRCINFO` with `makepkg --printsrcinfo`.
