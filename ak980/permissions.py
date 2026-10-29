"""Access to the keyboard without root: the udev rule."""

from __future__ import annotations

import os
import shlex

from .device import DONGLE_PID, VENDOR_ID, WIRED_PID
from .hid import enumerate_hid
from .service import run_as_root

RULE_NAME = "60-ajazz-ak980.rules"
RULE_SOURCE = os.path.join(os.path.dirname(__file__), "data", RULE_NAME)
RULE_LOCATIONS = ("/usr/lib/udev/rules.d/" + RULE_NAME, "/etc/udev/rules.d/" + RULE_NAME,
                  "/lib/udev/rules.d/" + RULE_NAME)


def rule_installed() -> bool:
    return any(os.path.exists(p) for p in RULE_LOCATIONS)


def rule_packaged() -> bool:
    """A distribution package ships the rule; a copy in /etc would shadow its future updates."""
    return any(os.path.exists(p) for p in RULE_LOCATIONS if not p.startswith("/etc/"))


def keyboard_nodes() -> list[str]:
    return [d.path for d in enumerate_hid(VENDOR_ID) if d.product_id in (WIRED_PID, DONGLE_PID)]


def accessible() -> bool | None:
    """True/False for the connected keyboard's nodes, None when no keyboard is connected."""
    nodes = keyboard_nodes()
    if not nodes:
        return None
    return all(os.access(p, os.R_OK | os.W_OK) for p in nodes)


def install_rule() -> None:
    """Puts the rule in /etc unless a package ships it, then applies it to the connected keyboard."""
    script = "set -e\n"
    if not rule_packaged():
        with open(RULE_SOURCE, encoding="utf-8") as f:
            rule = f.read()
        script += (
            "mkdir -p /etc/udev/rules.d\n"
            f"cat > /etc/udev/rules.d/{RULE_NAME} <<'AK980_EOF'\n{rule}AK980_EOF\n"
            f"chmod 644 /etc/udev/rules.d/{RULE_NAME}\n"
        )
    script += "udevadm control --reload-rules\n" + trigger_command(keyboard_nodes())
    run_as_root(script)


def trigger_command(nodes: list[str]) -> str:
    """Re-applies the rule to the keyboard's own nodes, with "change" as udev's reload hooks do.

    An "add" for every hidraw device would tell other programs their devices were plugged in again.
    """
    if not nodes:
        return ""       # nothing connected: the rule applies when the keyboard is plugged in
    names = " ".join(f"--sysname-match={shlex.quote(os.path.basename(n))}" for n in nodes)
    return f"udevadm trigger --action=change --subsystem-match=hidraw {names}\n"
