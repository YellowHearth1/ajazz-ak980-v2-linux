"""Command line: ak980 <command> ..."""

from __future__ import annotations

import argparse
import os
import sys

from . import APP_TITLE, HOMEPAGE, __version__
from .config import Config, Launcher, Rule, export_profile, import_profile, peek_language
from .device import Dongle, NotConnected, WiredKeyboard, connection_state, open_any
from .hid import HidError, enumerate_hid
from .i18n import set_language, tr
from .keymap import LAUNCHER_KEYS, KeyAction
from .layout import find_key, keys as layout_keys
from .lighting import (CAP_COLOR, DIRECTIONS, LIGHT_MODES, MAX_LEVEL, SIDE_LIGHT_MODES, SLEEP_CHOICES,
                       mode_by_id, mode_by_name)
from .macro import BUDGET_BYTES, Macro, used_bytes


def _color(s: str) -> tuple[int, int, int]:
    s = s.lstrip("#")
    if len(s) != 6:
        raise argparse.ArgumentTypeError(tr("a colour is written as RRGGBB: {value}", value=s))
    return int(s[0:2], 16), int(s[2:4], 16), int(s[4:6], 16)


def _onoff(s: str) -> bool:
    if s.lower() in ("on", "1", "yes", "true"):
        return True
    if s.lower() in ("off", "0", "no", "false"):
        return False
    raise argparse.ArgumentTypeError(tr("expected on or off"))


def _say(text: str) -> None:
    print(text)


# ---------------------------------------------------------------- commands

def cmd_list(a, cfg) -> None:
    for d in enumerate_hid(0x0C45):
        print(d)


def cmd_status(a, cfg) -> None:
    from . import permissions, runtime
    state = connection_state()
    _say(tr("Connection: {state}", state={"usb": tr("USB cable"), "2.4g": tr("2.4 GHz dongle"),
                                          None: tr("not found")}[state]))
    access = permissions.accessible()
    if access is False:
        _say(tr("Access: no permission — run `ak980 setup`"))
    info = Dongle.find()
    if info and state == "2.4g":
        with Dongle(info) as d:
            level = d.read_battery()
        _say(tr("Battery: {level}", level=f"{level}%" if level is not None else
                tr("no answer (is the keyboard in 2.4 GHz mode and awake?)")))
    _say(tr("Active profile: {name}", name=cfg.active))
    _say(tr("Background agent: {state}", state=tr("running") if runtime.agent_running() else tr("not running")))


def cmd_time(a, cfg) -> None:
    slot = a.slot if a.slot is not None else cfg.settings.clock_slot
    with open_any() as kb:
        ok = kb.sync_clock(slot=slot)
        if not ok:
            raise HidError(tr("The keyboard did not answer through the dongle (is it in 2.4 GHz mode?)"))
    _say(tr("Clock synced."))


def cmd_agent(a, cfg) -> None:
    from .agent import run
    sys.exit(run(system=a.system, interval=a.interval, once=a.once))


