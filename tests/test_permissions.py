"""The udev rule installer: it must touch only the keyboard's own nodes."""

import re

from ak980 import cli, permissions


def _rule_lines():
    with open(permissions.RULE_SOURCE, encoding="utf-8") as f:
        return [line.strip() for line in f if line.strip() and not line.startswith("#")]


def _paths(monkeypatch, tmp_path):
    """A packaged location and an /etc copy under tmp_path, neither of them there yet."""
    packaged = tmp_path / "usr" / permissions.RULE_NAME
    etc = tmp_path / "etc" / permissions.RULE_NAME
    packaged.parent.mkdir()
    etc.parent.mkdir()
    monkeypatch.setattr(permissions, "ETC_RULE", str(etc))
    monkeypatch.setattr(permissions, "RULE_LOCATIONS", (str(packaged), str(etc)))
    return packaged, etc


def test_trigger_targets_only_the_keyboard():
    assert permissions.trigger_command([]) == ""
    cmd = permissions.trigger_command(["/dev/hidraw0", "/dev/hidraw7"])
    assert cmd == ("udevadm trigger --action=change --subsystem-match=hidraw "
                   "--sysname-match=hidraw0 --sysname-match=hidraw7\n")


def test_rule_opens_only_the_vendor_interfaces():
    lines = _rule_lines()
    others = lines.index('ENV{.AJAZZ_AK980}=="", GOTO="ajazz_ak980_end"')    # other devices: untouched
    closed = lines.index('MODE="0600", GROUP="root"')       # also takes back what 0.0.1 opened
    opened = [i for i, line in enumerate(lines) if "uaccess" in line]
    assert others < closed < opened[0] and len(opened) == 2
    assert 'ENV{.AJAZZ_AK980}=="usb", ATTRS{bInterfaceNumber}=="02|03"' in lines[opened[0]]
    assert 'ENV{.AJAZZ_AK980}=="dongle", ATTRS{bInterfaceNumber}=="03|04"' in lines[opened[1]]


def test_access_check_follows_the_rule():
    """accessible() looks at exactly the interfaces the rule opens."""
    found = dict(re.findall(r'ENV\{\.AJAZZ_AK980\}=="(\w+)", ATTRS\{bInterfaceNumber\}=="([0-9|]+)"',
                            "\n".join(_rule_lines())))
    in_rule = {permissions.WIRED_PID: found["usb"], permissions.DONGLE_PID: found["dongle"]}
    assert in_rule == {pid: "|".join(f"{i:02d}" for i in ifaces)
                       for pid, ifaces in permissions.APP_INTERFACES.items()}


def test_rule_outdated(monkeypatch, tmp_path):
    packaged, etc = _paths(monkeypatch, tmp_path)
    with open(permissions.RULE_SOURCE, encoding="utf-8") as f:
        current = f.read()
    assert not permissions.rule_outdated()                     # no copy in /etc
    etc.write_text(current)
    assert not permissions.rule_outdated()                     # this version's rule
    etc.write_text('SUBSYSTEM=="hidraw", ATTRS{idVendor}=="0c45", MODE="0660", TAG+="uaccess"\n')
    assert permissions.rule_outdated()                         # like 0.0.1's: every interface
    etc.write_text(current)
    packaged.write_text("# from the package\n")
    assert permissions.rule_outdated()                         # current, but it hides the package's


def test_install_rule(monkeypatch, tmp_path):
    scripts = []
    monkeypatch.setattr(permissions, "run_as_root", scripts.append)      # never asks for a password
    monkeypatch.setattr(permissions, "keyboard_nodes", lambda: ["/dev/hidraw3", "/dev/hidraw7"])
    packaged, etc = _paths(monkeypatch, tmp_path)

    permissions.install_rule()                 # no package: the rule goes to /etc
    assert f"cat > {etc} <<" in scripts[-1] and 'ATTRS{idVendor}=="0c45"' in scripts[-1]
    # every node of the keyboard, so that the rule closes its keystroke interfaces again
    assert scripts[-1].endswith("--sysname-match=hidraw3 --sysname-match=hidraw7\n")
    assert "--action=add" not in scripts[-1]

    packaged.write_text("# from the package\n")
    permissions.install_rule()                 # packaged: no shadowing copy in /etc, just apply it
    assert str(etc) not in scripts[-1] and "udevadm control --reload-rules" in scripts[-1]

    etc.write_text("# left by 0.0.1 without the package\n")
    permissions.install_rule()
    assert scripts[-1].startswith(f"set -e\nrm -f {etc}\nudevadm control --reload-rules\n")


def test_setup_replaces_an_outdated_rule(monkeypatch, capsys):
    installed = []
    monkeypatch.setattr(permissions, "install_rule", lambda: installed.append(True))
    monkeypatch.setattr(permissions, "accessible", lambda: True)
    monkeypatch.setattr(permissions, "rule_outdated", lambda: False)
    assert cli.main(["--lang", "en", "setup"]) == 0
    assert not installed and "already accessible" in capsys.readouterr().out
    monkeypatch.setattr(permissions, "rule_outdated", lambda: True)     # e.g. 0.0.1's copy in /etc
    assert cli.main(["--lang", "en", "setup"]) == 0
    assert installed == [True]
