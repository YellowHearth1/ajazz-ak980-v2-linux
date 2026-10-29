"""Access to the keyboard without root: the udev rule."""

from __future__ import annotations

import os
import shlex

from .device import DONGLE_PID, VENDOR_ID, WIRED_PID
from .hid import enumerate_hid
from .service import run_as_root

RULE_NAME = "60-ajazz-ak980.rules"
RULE_SOURCE = os.path.join(os.path.dirname(__file__), "data", RULE_NAME)
ETC_RULE = "/etc/udev/rules.d/" + RULE_NAME
RULE_LOCATIONS = ("/usr/lib/udev/rules.d/" + RULE_NAME, ETC_RULE, "/lib/udev/rules.d/" + RULE_NAME)


def rule_installed() -> bool:
    return any(os.path.exists(p) for p in RULE_LOCATIONS)


def rule_packaged() -> bool:
    """A distribution package ships the rule; a copy in /etc would shadow its future updates."""
    return any(os.path.exists(p) for p in RULE_LOCATIONS if p != ETC_RULE)


def rule_outdated() -> bool:
    """udev reads a copy in /etc that is not this version's rule, or that hides the packaged one.

    0.0.1 wrote such a copy without a package, and its rule also opened the keystroke interfaces.
    """
    try:
        with open(ETC_RULE, encoding="utf-8") as f:
            copy = f.read()
    except OSError:
        return False
    if rule_packaged():
        return True
    with open(RULE_SOURCE, encoding="utf-8") as f:
        return f.read() != copy


# The interfaces the app opens (see the rule); the others carry keystrokes and stay closed to users
APP_INTERFACES = {WIRED_PID: (2, 3), DONGLE_PID: (3, 4)}


def keyboard_nodes() -> list[str]:
    """Every hidraw node of the keyboard and its dongle."""
    return [d.path for d in enumerate_hid(VENDOR_ID) if d.product_id in APP_INTERFACES]


def app_nodes() -> list[str]:
    """The nodes the app needs: the rule opens these and only these."""
    return [d.path for d in enumerate_hid(VENDOR_ID) if d.interface in APP_INTERFACES.get(d.product_id, ())]


def accessible() -> bool | None:
    """True/False for the nodes the app needs, None when no keyboard is connected."""
    nodes = app_nodes()
    if not nodes:
        return None
    return all(os.access(p, os.R_OK | os.W_OK) for p in nodes)


def install_rule() -> None:
    """Makes this version's rule the one udev reads, then applies it to the connected keyboard.

    The rule goes to /etc unless a package ships it; with a package, a copy left in /etc goes away.
    """
    script = "set -e\n"
    if not rule_packaged():
        with open(RULE_SOURCE, encoding="utf-8") as f:
            rule = f.read()
        script += (
            "mkdir -p /etc/udev/rules.d\n"
            f"cat > {ETC_RULE} <<'AK980_EOF'\n{rule}AK980_EOF\n"
            f"chmod 644 {ETC_RULE}\n"
        )
    elif os.path.exists(ETC_RULE):
        script += f"rm -f {ETC_RULE}\n"
    script += "udevadm control --reload-rules\n" + trigger_command(keyboard_nodes())
    run_as_root(script)


def trigger_command(nodes: list[str]) -> str:
    """Re-applies the rule to the keyboard's own nodes, with "change" as udev's reload hooks do.

    Every node of the keyboard, so that the keystroke interfaces lose what an older rule gave them.
    An "add" for every hidraw device would tell other programs their devices were plugged in again.
    """
    if not nodes:
        return ""       # nothing connected: the rule applies when the keyboard is plugged in
    names = " ".join(f"--sysname-match={shlex.quote(os.path.basename(n))}" for n in nodes)
    return f"udevadm trigger --action=change --subsystem-match=hidraw {names}\n"