def cmd_lcd(a, cfg) -> None:
    from . import lcd
    if a.history:
        for path in lcd.history_entries():
            print(path)
        return
    if not a.file:
        raise SystemExit(tr("Give a GIF, an image or a folder of frames"))
    o = lcd.RenderOptions(fit=a.fit, background=a.background, flip_h=a.flip_h, flip_v=a.flip_v,
                          brightness=a.brightness, contrast=a.contrast, saturation=a.saturation,
                          speed=a.speed, fixed_delay=a.delay, trim_start=a.trim_start, trim_end=a.trim_end,
                          frame_step=a.step, text=a.text or "", text_color=a.text_color,
                          text_size=a.text_size, text_pos=a.text_pos, dither=not a.no_dither)
    src = lcd.Source.load(a.file)
    frames = src.render(o)
    if src.available(o) > len(frames):
        _say(tr("Using {used} of {total} frames (the screen holds {max}).", used=len(frames),
                total=src.available(o), max=lcd.LCD_MAX_FRAMES))
    if a.preview:
        imgs = [lcd.preview_565(f, o.dither) for f, _ in frames]
        imgs[0].save(a.preview, save_all=True, append_images=imgs[1:], duration=[d for _, d in frames], loop=0)
        _say(tr("Preview saved: {path}", path=a.preview))
        return
    blob = lcd.encode_animation(frames, o.dither)
    pages = len(blob) // lcd.LCD_PAGE
    _say(tr("{frames} frames, {pages} pages of 4 KiB, about {seconds} s", frames=len(frames), pages=pages,
            seconds=round(pages * lcd.SECONDS_PER_PAGE)))
    slot = a.slot if a.slot is not None else cfg.settings.lcd_slot
    _say(tr("Copy kept in {path}", path=lcd.save_history(frames, a.file)))
    with WiredKeyboard.open() as kb:
        def progress(p: float) -> None:
            n = int(p * 40)
            print(f"\r[{'#' * n}{'.' * (40 - n)}] {p:4.0%}", end="", flush=True)
        kb.upload_lcd(blob, slot=slot, progress=progress)
    print()
    _say(tr("Uploaded to the keyboard."))
    cfg.settings.last_image = os.path.abspath(a.file)
    cfg.save()


def cmd_effects(a, cfg) -> None:
    from .effects import EFFECTS
    _say(tr("Keyboard effects (stored in the keyboard, `ak980 light`):"))
    for m in LIGHT_MODES:
        print(f"  {m.id:2}  {tr(m.name)}")
    _say(tr("PC effects (need the agent or the app running, and the cable; `ak980 fx`):"))
    for e in EFFECTS.values():
        print(f"  {e.id:10} {tr(e.name)} — {tr(e.description)}")


def cmd_light(a, cfg) -> None:
    p = cfg.profile
    s = p.lighting
    if a.mode:
        s.mode = mode_by_name(a.mode).id
    if a.color:
        s.r, s.g, s.b = a.color
        s.rainbow = False
    if a.rainbow is not None:
        s.rainbow = a.rainbow
    if a.brightness is not None:
        s.brightness = a.brightness
    if a.speed is not None:
        s.speed = a.speed
    if a.direction is not None:
        s.direction = a.direction
    p.light_source = "firmware"
    with WiredKeyboard.open() as kb:
        kb.set_lighting(s)
    cfg.save()
    m = mode_by_id(s.mode)
    _say(tr("Lighting: {name}, brightness {b}, speed {s}", name=tr(m.name), b=s.brightness, s=s.speed)
         + (f", #{s.r:02x}{s.g:02x}{s.b:02x}" if m.caps & CAP_COLOR and not s.rainbow else ""))


def cmd_fx(a, cfg) -> None:
    from . import runtime
    from .effects import EFFECTS
    if a.list or not a.effect:
        for e in EFFECTS.values():
            print(f"{e.id:10} {tr(e.name)}")
            for prm in e.params:
                extra = (" " + "|".join(c[0] for c in prm.choices)) if prm.kind == "choice" else (
                    f" {prm.lo}..{prm.hi}" if prm.kind == "range" else " RRGGBB" if prm.kind == "color" else " on|off")
                print(f"    {prm.id}={extra.strip()}  ({tr(prm.label)})")
        return
    if a.effect not in EFFECTS:
        raise SystemExit(tr("Unknown PC effect: {name}", name=a.effect))
    eff = EFFECTS[a.effect]
    p = cfg.profile
    values = dict(p.host_values.get(eff.id, {}))
    for item in a.values:
        key, _, value = item.partition("=")
        prm = next((x for x in eff.params if x.id == key), None)
        if prm is None:
            raise SystemExit(tr("{effect} has no setting “{name}”", effect=eff.id, name=key))
        if prm.kind == "color":
            values[key] = _color(value)
        elif prm.kind == "range":
            values[key] = max(prm.lo, min(prm.hi, int(value)))
        elif prm.kind == "toggle":
            values[key] = _onoff(value)
        else:
            if value not in [c[0] for c in prm.choices]:
                raise SystemExit(tr("{name} is one of: {choices}", name=key,
                                    choices=", ".join(c[0] for c in prm.choices)))
            values[key] = value
    p.host_effect = eff.id
    p.host_values[eff.id] = values
    if a.brightness is not None:
        p.host_brightness = a.brightness
    p.light_source = "host"
    cfg.save()
    _say(tr("PC effect: {name}", name=tr(eff.name)))
    if not runtime.agent_running():
        _say(tr("The agent is not running, so nothing is streamed yet. Start it with `ak980 agent` or "
                "enable it: `ak980 service enable session`."))


