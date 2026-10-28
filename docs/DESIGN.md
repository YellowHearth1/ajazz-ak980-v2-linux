---
version: alpha
name: Ajazz AK980 V2 | Software for Linux
description: A configurator for one specific keyboard, built around a faithful drawing of that keyboard, for people who game and type on it every day.
colors:
  desk: "#191A1E"       # window ground, the desk the keyboard sits on
  case: "#212328"       # panels: the keyboard's case
  plate: "#1B1C20"      # inputs and lists: the plate between the keys
  keycap: "#2B2E35"     # buttons and raised controls
  keycap-hi: "#363A44"  # hover, selected rows
  edge: "#383C46"       # hairlines, used sparingly
  ink: "#ECE7DF"        # text: PBT legend white
  ink-dim: "#A39F98"    # secondary text, 6.0:1 on case
  ink-off: "#6F6C67"    # disabled text only
  amber: "#FF8F1F"      # the one signal: action, selection, focus
  ok: "#5BD08F"
  warn: "#F2C14E"
  error: "#FF6B5E"
typography:
  display:
    fontFamily: Fira Sans Condensed
    fontSize: 22px (title), 16px (section headings)
    fontWeight: 600
  body:
    fontFamily: Fira Sans
    fontSize: 13px
  data:
    fontFamily: Fira Mono
    fontSize: 12px
rounded:
  keycap: 7px
  control: 6px
  input: 5px
  stage: 0
spacing: [4, 8, 12, 20, 32]
---

## Overview

The product configures one physical object, the AK980 V2: a 98-key board with a 1.14" TFT screen
and a knob above the numpad. Its world is keycaps, an aluminium-grey case, light leaking between
the keys and the vendor's own dark driver with its orange accent. The app is built from those
materials instead of from a component kit: the window is the desk, panels are the case, controls
are keycaps, text is legend white, and one amber signal (the vendor's brand colour, which the brief
asked to follow) marks what you can act on.

## The signature

The keyboard itself, drawn from the vendor's layout file, is the stage of every page that touches
keys. LED colours glow between the caps and shine through the legends the way shine-through
keycaps do. On the Screen page the view moves in until the real 240 × 135 panel fills the stage and
its pixel grid shows; the animation plays on the keyboard's own screen, not in a generic preview
box. That move is the one piece of motion the app spends; it follows the *Interface animations*
setting.

## Colors

- **Desk / case / plate / keycap:** four steps of the same graphite, so surfaces separate by tone
  rather than by borders and shadows.
- **Ink `#ECE7DF`:** warm legend white, 13:1 on the case.
- **Ink-dim `#A39F98`:** labels and help text, 6.0:1 on the case.
- **Amber `#FF8F1F`:** primary buttons, the selected effect, the active navigation item, focus
  rings, slider fill. Text on amber uses `#1A1408` (7.7:1). Amber never decorates.
- **Status:** green, yellow and red only for real state (connected, needs attention, failed).

## Typography

Fira Sans Condensed SemiBold for the title, section headings and big numbers; Fira Sans for
everything else; Fira Mono only where a reader scans data (hex colours, bytes, frame delays,
percentages). Sentence case everywhere. The fonts are bundled for the AppImage and come from the
system in packages.

## Layout

A navigation rail of keycap-shaped items on the left; a header with the keyboard's name, the active
profile, the link state and the clock; the keyboard stage; the page's controls below it in columns
separated by thin rules. Pages that do not need the keyboard (Macros, Profiles, Device) hide the
stage. Columns never nest cards.

## Shapes and depth

Radius by role: keycap-like choices 7 px with a 3 px darker bottom edge (they read as keys), buttons
6 px, inputs 5 px, the stage none. The only shadow is the one under the keyboard on the desk.

## Components

- *Primary button:* amber fill, dark text, one per area at most (Upload, Write, Send).
- *Keycap grid:* choices that are keyboard things (effects, key actions) are drawn as keys.
- *Toggle:* on/off settings; *Segmented:* 2–4 exclusive choices; *Slider row:* slider plus its value
  in mono.
- *Colour picker:* always visible on the lighting pages (plane, hue strip, hex, presets), as in the
  vendor driver; a swatch that opens it elsewhere.
- *Toast:* a short line in the header that names what happened ("Lighting applied").

## Do's and don'ts

- Do derive every new colour, size and radius from the tokens above.
- Do write controls as actions ("Upload to keyboard", "Write layer to keyboard") and keep the same
  verb in the result ("Animation uploaded").
- Do show state honestly: say when something needs the cable, the agent, or another package.
- Don't use emoji, the stock icon sets or icons in tinted squares; the icons are drawn for this app.
- Don't use the defaults this design refuses: purple gradients, a second accent colour, glass
  panels, glow shadows, ALL-CAPS labels, decorative numbering, a lone neon accent on near-black.
