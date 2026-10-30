# AJAZZ AK980 V2 protocol

Everything the app sends to the keyboard, byte by byte. Offsets are **payload offsets**: byte 0 is
the first byte after the HID report ID, which is always `0x00` (unnumbered reports).

Sources: the wired protocol was reverse engineered from the vendor's `DeviceDriver.exe` in
[donglover69/ak980-studio](https://github.com/donglover69/ak980-studio); the real-time stream comes
from [hcode10/OpenRGB-With-AulaF108Pro-support](https://github.com/hcode10/OpenRGB-With-AulaF108Pro-support)
(SignalRGB's AULA F108 Pro plugin, same firmware family). The code is in `ak980/device.py`,
`ak980/lcd.py`, `ak980/keymap.py`, `ak980/macro.py`.

## Devices and interfaces

| Link | USB ID | Interface | Usage page | Reports | Used for |
|---|---|---|---|---|---|
| cable | `0c45:8009` | 0 | `0x0001/0x06` | keyboard | typing |
| cable | `0c45:8009` | 1 | `0x000C` | consumer, mouse | knob, media keys |
| cable | `0c45:8009` | 2 | `0xFF68/0x61` | 4097-byte output, 65-byte input | screen pages |
| cable | `0c45:8009` | 3 | `0xFF13/0x01` | 65-byte feature, input, output | every command |
| dongle | `0c45:fefe` | 3 | `0xFF60/0x61` | 33-byte output and input | battery, clock |
| dongle | `0c45:fefe` | 4 | `0xFF59/0x61` | 65-byte output and input | unused |

On Linux each interface is a `/dev/hidrawN`; `ak980 list` prints them. Feature reports go through
`HIDIOCSFEATURE`/`HIDIOCGFEATURE`, output reports through `write()`. The udev rule
(`ak980/data/60-ajazz-ak980.rules`) gives the user the cable's interfaces 2 and 3 and the dongle's 3
and 4, matched by `bInterfaceNumber`, and sets every other interface of both to root-only: those
carry keystrokes. Since udev leaves a node alone when no rule sets its mode or group, that also
takes back what the 0.0.1 rule gave them as soon as the rules are applied again.

Checked on 2026-10-30: in sysfs the cable has interfaces 0–3 with the usage pages above and no
interface 4; the user may open the cable's 2 and 3 and the dongle's 3 and 4, and is refused the
cable's 0 and 1 and the dongle's 0–2; the clock and a lighting command go through over the cable.

In 2.4 GHz mode the keyboard's own USB interfaces disappear (the cable only charges), so the dongle
is the only way to reach it. The vendor driver does not configure the keyboard over the dongle either.

## Wired commands

Every command is a 64-byte packet sent as a feature report. The keyboard echoes the packet when it
is read back with a get-feature request; byte 3 of the echo is a status (1 = accepted, `0xFF` after
a commit).

```
byte 0   0x04        command marker
byte 1   opcode
byte 2   argument
byte 8   COUNT low   number of data packets that follow
byte 9   COUNT high
```

A transaction is

```
04 18                       begin
04 OP ARG .. COUNT          the command
COUNT × 64 bytes            data packets, no marker
04 02                       commit
04 F0                       apply (lighting and key layers)
```

The vendor driver waits 35 ms before each packet; the app does the same for configuration.

| Opcode | Arg | COUNT | Data | Meaning |
|---|---|---|---|---|
| `13` | 0 | 1 | `MODE R G B 0 0 0 0 RAINBOW BRIGHT SPEED DIR 0 0 AA 55` | lighting effect, modes 1–19, 0 = off |
| `13` | 0 | 1 | `80 … [9]=BRIGHT … [14]=AA [15]=55` | switch to per-key colours |
| `23` | 0 | 9 | `[idx R G B] × 144`, last two bytes `AA 55` | per-key colour table |
| `20` | 0 | 8 | `[idx R G B] × 128` | real-time frame, see below |
| `17` | 1 | 1 | `00 01 ALTTAB ALTF4 WIN FNSWAP SLEEP 00 RESPONSE … [62]=AA [63]=55` | keyboard options |
| `11` | 0 | 9 | 576 bytes, 4 per key index, `AA 55` at 574 | base layer |
| `27` | 0 | 9 | same | Fn layer |
| `19`, `15` | | | see *Macros* | macros |
| `28` | 0 | — | `[8]=1`, then one packet `00 SLOT 5A YY MM DD hh mm ss 00 DOW … [62]=AA [63]=55` | screen clock |
| `72` | SLOT | pages | 4 KiB pages on interface 2, see *Screen* | screen upload |
| `F5` | | | read-back of the colour table | diagnostics only |

Lighting brightness and speed are 0–5, direction 0 left, 1 down, 2 up, 3 right. Sleep is 0 never,
1 = 1 min, 2 = 5 min, 3 = 30 min; response (debounce) 1–5.

### Clock

```
04 18
04 28 00 00 00 00 00 00 01
00 SLOT 5A YY MM DD hh mm ss 00 DOW ... AA 55     YY = year − 2000, DOW 0 = Sunday
04 02
```

`SLOT` is the screen page the vendor app sends along ("selected list item + 1", normally 1).

On the AK980 V2 every clock packet switches the screen to its clock page, whatever `SLOT` says
(checked 2026-10-30 with 1 and 2, while the animation was on the screen). Setting the clock is
therefore also what puts the clock page back after the computer starts; the app sets it on connect
only when asked to.

### Screen

The panel is 240 × 135, RGB565. The upload blob is

```
[0]            frame count (1–140)
[1 .. n]       delay of each frame in 2 ms units (1–255)
[256 ..]       frames, RGB565 little-endian, rows top to bottom, 64 800 bytes each
padding        0xFF up to a multiple of 4096
```

It goes out as `04 18`, `04 72 SLOT .. .. .. .. .. PAGES`, then each 4 KiB page as one 4097-byte
output report on interface 2 (report ID 0 + page). The keyboard acknowledges every page with an
input report after writing it to flash, about 145 ms per page; then `04 02`. There is no command to
read the animation back.

The keyboard keeps one animation. An upload with `SLOT` 2 (2026-10-30, one frame) replaced the
animation that had been sent with `SLOT` 1, and the knob's screen menu then offered only the new
one and the clock page. After an upload the screen shows the animation. No known command switches
to the animation page without uploading, so showing something else for a while and going back
means uploading the old animation again: 28 frames are 444 pages, about 64 s of flash writes.

### Real-time frames

```
04 20 00 00 00 00 00 00 08      (then read the echo)
8 × 64 bytes                    [index R G B] × 128, index = LED index of the key
04 02                           (then read the echo)
```

Nothing is written to flash; the frame stays until the next one or until a configuration command
replaces it. Measured on an AK980 V2: one frame takes 3–15 ms; with 1 ms between reports the stream
reaches ~70 fps, with 2 ms ~40 fps. Sending reports back to back makes the keyboard stall the
control pipe (`EPIPE`) until the next transfer. The app streams at 30 fps with 2 ms gaps.

The logo light next to the arrows is not in this table: lighting every unused index (0, 14–18, 35,
36, 53, 54, 71, 72, 89, 90, 95, 97, 107–116, 120, 124–127) changes nothing on the keyboard.

### Key entries

Four bytes per key index (the key's LED index from `ak980/data/layout.json`):

| Entry | Meaning |
|---|---|
| `00 00 00 00` | default |
| `05 03 00 00` | disabled |
| `02 MODS CODE 00` | HID key with modifiers (1 Ctrl, 2 Shift, 4 Alt, 8 Win); a bare modifier is `02 BIT 00 00` |
| `01 01 BTN 00` | mouse: 1 left, 2 right, 4 middle, 3 double click, 10 button 4, 08 button 5 |
| `01 03 DIR 00` | wheel: 01 up, FF down |
| `03 LO HI 00` | consumer usage: CD play/pause, B7 stop, B6/B5 previous/next, E9/EA volume, E2 mute, 0192 calculator, 018A mail, 0183 player, 0194 computer, 0221–022A browser |
| `06 SLOT MODE COUNT` | macro; MODE 0 once, 1 COUNT times, 2 until pressed again, 3 while held, 4 click |
| `07 K1 K2 K3` | up to three keys at once |

Writing a layer replaces all of it. *Program or link* in the app is `02 00 68..73 00` (F13–F24),
which the agent turns into the action.

### Macros

```
04 19
04 15 00 00 00 00 00 00 PAGES
PAGES × 64 bytes
04 02
```

The image starts with a directory of 100 × 4 bytes (record offset LE16 + `00 00`, `FF FF FF FF`
for an empty macro). From byte 400, each macro is `[event count LE16][6 × 00][events]`; every step
is two 4-byte events, the action `00 00 CODE TYPE` (TYPE `B0` key down, `30` key up, `90` mouse
down, `10` mouse up; CODE is the HID usage or the mouse button mask) and the pause after it
`MS_LO MS_HI 00 50` (at least 10 ms). PAGES is size / 64 + 1, or + 2 when the size is not a
multiple of 64; the last two bytes are `AA 55`. The vendor driver refuses more than 0xA8C bytes of
(steps + macros) × 8.

## Dongle commands

33-byte output reports: report ID 0, the command, and the byte sum of everything before it in the
last byte. The reply repeats the first three command bytes and ends with its own byte sum (battery:
`20 01 00 64 … 85`); one request can bring two identical replies a few milliseconds apart. The
keyboard only answers while it is in 2.4 GHz mode; after a while idle the first request just wakes
the radio link.

| Command | Reply | Meaning |
|---|---|---|
| `20 01` | `20 01 00 LEVEL …` | battery in percent |
| `0C 10 00 00 SLOT 5A YY MM DD hh mm ss 00 DOW 00 00 00 AA 55` | echo | screen clock |

The vendor driver also sends screen data in 28-byte chunks after a `7F 03 00 SLOT COUNT24` header
over the dongle; the app does not implement it because the chunk layout is not settled.

The dongle (`0c45:fefe`) has five HID interfaces: 0 keyboard, 1 mouse, 2 consumer, system and NKRO
keyboard, 3 vendor page `FF60` (the commands above) and 4 vendor page `FF59` (65-byte reports, unused).
Linux reads interfaces 3 and 4 only while a program has their hidraw node open; Windows and macOS
always do. Some receivers with a vendor interface stop answering on every interface when a report
waits there unread (the kernel gave the Cooler Master mouse dongle `HID_QUIRK_ALWAYS_POLL` for that in
2025), and keyboards with this dongle are reported to drop under Linux but not under macOS. The agent
therefore keeps both nodes open while the dongle is plugged in. In 25 minutes of typing with both
open (2026-10-29), the dongle sent nothing it was not asked for, and typing does not reach interfaces 3
or 4. The udev rule opens interfaces 3 and 4 to the user and leaves 0–2, which carry keystrokes, to
root.

The one report the dongle has sent unasked so far came on interface 3 on 2026-10-30, one second after
the keyboard was switched from 2.4 GHz to the cable: `55 FC 04` and 29 zero bytes, without a byte sum.
Its meaning is not known; it looks like the dongle noticing that the keyboard left the radio link. The
agent writes every such report to `link.log`, so a real dropout should show whether the dongle says the
same then.

## What has been verified

| Command | On an AK980 V2 by this project | Elsewhere |
|---|---|---|
| clock over USB | yes, 2026-10-28 | ak980-studio |
| screen upload | yes, 2026-10-28; 2026-10-30: one animation only, whatever the slot | ak980-studio |
| clock page | yes, 2026-10-30: every clock packet shows the clock page | |
| real-time frames | yes, 2026-10-28: colours, timing and stall behaviour, then end to end through the agent | OpenRGB / SignalRGB on the F108 Pro |
| lighting effects (`13`) | sent and acknowledged, 2026-10-28 | ak980-studio |
| per-key colours, options, key layers, macros | yes, 2026-10-30 with the Rust version over the cable: per-key colours (WASD red, Esc green), Win lock, base layer (Shift+B, disabled key, macro, three keys at once, mute), Fn layer (Fn+Num1 → C), a macro typing "ok"; read from the keyboard's own input events, then restored | ak980-studio on an AK980 V2 |
| colour read-back (`F5`) | answers, but the table does not follow animated effects | ak980-studio on the F108 Pro |
| dongle battery | yes, 2026-10-29: `20 01 00 64` (100 %); about 120 ms for the first request after idle, 13 ms after | ak980-studio |
| dongle clock | yes, 2026-10-29: a test time sent over the dongle showed on the screen; the agent syncs on its own when the keyboard switches to 2.4 GHz | taken from the vendor driver |
| logo light | not addressable | |

If you can test a line marked "not yet", please open an issue with what you saw.