def cmd_side(a, cfg) -> None:
    with WiredKeyboard.open() as kb:
        kb.set_side_light(SIDE_LIGHT_MODES[a.mode])


def cmd_custom(a, cfg) -> None:
    p = cfg.profile
    if a.read:
        with WiredKeyboard.open() as kb:
            table = kb.read_custom_table()
        for i in range(0, len(table), 64):
            print(table[i:i + 64].hex(" "))
        return
    colors = dict(p.custom_colors) if not a.clear else {}
    if a.all:
        colors = {k.index: a.all for k in layout_keys()}
    if a.wasd:
        for name in ("W", "A", "S", "D"):
            colors[find_key(name).index] = a.wasd
    for spec in a.key or []:
        name, _, col = spec.rpartition("=")
        colors[find_key(name).index] = _color(col)
    if a.brightness is not None:
        p.custom_brightness = a.brightness
    p.custom_colors = colors
    p.light_source = "custom"
    with WiredKeyboard.open() as kb:
        kb.set_custom_colors(colors, p.custom_brightness)
    cfg.save()
    _say(tr("Colours of {n} keys written.", n=len(colors)))


def cmd_options(a, cfg) -> None:
    o = cfg.profile.options
    changes = {"disable_win": a.win_lock, "disable_alt_tab": a.alt_tab_lock, "disable_alt_f4": a.alt_f4_lock,
               "fn_switch": a.fn_toggle, "sleep_time": a.sleep, "key_response": a.response}
    changed = any(v is not None for v in changes.values())
    for field, value in changes.items():
        if value is not None:
            setattr(o, field, value)
    if changed:
        with WiredKeyboard.open() as kb:
            kb.set_options(o)
        cfg.save()
    yes, no = tr("yes"), tr("no")
    _say(tr("Win key locked: {v}", v=yes if o.disable_win else no))
    _say(tr("Alt+Tab locked: {v}", v=yes if o.disable_alt_tab else no))
    _say(tr("Alt+F4 locked: {v}", v=yes if o.disable_alt_f4 else no))
    _say(tr("Fn toggles instead of being held: {v}", v=yes if o.fn_switch else no))
    _say(tr("Sleep after: {v}", v=tr(SLEEP_CHOICES[o.sleep_time]) if 0 <= o.sleep_time < 4 else o.sleep_time))
    _say(tr("Key response level: {v}", v=o.key_response))
    if not changed:
        _say(tr("(saved values shown; nothing was sent)"))


def cmd_keys(a, cfg) -> None:
    for k in sorted(layout_keys(), key=lambda k: (k.y, k.x)):
        print(f"{k.index:4}  {k.id:12} {k.label}")


