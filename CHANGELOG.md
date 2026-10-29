# Changelog

## 0.0.2 — unreleased

- 2.4 GHz: the agent keeps the receiver's two vendor interfaces open, so that Linux polls them the
  way Windows and macOS do. Receivers of this kind can stop answering when a report waits unread
  there, which matches keys that stop arriving mid-typing until the cable goes in.
- 2.4 GHz: the agent asks the keyboard for its battery once a minute and records every change of the
  link in `~/.local/state/ak980/link.log`, so a dropout shows whether the receiver, the radio link or
  the PC failed.
- Security: the udev rule opens only the interfaces the app uses (screen and commands on the cable,
  3 and 4 on the receiver). It used to open the keystroke interfaces too, which let any program of
  the user read raw key presses past the Wayland compositor. The rule now sets those interfaces back
  to root-only whenever udev applies it, so an update closes them on a keyboard that stays plugged
  in: the Arch package does it while installing, and `ak980 setup` replaces a copy of the 0.0.1 rule
  left in `/etc/udev/rules.d`.

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
