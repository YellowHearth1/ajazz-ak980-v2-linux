"""The udev rule installer: it must touch only the keyboard's own nodes."""

from ak980 import permissions


def test_trigger_targets_only_the_keyboard():
    assert permissions.trigger_command([]) == ""
    cmd = permissions.trigger_command(["/dev/hidraw0", "/dev/hidraw7"])
    assert cmd == ("udevadm trigger --action=change --subsystem-match=hidraw "
                   "--sysname-match=hidraw0 --sysname-match=hidraw7\n")


def test_install_rule(monkeypatch, tmp_path):
    scripts = []
    monkeypatch.setattr(permissions, "run_as_root", scripts.append)      # never asks for a password
    monkeypatch.setattr(permissions, "keyboard_nodes", lambda: ["/dev/hidraw3"])
    packaged = tmp_path / "usr" / permissions.RULE_NAME
    monkeypatch.setattr(permissions, "RULE_LOCATIONS", (str(packaged), "/etc/udev/rules.d/" + permissions.RULE_NAME))

    permissions.install_rule()                 # no package: the rule goes to /etc
    assert "cat > /etc/udev/rules.d/" in scripts[-1] and 'ATTRS{idVendor}=="0c45"' in scripts[-1]
    assert scripts[-1].endswith("--sysname-match=hidraw3\n") and "--action=add" not in scripts[-1]

    packaged.parent.mkdir(parents=True)
    packaged.write_text("# from the package\n")
    permissions.install_rule()                 # packaged: no shadowing copy in /etc, just apply it
    assert "/etc/udev" not in scripts[-1] and "udevadm control --reload-rules" in scripts[-1]