def cmd_remap(a, cfg) -> None:
    from .layout import by_index
    p = cfg.profile
    table = p.keymap_fn if a.fn else p.keymap_top
    if a.list:
        names = [m.name for m in p.macros]
        for layer, t in ((tr("base"), p.keymap_top), ("Fn", p.keymap_fn)):
            for idx, act in sorted(t.items()):
                k = by_index(idx)
                print(f"[{layer}] {k.id if k else idx} → {act.describe(names)}  ({act.to_spec()})")
        if not p.keymap_top and not p.keymap_fn:
            _say(tr("No keys are remapped."))
        return
    if a.reset:
        table.clear()
    elif a.key and a.action:
        k = find_key(a.key)
        act = KeyAction.parse(a.action)
        if act.kind == "default":
            table.pop(k.index, None)
        else:
            table[k.index] = act
        print(f"{k.id} → {act.describe([m.name for m in p.macros])}  ({act.to_entry().hex(' ')})")
    else:
        raise SystemExit(tr("Give KEY ACTION, --reset or --list"))
    if a.dry:
        from .keymap import encode_layer
        data = encode_layer(table)
        for i in range(0, len(data), 64):
            print(data[i:i + 64].hex(" "))
        return
    with WiredKeyboard.open() as kb:
        kb.set_keymap(a.fn, table)
    cfg.save()
    _say(tr("Layer written to the keyboard."))


def cmd_macro(a, cfg) -> None:
    p = cfg.profile
    if a.action == "list":
        for i, m in enumerate(p.macros):
            print(f"{i:3}  {m.name}  ({len(m.steps)})")
            if a.verbose:
                for s in m.steps:
                    print("       " + s.describe())
        _say(tr("Memory: {used} of {total} bytes", used=used_bytes(p.macros), total=BUDGET_BYTES))
        return
    if a.action == "add":
        m = Macro.parse(a.spec, a.name)
        p.macros.append(m)
        _say(tr("Macro {n}: {name}. Assign it to a key: ak980 remap KEY macro:{n}", n=len(p.macros) - 1,
                name=m.name))
    elif a.action == "rm":
        removed = p.macros.pop(a.index)
        _say(tr("Removed: {name}. The macros after it moved up by one.", name=removed.name))
    elif a.action == "clear":
        p.macros.clear()
    with WiredKeyboard.open() as kb:
        kb.set_macros(p.macros)
    cfg.save()
    _say(tr("Macros written to the keyboard ({n}).", n=len(p.macros)))


def cmd_launch(a, cfg) -> None:
    s = cfg.settings
    if a.list or (not a.key and not a.rm):
        for x in s.launchers:
            print(f"{x.key:4} → {x.command}")
        if not s.launchers:
            _say(tr("No launcher keys. Example: ak980 launch F13 firefox, then ak980 remap F1 key:F13 --fn"))
        return
    if a.rm:
        s.launchers = [x for x in s.launchers if x.key.upper() != a.rm.upper()]
    else:
        key = a.key.upper()
        if key not in LAUNCHER_KEYS:
            raise SystemExit(tr("Launcher keys are F13–F24"))
        s.launchers = [x for x in s.launchers if x.key != key] + [Launcher(key, " ".join(a.command))]
        _say(tr("{key} runs: {cmd}. Now send {key} from a key: ak980 remap KEY key:{key}", key=key,
                cmd=" ".join(a.command)))
    cfg.save()


def cmd_profile(a, cfg) -> None:
    act = a.action
    if act == "list":
        for p in cfg.profiles:
            print(("* " if p.name == cfg.active else "  ") + p.name)
        return
    if act == "new":
        base = cfg.profile if a.copy else None
        from .config import Profile
        p = cfg.add(base.copy(a.name) if base else Profile(name=a.name))
        _say(tr("Profile created: {name}", name=p.name))
    elif act == "rm":
        cfg.remove(a.name)
    elif act == "rename":
        cfg.rename(a.name, a.new)
    elif act == "export":
        p = cfg.get(a.name)
        if p is None:
            raise SystemExit(tr("No profile named “{name}”", name=a.name))
        export_profile(p, a.file)
    elif act == "import":
        p = cfg.add(import_profile(a.file))
        _say(tr("Profile imported: {name}", name=p.name))
    elif act in ("use", "apply"):
        if act == "use":
            if cfg.get(a.name) is None:
                raise SystemExit(tr("No profile named “{name}”", name=a.name))
            cfg.active = a.name
        from .profiles import apply_profile
        with WiredKeyboard.open() as kb:
            apply_profile(kb, cfg.profile, step=lambda s: _say("… " + s))
        _say(tr("Profile “{name}” is on the keyboard.", name=cfg.active))
    cfg.save()


