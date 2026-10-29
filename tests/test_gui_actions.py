"""Drives the pages' own handlers in preview mode (AK980_PREVIEW_ONLY is set in conftest): nothing reaches a keyboard."""

import time

import pytest


@pytest.fixture(scope="module")
def win():
    pytest.importorskip("PyQt6.QtWidgets")
    from PyQt6.QtWidgets import QApplication
    app = QApplication.instance() or QApplication([])
    from ak980.gui import theme
    from ak980.gui.window import MainWindow
    theme.load_fonts()
    app.setStyleSheet(theme.stylesheet())
    w = MainWindow()
    assert w.preview_only
    w.resize(1280, 860)
    w.show()
    yield w
    w.engine.stop()
    w.close()


def wait_until(condition, timeout: float = 10.0) -> bool:
    from PyQt6.QtWidgets import QApplication
    end = time.monotonic() + timeout
    while time.monotonic() < end:
        QApplication.processEvents()
        if condition():
            return True
        time.sleep(0.01)
    return False


def test_lighting(win):
    page = win.pages[1]
    win.select_page(1)
    page.source.set_value("firmware")
    page.source_changed("firmware")
    page.modes.set_value(7)
    page.firmware_changed()
    assert win.cfg.profile.lighting.mode == 7 and win.cfg.profile.light_source == "firmware"
    page.source.set_value("host")
    page.source_changed("host")
    page.effects.set_value("fire")
    page.host_changed("fire")
    page.param_editor._set("height", 80)
    assert win.cfg.profile.light_source == "host" and win.cfg.profile.host_values["fire"]["height"] == 80
    page.effects.set_value("wave")
    page.host_changed("wave")                              # the brightness row moves to the new form
    assert wait_until(lambda: page.host_brightness.isVisibleTo(win))
    assert page.host_brightness.parentWidget() is page.param_editor
    assert wait_until(lambda: bool(win.stage.colors))     # the engine's preview frames arrive


def test_keys_and_launchers(win):
    from ak980.layout import by_index
    page = win.pages[3]
    win.select_page(3)
    key = by_index(55)
    page.select(key)
    page.kind.set_value("media")
    page.kind_changed("media")
    assert win.cfg.profile.keymap_top[55].kind == "media"
    page.kind.set_value("launch")
    page.editors.setCurrentIndex(page.editor_index["launch"])
    page.command.setText("obs")
    page.changed()
    action = win.cfg.profile.keymap_top[55]
    assert action.kind == "key" and [x.command for x in win.cfg.settings.launchers] == ["obs"]
    page.reset_key()
    assert 55 not in win.cfg.profile.keymap_top and not win.cfg.settings.launchers
    page.write()


def test_colours(win):
    from PyQt6.QtCore import Qt
    from ak980.layout import by_index
    page = win.pages[2]
    win.select_page(2)
    page.clear()
    page.on_stage_key(by_index(37), Qt.MouseButton.LeftButton)
    assert 37 in page.colors
    page.fill_all()
    page.rainbow()
    page.gradient()
    page.undo()
    page.write()
    assert win.cfg.profile.light_source == "custom" and win.cfg.profile.custom_colors


def test_macros(win):
    from ak980.macro import Macro, MacroStep
    page = win.pages[4]
    win.select_page(4)
    page.macros.append(Macro("test"))
    page.refresh(len(page.macros) - 1)
    page.add_step(MacroStep("down", 4))
    page.add_step(MacroStep("up", 4))
    page.table.selectRow(0)
    page.delay.setValue(120)
    page.set_delay()
    assert page.macro().steps[0].delay == 120
    page.delete_steps()
    assert len(page.macro().steps) == 1
    page.write()


def test_screen(win, tmp_path):
    from PIL import Image
    frames = [Image.new("RGB", (60, 40), (i * 40, 0, 0)) for i in range(5)]
    gif = tmp_path / "t.gif"
    frames[0].save(gif, save_all=True, append_images=frames[1:], duration=80, loop=0)
    page = win.pages[0]
    win.select_page(0)
    page.load(str(gif))
    assert wait_until(lambda: len(page.frames) == 5)
    page.timeline.current = 1
    page.frame_selected(1)
    page.frame_delay.setValue(300)
    page.set_frame_delay()
    assert wait_until(lambda: page.frames and page.frames[1][1] == 300)
    page.delete_frame(0)
    assert wait_until(lambda: len(page.frames) == 4)
    page.restore_frames()
    assert wait_until(lambda: len(page.frames) == 5)
    page.toggle_play()
    page.upload()                                          # preview mode: encodes, sends nothing
    assert wait_until(lambda: page.upload_button.isEnabled())


def test_profiles_and_device(win, tmp_path):
    from ak980.config import Profile
    profiles, device = win.pages[5], win.pages[6]
    win.select_page(5)
    win.cfg.add(Profile(name="Game"))
    profiles._changed()
    win.switch_profile("Game")
    assert win.cfg.active == "Game" and win.profile_box.currentText() == "Game"
    from ak980.config import export_profile, import_profile
    export_profile(win.cfg.profile, str(tmp_path / "p.json"))
    assert import_profile(str(tmp_path / "p.json")).name == "Game"
    win.select_page(6)
    device.win_lock.setChecked(True)
    device.send_options()
    assert win.cfg.profile.options.disable_win
    device.send_all()
    win.sync_clock()


def test_follows_external_changes(win):
    import os
    from ak980.config import Config, Profile
    assert wait_until(lambda: not win.tasks)
    other = Config.load(win.cfg.path)          # what the agent does when a game starts
    if other.get("Elsewhere") is None:
        other.add(Profile(name="Elsewhere"))
    other.active = "Elsewhere"
    other.save()
    later = time.time() + 5
    os.utime(win.cfg.path, (later, later))     # a new mtime even on a coarse clock
    win.update_connection()
    assert win.cfg.active == "Elsewhere" and win.profile_box.currentText() == "Elsewhere"
