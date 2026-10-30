# Changelog

## 0.0.3 — 2026-10-30

> **Written entirely by artificial intelligence.** The author needed software like this for the
> keyboard but does not have the skills to write it, and asked Claude, a large language model made by
> Anthropic, to make it. Use it at your own risk and expect mistakes.

- Packages for pacman: every release now carries `.pkg.tar.zst` packages for Arch, Artix and
  derivatives, installed with `sudo pacman -U`, next to the AppImage and the source archive.
- Clock: the agent sets the keyboard's clock once a week by default instead of on every connection
  and every hour. Every clock packet shows the AK980 V2's clock page, so the animation used to give
  way to the clock after each start of the computer. Hourly, daily, weekly or only by hand on the
  *Device* page and with `ak980 time --auto`; a new time zone is followed at once.
- Battery calibration: the battery measurements have a page of their own with the steps a
  calibration needs and the wear in percent.
- LED colours: a page of its own for how colours come out on the LEDs: per channel a strength (white
  looks pink, the red LEDs are the strongest) and a curve, tuned live on every key in a test colour;
  `ak980 light --balance` for the strengths.
- The AJAZZ logo light: no command reaches it, but the keyboard's own Fn + Z switches it off; the
  Lighting page lists that and the keyboard's other own keys.
- Reminders: at a set time a notification, the keys calling for attention (five patterns, any
  colour, for the minutes asked) and, if asked, the text blinking on the screen with the last
  animation put back afterwards; `ak980 remind`.
- Looks: the theme, the keyboard's colourway and model are in the sidebar; three more AK980
  colourways and themes (Mint, Dusk, Frost); the keyboard can be drawn as the AK980 with Insert,
  Print Screen, Scroll Lock and Pause instead of the screen and knob. The colourway cannot be
  detected: the keyboard reports the same name and product number in every colour.
- Colours are picked on a colour wheel; "Single on" and "Single off" are shown typing instead of
  with F and J lit; waves spreading from the centre take a centre from sliders or a click.
- Found on the keyboard: it keeps one animation whatever the upload's slot, and every clock packet
  shows the clock page (docs/PROTOCOL.md).

## 0.0.2 — 2026-10-30

> **Written entirely by artificial intelligence.** The author needed software like this for the
> keyboard but does not have the skills to write it, and asked Claude, a large language model made by
> Anthropic, to make it. Use it at your own risk and expect mistakes.

The whole program is rewritten in Rust: the `ak980` command and agent, and a new window on iced.
The bytes it sends to the keyboard are checked against what the Python version, verified on the
keyboard, sends for every kind of setting, and the program itself has been checked on an AK980 V2
over the receiver and over the cable.

- Size: the AppImage is 11 MB instead of 131 MB; the programs need only the C runtime and load the
  display libraries of the system.
- Themes: Graphite and Lavender from the two colourways of the AK980 V2, and Catppuccin's Latte,
  Frappé, Macchiato and Mocha with any of its 14 accents. The keyboard on the pages can be drawn in
  either colourway apart from the theme; every combination reads at WCAG AA.
- Services that cannot keep a system from booting: dinit descriptions are checked with dinit's own
  checker before they are written, files are replaced in one step, the init system's tool switches a
  service on and a refused one is taken away again. A per-user runit service (turnstile or
  userspawn) is new; the user's dinit is found from the running instance. The system agent gives up
  root for `nobody` with the `input` group. The README says how to switch every kind off from a
  console.
- 2.4 GHz: *Reconnect the receiver* (and `ak980 reconnect`) re-plugs the receiver through its USB
  `authorized` switch; `ak980 link` prints the link record; the agent notifies when the keyboard
  stops answering and when its battery gets low.
- Window: Undo after deleting a profile or a macro, resetting a layer or clearing the colours;
  errors stay until closed; settings are written at most twice a second and before the window
  closes.
- Fixes: deleting a macro no longer leaves keys pointing at the wrong macro (the keys follow their
  macros, those of the deleted one go back to default); the window, the agent and the CLI no longer
  share one temporary file when they save the settings; turning on another way to start the agent
  switches the old one off only once the new one works; `ak980 agent --once` always syncs.
- Beyond the vendor's app, with the vendor's own commands only:
  - PC effects: a system monitor (processor load, memory in use or the processor's temperature as a
    bar) and a typing heat map; over any PC effect, the lock keys while they are on and a flash for
    desktop notifications (only that one came is seen, never its text).
  - Profile keys: a Program or link key can switch to the next profile or to a named one
    (`profile:next`, `profile:use:NAME`); the agent does it at once.
  - Battery measurements: the agent learns from whole discharges and charges how long a full charge
    lasts and how long charging takes, reads the level off the charging curve while the keyboard
    charges on the cable in wired mode, and compares later cycles with the first. `ak980 battery`,
    the Device page and, when turned on, the battery line show it.
  - `ak980 status --json` and `--waybar`, for scripts and panels.
  - Screen editor: turn, zoom and pan, sharp pixels for pixel art, tones (black and white, sepia,
    negative), a hue shift, playing backward or there and back, running text over a picture, an
    outline for captions; the same options in `ak980 lcd`.
- Not in this version yet: the tray icon.
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

**Outdated.** The Python version; the project has gone another way, and 0.0.1 is no longer
developed. Use 0.0.2 or later.

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