def cmd_auto(a, cfg) -> None:
    s = cfg.settings
    if a.action == "on":
        s.auto_switch = True
    elif a.action == "off":
        s.auto_switch = False
    elif a.action == "add":
        if cfg.get(a.profile) is None:
            raise SystemExit(tr("No profile named “{name}”", name=a.profile))
        s.rules = [r for r in s.rules if r.process.lower() != a.process.lower()] + [Rule(a.process, a.profile)]
    elif a.action == "rm":
        s.rules = [r for r in s.rules if r.process.lower() != a.process.lower()]
    elif a.action == "default":
        s.default_profile = a.profile or ""
    cfg.save()
    _say(tr("Automatic switching: {state}", state=tr("on") if s.auto_switch else tr("off")))
    for r in s.rules:
        print(f"  {r.process} → {r.profile}")
    if s.default_profile:
        _say(tr("After the program exits: {name}", name=s.default_profile))


def cmd_service(a, cfg) -> None:
    from . import service
    if a.action == "list":
        for m in service.methods():
            mark = "●" if m.enabled else "○"
            note = "" if m.available else "  " + tr("(not available here)")
            print(f"{mark} {m.id:14} {tr(m.title, init=m.id.split('-', 1)[-1])}{note}")
            print(f"  {'':14} {tr(m.detail)}")
        return
    (service.enable if a.action == "enable" else service.disable)(a.id)
    _say(tr("Done."))


def cmd_setup(a, cfg) -> None:
    from . import permissions
    if permissions.accessible() and not permissions.rule_outdated() and not a.force:
        _say(tr("The keyboard is already accessible."))
        return
    permissions.install_rule()
    _say(tr("Access rule installed. If the keyboard still is not accessible, reconnect it."))


def cmd_apply(a, cfg) -> None:
    from .profiles import apply_profile
    with WiredKeyboard.open() as kb:
        apply_profile(kb, cfg.profile, step=lambda s: _say("… " + s))
        kb.sync_clock(slot=cfg.settings.clock_slot)
    _say(tr("Done."))


def cmd_gui(a, cfg) -> None:
    from .gui import main
    sys.exit(main(tray=a.tray))


