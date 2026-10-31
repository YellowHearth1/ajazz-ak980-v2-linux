---
version: 0.0.5
name: Ajazz AK980 V2 | Software for Linux
description: A configurator for one specific keyboard, built around a faithful drawing of that keyboard, for people who game and type on it every day.
source: crates/ak980-gui/src/theme.rs (every value below lives there)
typography:
  family: Ubuntu, Ubuntu Mono for numbers that are typed or compared (bundled, Ubuntu Font Licence)
  title: 21px medium
  heading: 15px medium
  text: 14px regular
  small: 12.5px regular (hints, counts, units)
spacing:
  space: 8px
  page-pad: 24px
  sidebar: 212px
rounded:
  radius: 8px
  radius-small: 6px
---

# Design

The window is a settings tool for one keyboard. The keyboard drawing is its centre: every page that
touches keys works on it, and the colours of the window come from the keyboard's own colourways.
Nothing moves for decoration; the only motion is the preview of an effect or an animation.

## Colour roles

Every widget style reads **roles**, never a colour. The roles are Catppuccin's
([style guide](https://github.com/catppuccin/catppuccin/blob/main/docs/style-guide.md)), so a theme
is a table of them:

| Role | Used for |
|---|---|
| `crust`, `mantle`, `base` | the sidebar, the stage behind the drawing, the page |
| `surface0`–`surface2` | cards, inputs, raised controls, their hover |
| `overlay0`, `overlay1` | hairlines, borders, the unconnected dot |
| `subtext0`, `subtext1`, `text` | hints and captions, secondary text, text |
| `accent` | primary buttons, the selected row of a list, the selected segment |
| `on_accent` | text on an accent fill |
| `accent_mark` | thin marks: focus, sliders, switches, the selected key's outline, the remap dot |
| `success`, `warning`, `danger` | the link dot, banners, destructive buttons and error notices |

Two themes follow the two colourways of the AK980 V2:

| | Graphite (dark) | Lavender (light) |
|---|---|---|
| page `base` | `#1B1C20` | `#F7F6FB` |
| `text` | `#E2E2DE` | `#2C2847` |
| `accent` | `#F2CF2E` (the yellow Esc, Enter and Space) | `#6F5CC9` (the lavender legends) |
| keyboard | black case, light grey caps, graphite modifiers, yellow accent caps | white case, white caps with violet legends, lavender modifiers, sky-blue accent caps |

Three more themes follow other AK980 colourways: *Mint* (black case, white letters, black modifiers,
mint accents), *Dusk* (black case, caps fading from black through grey to pink) and *Frost* (white
case, blue-grey modifiers, dark slate accents and arrows). Catppuccin Latte, Frappé, Macchiato and
Mocha come from the official palette with any of its 14 accents. "As the system" picks Graphite or Lavender from the desktop's light or dark preference.

## Contrast is computed, not hoped for

- Text roles reach 4.5:1 against every ground they sit on (WCAG AA); where a flavour's subtext or
  overlay falls short, it moves towards `text` just far enough.
- A fill keeps the accent's own colour and takes whichever text reads on it at 4.5:1 (`base`, or the
  darkest or lightest text). Only when none does (Latte's blue) is the fill shaded towards black, so
  the hue stays.
- `accent_mark` is the accent shaded until it reaches 3:1 against the page, for marks that are thin.
- The stage under the drawing is at least 1.4:1 from the keyboard's case, and the case gets an
  outline at 3:1 against the stage, so a black keyboard on a dark theme or a white one on a light
  theme never melts into the page.

`theme.rs` has tests for all of this, for every theme, accent and colourway.

## The keyboard drawing

Drawn from `ak980/data/layout.json` on a canvas. Caps have sides and a sculpted top; legends are
printed (the AK980 V2 has opaque caps), characters large and words small, as on the real caps. LED
light leaks out from under a cap in a thin rim. A selected key gets an outline in `accent_mark`, a
remapped key a dot. In paint mode the left button paints and drags, the middle one clears, the right
one picks a key's colour. The keyboard's colourway is chosen apart from the theme, and so is the model drawn: the V2 with its
screen and knob, or the AK980 with Insert, Print Screen, Scroll Lock and Pause in their place. A
colourway may give the arrows the accent (Frost) or leave the numpad's Enter a modifier (Mint), or
fade the caps from the top row to the bottom one (Dusk). Effects that react to keys ("Single on")
show typing on the drawing.

## Layout

A sidebar with the pages, the look (theme, keyboard colourway and model, Catppuccin's accent) and
the link state at its foot; a header with the page title, what runs on
the keyboard right now and the profile; notices; then the page, which scrolls. Settings are rows in
cards: the label (with a hint under it) on the left, the control on the right. Choices of a few
options are segmented and wrap onto a second line when the window is narrow. The narrowest window is
980 × 640; the screenshot test renders every page at that width in both languages
(`AK980_SHOTS_WIDTH=980`).

## Feedback

- Work on the keyboard runs on a thread of its own; the header says what runs.
- Notices appear at the bottom: confirmations go after 6 seconds, errors stay until closed.
- Deleting a profile or a macro, resetting a layer and clearing or filling the colours can be undone
  from a banner above the page until the next change.
- A screen upload is confirmed first, because the keyboard cannot give the old animation back.
- Icon-only buttons have tooltips. Everything works with the mouse alone.
- Colours are picked on a colour wheel (hue around, saturation outwards), next to swatches and a hex
  field.
- Agent notifications go through `notify-send`: a lost 2.4 GHz link, a battery at 20 % and 10 %,
  reminders.
