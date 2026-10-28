import os

from ak980 import service


def test_templates_render():
    for template in ("systemd/ak980-agent.service", "systemd/ak980d.service", "openrc/ak980d", "dinit/ak980d",
                     "dinit/user/ak980-agent", "runit/ak980d/run", "s6/ak980d-srv/run", "xdg/ak980-agent.desktop"):
        text = service.render(template, "/usr/bin/ak980")
        assert "@AK980@" not in text and "/usr/bin/ak980" in text and "agent" in text


def test_system_scripts():
    for init in service.SYSTEM_FILES:
        on = service.system_script(init, True)
        off = service.system_script(init, False)
        assert on.startswith("set -e") and "ak980d" in on and "ak980d" in off


def test_session_method_always_offered():
    ids = [m.id for m in service.methods()]
    assert "session" in ids


def test_xdg_without_packaged_entry(monkeypatch, tmp_path):
    monkeypatch.setattr(service, "start_agent_now", lambda: None)
    monkeypatch.setattr(service, "stop_agent_now", lambda: None)
    monkeypatch.setattr(service, "XDG_SYSTEM", str(tmp_path / "missing.desktop"))
    monkeypatch.setattr(service, "XDG_USER", str(tmp_path / "user" / "ak980-agent.desktop"))
    service.xdg_enable()
    assert os.path.exists(service.XDG_USER) and service.xdg_enabled()
    service.xdg_disable()
    assert not os.path.exists(service.XDG_USER) and not service.xdg_enabled()


def test_xdg_with_packaged_entry(monkeypatch, tmp_path):
    monkeypatch.setattr(service, "start_agent_now", lambda: None)
    monkeypatch.setattr(service, "stop_agent_now", lambda: None)
    system = tmp_path / "ak980-agent.desktop"
    system.write_text("[Desktop Entry]\nType=Application\nExec=/usr/bin/ak980 agent\n")
    monkeypatch.setattr(service, "XDG_SYSTEM", str(system))
    monkeypatch.setattr(service, "XDG_USER", str(tmp_path / "user" / "ak980-agent.desktop"))
    assert service.xdg_enabled()
    service.xdg_disable()
    assert "Hidden=true" in open(service.XDG_USER).read() and not service.xdg_enabled()
    service.xdg_enable()
    assert not os.path.exists(service.XDG_USER) and service.xdg_enabled()