def build_parser() -> argparse.ArgumentParser:
    p = argparse.ArgumentParser(prog="ak980", description=APP_TITLE,
                                epilog=tr("Without a command the app window opens. Docs: {url}", url=HOMEPAGE))
    p.add_argument("--version", action="version", version=f"ak980 {__version__}")
    p.add_argument("--lang", choices=["auto", "en", "uk"], help=tr("interface language"))
    sub = p.add_subparsers(dest="cmd", metavar=tr("command"))

    def add(name, func, help_text, **kw):
        s = sub.add_parser(name, help=help_text, description=help_text, **kw)
        s.set_defaults(func=func)
        return s

    add("gui", cmd_gui, tr("open the app window")).add_argument("--tray", action="store_true",
                                                                 help=tr("start hidden in the tray"))
    add("status", cmd_status, tr("connection, battery, profile, agent"))
    add("list", cmd_list, tr("the keyboard's HID interfaces"))
    s = add("time", cmd_time, tr("sync the screen clock with the system time"))
    s.add_argument("--slot", type=int, help=tr("screen page sent with the time (default 1)"))

    s = add("agent", cmd_agent, tr("background agent: clock, PC effects, profiles, launcher keys"),
            aliases=["daemon"])
    s.add_argument("--system", action="store_true", help=tr("system service mode: clock only"))
    s.add_argument("--interval", type=int, help=tr("minutes between clock syncs (default 60)"))
    s.add_argument("--once", action="store_true", help=tr("sync once and exit"))

    s = add("lcd", cmd_lcd, tr("upload a GIF, an image or a folder of frames to the screen"))
    s.add_argument("file", nargs="?")
    s.add_argument("--fit", choices=["fit", "fill", "stretch"], default="fit")
    s.add_argument("--background", type=_color, default=(0, 0, 0), help="RRGGBB")
    s.add_argument("--flip-h", action="store_true")
    s.add_argument("--flip-v", action="store_true")
    s.add_argument("--brightness", type=float, default=1.0)
    s.add_argument("--contrast", type=float, default=1.0)
    s.add_argument("--saturation", type=float, default=1.0)
    s.add_argument("--speed", type=float, default=1.0)
    s.add_argument("--delay", type=int, help=tr("fixed frame delay in ms (2..510)"))
    s.add_argument("--trim-start", type=int, default=0)
    s.add_argument("--trim-end", type=int, default=0)
    s.add_argument("--step", type=int, default=1, help=tr("keep every Nth frame"))
    s.add_argument("--text")
    s.add_argument("--text-color", type=_color, default=(255, 255, 255))
    s.add_argument("--text-size", type=int, default=18)
    s.add_argument("--text-pos", default="bottom-right",
                   choices=["top-left", "top", "top-right", "center", "bottom-left", "bottom", "bottom-right"])
    s.add_argument("--no-dither", action="store_true")
    s.add_argument("--slot", type=int)
    s.add_argument("--preview", metavar="OUT.gif", help=tr("only save how it will look"))
    s.add_argument("--history", action="store_true", help=tr("list earlier uploads"))

    add("effects", cmd_effects, tr("list keyboard and PC lighting effects"), aliases=["modes"])
    s = add("light", cmd_light, tr("keyboard lighting effect (stored in the keyboard)"))
    s.add_argument("mode", nargs="?")
    s.add_argument("--color", type=_color)
    s.add_argument("--rainbow", type=_onoff)
    s.add_argument("--brightness", type=int, choices=range(0, MAX_LEVEL + 1))
    s.add_argument("--speed", type=int, choices=range(0, MAX_LEVEL + 1))
    s.add_argument("--direction", type=int, choices=range(4),
                   help=", ".join(f"{i} {tr(d)}" for i, d in enumerate(DIRECTIONS)))

    s = add("fx", cmd_fx, tr("PC lighting effect, e.g. ak980 fx spectrum palette=fire"))
    s.add_argument("effect", nargs="?")
    s.add_argument("values", nargs="*", metavar="setting=value")
    s.add_argument("--brightness", type=int, choices=range(0, 101), metavar="0..100")
    s.add_argument("--list", action="store_true")

    s = add("side", cmd_side, tr("side light (hidden in the vendor driver, may do nothing)"))
    s.add_argument("mode", choices=list(SIDE_LIGHT_MODES))

    s = add("custom", cmd_custom, tr("per-key colours stored in the keyboard"))
    s.add_argument("--all", type=_color)
    s.add_argument("--key", action="append", metavar="KEY=RRGGBB")
    s.add_argument("--wasd", type=_color)
    s.add_argument("--clear", action="store_true")
    s.add_argument("--brightness", type=int, choices=range(0, MAX_LEVEL + 1))
    s.add_argument("--read", action="store_true", help=tr("print the keyboard's colour table (diagnostics)"))

    s = add("options", cmd_options, tr("Win/Alt+Tab/Alt+F4 locks, Fn, sleep, key response"))
    s.add_argument("--win-lock", type=_onoff)
    s.add_argument("--alt-tab-lock", type=_onoff)
    s.add_argument("--alt-f4-lock", type=_onoff)
    s.add_argument("--fn-toggle", type=_onoff)
    s.add_argument("--sleep", type=int, choices=range(4), help=", ".join(
        f"{i} {tr(x)}" for i, x in enumerate(SLEEP_CHOICES)))
    s.add_argument("--response", type=int, choices=range(1, 6))

    add("keys", cmd_keys, tr("key names for remap and custom"))
    s = add("remap", cmd_remap, tr("remap a key"),
            epilog="default | off | key:ctrl+shift+A | mouse:left|right|middle|double|btn4|btn5|wheelup|wheeldown | "
                   "media:play|next|prev|volup|voldown|mute|calc|... | macro:N[:once|repeat:K|toggle|hold|click] | "
                   "multi:A,B,C")
    s.add_argument("key", nargs="?")
    s.add_argument("action", nargs="?")
    s.add_argument("--fn", action="store_true", help=tr("the Fn layer (while Fn is held)"))
    s.add_argument("--reset", action="store_true", help=tr("reset the whole layer"))
    s.add_argument("--list", action="store_true")
    s.add_argument("--dry", action="store_true", help=tr("only print the bytes"))

    s = add("macro", cmd_macro, tr("macros"))
    ms = s.add_subparsers(dest="action", required=True)
    m = ms.add_parser("list")
    m.add_argument("-v", "--verbose", action="store_true")
    m = ms.add_parser("add", help="text:Hello | combo:ctrl+shift+esc | steps:down:A,wait:50,up:A,click:left")
    m.add_argument("name")
    m.add_argument("spec")
    m = ms.add_parser("rm")
    m.add_argument("index", type=int)
    ms.add_parser("clear")
    ms.add_parser("upload")

    s = add("launch", cmd_launch, tr("launcher keys F13–F24: run a program or open a link"))
    s.add_argument("key", nargs="?")
    s.add_argument("command", nargs="*")
    s.add_argument("--rm", metavar="KEY")
    s.add_argument("--list", action="store_true")

    s = add("profile", cmd_profile, tr("profiles"))
    ps = s.add_subparsers(dest="action", required=True)
    ps.add_parser("list")
    x = ps.add_parser("use")
    x.add_argument("name")
    ps.add_parser("apply")
    x = ps.add_parser("new")
    x.add_argument("name")
    x.add_argument("--copy", action="store_true", help=tr("start from the active profile"))
    x = ps.add_parser("rm")
    x.add_argument("name")
    x = ps.add_parser("rename")
    x.add_argument("name")
    x.add_argument("new")
    x = ps.add_parser("export")
    x.add_argument("name")
    x.add_argument("file")
    x = ps.add_parser("import")
    x.add_argument("file")

    s = add("auto", cmd_auto, tr("switch profiles when a program starts"))
    au = s.add_subparsers(dest="action", required=True)
    au.add_parser("on")
    au.add_parser("off")
    au.add_parser("list")
    x = au.add_parser("add")
    x.add_argument("process")
    x.add_argument("profile")
    x = au.add_parser("rm")
    x.add_argument("process")
    x = au.add_parser("default")
    x.add_argument("profile", nargs="?")

    s = add("service", cmd_service, tr("start the agent automatically (XDG, systemd, dinit, OpenRC, runit, s6)"))
    ss = s.add_subparsers(dest="action", required=True)
    ss.add_parser("list")
    for verb in ("enable", "disable"):
        x = ss.add_parser(verb)
        x.add_argument("id", help="session | systemd-user | dinit-user | system-<init>")

    s = add("setup", cmd_setup, tr("let your user access the keyboard (installs the udev rule)"))
    s.add_argument("--force", action="store_true")
    add("apply", cmd_apply, tr("send the active profile and the time to the keyboard"))
    return p


def main(argv: list[str] | None = None) -> int:
    argv = sys.argv[1:] if argv is None else argv
    lang = peek_language()
    if "--lang" in argv:
        i = argv.index("--lang")
        if i + 1 < len(argv):
            lang = argv[i + 1]
    set_language(lang)
    cfg = Config.load()
    p = build_parser()
    a = p.parse_args(argv)
    if not a.cmd:
        from .gui import main as gui_main
        return gui_main()
    try:
        a.func(a, cfg)
    except (NotConnected, HidError, ValueError, PermissionError) as e:
        print(tr("Error: {message}", message=e), file=sys.stderr)
        return 1
    except OSError as e:
        print(tr("Error: {message}", message=e), file=sys.stderr)
        return 1
    except KeyboardInterrupt:
        return 130
    return 0
