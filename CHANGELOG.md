# Changelog

## 0.0.1 — 2026-10-29

First public release.

- Screen: GIFs, pictures and folders of frames on the 240 × 135 display with fit/fill/stretch,
  colour adjustments, speed, trimming, frame stepping, captions, per-frame delays and deleting
  frames. Every upload is kept, because the keyboard cannot give an animation back.
- Clock: kept in sync by the background agent when the keyboard connects, after sleep, when the
  time or time zone changes, at midnight and hourly; over the cable or the 2.4 GHz dongle.
- Lighting: the 19 firmware effects with colour, rainbow, brightness, speed and direction; per-key
  colours; eight effects computed on the PC and streamed in real time (rainbow wave, colour drift,
  breathing, starlight, reactive, fire, music spectrum, music pulse).
- Keys: remapping on the base and Fn layers, including mouse, media, macros, several keys at once
  and launching programs or links through F13–F24.
- Macros: recording with real pauses, text, shortcuts, mouse clicks, editable pauses.
- Profiles: export/import, switching from the header and the tray, automatic switching while a
  program runs and back when it exits.
- Background agent and services for XDG autostart, systemd, dinit, OpenRC, runit and s6.
- Packages: Arch/Artix split PKGBUILD, AppImage, Makefile for other distributions; man page and
  bash/zsh/fish completions.
- Interface in English and Ukrainian.
