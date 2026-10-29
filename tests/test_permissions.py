"""The udev rule installer: it must touch only the keyboard's own nodes."""

from ak980 import permissions


def test_trigger_targets_only_the_keyboard():
    assert permissions.trigger_command([]) == ""
    cmd = permissions.trigger_command(["/dev/hidraw0", "/dev/hidraw7"])
    assert cmd == ("udevadm trigger --action=change --subsystem-match=hidraw "
                   "--sysname-match=hidraw0 --sysname-match=hidraw7\n")


def test_revoke_only_the_keystroke_nodes():
    assert permissions.revoke_command(["/dev/hidraw3"], ["/dev/hidraw3"]) == ""
    cmd = permissions.revoke_command(["/dev/hidraw7", "/dev/hidraw10"], ["/dev/hidraw10"])
    assert "chmod 0600 /dev/hidraw7\n" in cmd and cmd.endswith("setfacl -b /dev/hidraw7; fi\n")
    assert "hidraw10" not in cmd


def test_rule_opens_only_the_vendor_interfaces():
    with open(permissions.RULE_SOURCE, encoding="utf-8") as f:
        rules = [line for line in f if "uaccess" in line and not line.startswith("#")]
    assert len(rules) == 2
    assert 'ENV{.AJAZZ_AK980}=="usb", ATTRS{bInterfaceNumber}=="02|03"' in rules[0]
    assert 'ENV{.AJAZZ_AK980}=="dongle", ATTRS{bInterfaceNumber}=="03|04"' in rules[1]


def test_install_rule(monkeypatch, tmp_path):
    scripts = []
    monkeypatch.setattr(permissions, "run_as_root", scripts.append)      # never asks for a password
    monkeypatch.setattr(permissions, "keyboard_nodes", lambda: ["/dev/hidraw3"])
    monkeypatch.setattr(permissions, "app_nodes", lambda: ["/dev/hidraw3"])
    packaged = tmp_path / "usr" / permissions.RULE_NAME
    monkeypatch.setattr(permissions, "RULE_LOCATIONS", (str(packaged), "/etc/udev/rules.d/" + permissions.RULE_NAME))

    permissions.install_rule()                 # no package: the rule goes to /etc
    assert "cat > /etc/udev/rules.d/" in scripts[-1] and 'ATTRS{idVendor}=="0c45"' in scripts[-1]
    assert scripts[-1].endswith("--sysname-match=hidraw3\n") and "--action=add" not in scripts[-1]

    packaged.parent.mkdir(parents=True)
    packaged.write_text("# from the package\n")
    permissions.install_rule()                 # packaged: no shadowing copy in /etc, just apply it
    assert "/etc/udev" not in scripts[-1] and "udevadm control --reload-rules" in scripts[-1]
