"""Access to the keyboard without root: the udev rule."""

from __future__ import annotations

import os

from .device import DONGLE_PID, VENDOR_ID, WIRED_PID
from .hid import enumerate_hid
from .service import run_as_root

RULE_NAME = "60-ajazz-ak980.rules"
RULE_SOURCE = os.path.join(os.path.dirname(__file__), "data", RULE_NAME)
RULE_LOCATIONS = ("/usr/lib/udev/rules.d/" + RULE_NAME, "/etc/udev/rules.d/" + RULE_NAME,
                  "/lib/udev/rules.d/" + RULE_NAME)


def rule_installed() -> bool:
    return any(os.path.exists(p) for p in RULE_LOCATIONS)


def keyboard_nodes() -> list[str]:
    return [d.path for d in enumerate_hid(VENDOR_ID) if d.product_id in (WIRED_PID, DONGLE_PID)]


def accessible() -> bool | None:
    """True/False for the connected keyboard's nodes, None when no keyboard is connected."""
    nodes = keyboard_nodes()
    if not nodes:
        return None
    return all(os.access(p, os.R_OK | os.W_OK) for p in nodes)


def install_rule() -> None:
    with open(RULE_SOURCE, encoding="utf-8") as f:
        rule = f.read()
    script = (
        "set -e\n"
        "mkdir -p /etc/udev/rules.d\n"
        f"cat > /etc/udev/rules.d/{RULE_NAME} <<'AK980_EOF'\n{rule}AK980_EOF\n"
        f"chmod 644 /etc/udev/rules.d/{RULE_NAME}\n"
        "udevadm control --reload-rules\n"
        "udevadm trigger --action=add --subsystem-match=hidraw\n"
    )
    run_as_root(script)
