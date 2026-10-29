"""Device: keyboard options, clock, background agent, access, language and about."""

from __future__ import annotations

from PyQt6.QtCore import Qt, QTimer
from PyQt6.QtWidgets import QHBoxLayout, QSpinBox, QVBoxLayout, QWidget

from ... import APP_TITLE, HOMEPAGE, ISSUES, __version__, permissions, runtime, service
from ...i18n import LANGUAGES, tr
from ...lighting import SLEEP_CHOICES
from .. import theme
from ..widgets import Dot, Form, Segmented, SliderRow, StatusLine, Toggle, button, column, combo, heading, label
from . import Page

FIRMWARE_COMBOS = [
    ("Fn + Del", "Screen on or off"),
    ("Fn + Z", "Logo light on or off"),
    ("Fn + X", "All lights on or off"),
    ("Fn + knob press", "Knob switches between volume and the screen menu"),
]


class DevicePage(Page):
    title = "Device"
    icon = "device"
    stage_mode = None

    def __init__(self, win):
        super().__init__(win)
        o = self.profile.options

        # Keyboard options (stored in the keyboard, part of the profile)
        kb = Form()
        self.win_lock = Toggle(tr("Lock the Win key"), o.disable_win)
        self.alt_tab = Toggle(tr("Lock Alt+Tab"), o.disable_alt_tab)
        self.alt_f4 = Toggle(tr("Lock Alt+F4"), o.disable_alt_f4)
        self.fn = Toggle(tr("Fn works as a switch"), o.fn_switch)
        self.fn.setToolTip(tr("Press Fn once to turn the Fn layer on, again to turn it off"))
        for t in (self.win_lock, self.alt_tab, self.alt_f4, self.fn):
            kb.full(t)
        self.sleep = Segmented([(i, tr(x)) for i, x in enumerate(SLEEP_CHOICES)], o.sleep_time)
        kb.stacked(tr("Sleep after"), self.sleep)
        self.response = SliderRow(1, 5, o.key_response)
        self.response.setToolTip(tr("Debounce: lower reacts faster, higher ignores accidental double presses"))
        kb.stacked(tr("Key response"), self.response)
        self.options_button = button(tr("Send to keyboard"), "primary", "upload")
        self.options_button.clicked.connect(self.send_options)
        kb_col = column(heading(tr("Keyboard")), kb, self.options_button,
                        label(tr("Part of the active profile."), "dim"))

        # Clock and background agent
        agent = QVBoxLayout()
        agent.setSpacing(theme.S3)
        agent.addWidget(heading(tr("Background agent")))
        state_row = QHBoxLayout()
        self.agent_dot = Dot()
        state_row.addWidget(self.agent_dot)
        self.agent_state = label("")
        state_row.addWidget(self.agent_state, 1)
        agent.addLayout(state_row)
        agent.addWidget(label(tr("It keeps the screen clock right, runs PC lighting effects, switches profiles for "
                                 "games and handles launcher keys. Pick how it starts:"), "dim", wrap=True))
        self.methods_box = QVBoxLayout()
        self.methods_box.setSpacing(theme.S2)
        agent.addLayout(self.methods_box)
        clock = Form()
        self.interval = QSpinBox()
        self.interval.setRange(5, 1440)
        self.interval.setSuffix(tr(" min"))
        self.interval.setValue(self.cfg.settings.sync_interval_min)
        self.interval.valueChanged.connect(self._interval)
        clock.add(tr("Re-sync the clock every"), self.interval)
        self.clock_slot = QSpinBox()
        self.clock_slot.setRange(0, 8)
        self.clock_slot.setValue(self.cfg.settings.clock_slot)
        self.clock_slot.setToolTip(tr("Sent with the time, as the vendor app does; it also selects the screen page"))
        self.clock_slot.valueChanged.connect(self._slot)
        clock.add(tr("Screen page"), self.clock_slot)
        agent.addSpacing(theme.S2)
        agent.addWidget(heading(tr("Clock")))
        agent.addLayout(clock)
        agent.addStretch(1)

        # This computer
        pc = QVBoxLayout()
        pc.setSpacing(theme.S3)
        pc.addWidget(heading(tr("This computer")))
        access_row = QHBoxLayout()
        self.access_dot = Dot()
        access_row.addWidget(self.access_dot)
        self.access_text = label("", wrap=True)
        access_row.addWidget(self.access_text, 1)
        pc.addLayout(access_row)
        self.access_button = button(tr("Allow access"), None, "shield",
                                    tr("Installs the udev rule so your user can use the keyboard"))
        self.access_button.clicked.connect(self.win.fix_permissions)
        pc.addWidget(self.access_button)
        prefs = Form()
        self.language = combo([("auto", tr("System language"))] + list(LANGUAGES.items()),
                              self.cfg.settings.language)
        self.language.currentIndexChanged.connect(self._language)
        prefs.add(tr("Language"), self.language)
        self.animations = Toggle(tr("Interface animations"), self.cfg.settings.animations)
        self.animations.toggled.connect(self._animations)
        prefs.full(self.animations)
        self.tray = Toggle(tr("Closing the window keeps the app in the tray"), self.cfg.settings.close_to_tray)
        self.tray.toggled.connect(self._tray)
        prefs.full(self.tray)
        pc.addLayout(prefs)
        self.lang_note = StatusLine()
        pc.addWidget(self.lang_note)
        b = button(tr("Send everything again"), None, "upload",
                   tr("Lighting, options, macros, both key layers and the time — for example after a reset"))
        b.clicked.connect(self.send_all)
        pc.addWidget(b)
        pc.addSpacing(theme.S2)
        pc.addWidget(heading(tr("On the keyboard itself")))
        combos = Form()
        for keys, what in FIRMWARE_COMBOS:
            combos.add(tr(keys), label(tr(what), wrap=True))
        pc.addLayout(combos)
        pc.addSpacing(theme.S2)
        pc.addWidget(heading(tr("About")))
        about = label(
            f"{APP_TITLE} {__version__}<br>"
            + tr("Unofficial software, not affiliated with AJAZZ.") + "<br>"
            + tr("Protocol: {a} and {b}.", a='<a href="https://github.com/donglover69/ak980-studio">ak980-studio</a>',
                 b='<a href="https://github.com/hcode10/OpenRGB-With-AulaF108Pro-support">OpenRGB F108 Pro</a>')
            + "<br>" + f'<a href="{HOMEPAGE}">{tr("Project page")}</a> · <a href="{ISSUES}">{tr("Report a problem")}</a>'
            + " · MIT", "dim", wrap=True)
        about.setTextFormat(Qt.TextFormat.RichText)
        about.setOpenExternalLinks(True)
        about.setStyleSheet(f"QLabel {{ color: {theme.INK_DIM}; }} a {{ color: {theme.AMBER}; }}")
        pc.addWidget(about)
        pc.addStretch(1)

        self.root.addLayout(self.columns(kb_col, agent, pc, stretches=[4, 4, 4]), 1)
        self.method_toggles: dict[str, Toggle] = {}
        self._fill_methods()
        self.timer = QTimer(self, interval=3000)
        self.timer.timeout.connect(self.update_state)

    # ------------------------------------------------------------ keyboard options

    def refresh(self) -> None:
        o = self.profile.options
        for t, v in ((self.win_lock, o.disable_win), (self.alt_tab, o.disable_alt_tab),
                     (self.alt_f4, o.disable_alt_f4), (self.fn, o.fn_switch)):
            t.setChecked(v)
        self.sleep.set_value(o.sleep_time)
        self.response.set_value(o.key_response)

    def send_options(self) -> None:
        o = self.profile.options
        o.disable_win, o.disable_alt_tab = self.win_lock.isChecked(), self.alt_tab.isChecked()
        o.disable_alt_f4, o.fn_switch = self.alt_f4.isChecked(), self.fn.isChecked()
        o.sleep_time, o.key_response = self.sleep.value(), self.response.value()
        self.win.save()
        self.win.run(self.win.wired(lambda kb: kb.set_options(o)),
                     lambda _: self.win.notify(tr("Keyboard options sent")), tr("Sending options…"))

    def send_all(self) -> None:
        from ...profiles import apply_profile
        p, slot = self.profile, self.cfg.settings.clock_slot

        def work(kb):
            apply_profile(kb, p)
            kb.sync_clock(slot=slot)

        self.win.run(self.win.wired(work), lambda _: self.win.notify(tr("Everything sent to the keyboard")),
                     tr("Sending the profile…"))

    # ------------------------------------------------------------ agent

    def _fill_methods(self) -> None:
        while self.methods_box.count():
            item = self.methods_box.takeAt(0)
            if item.widget():
                item.widget().deleteLater()
        self.method_toggles.clear()
        for m in service.methods():
            box = QWidget()
            lay = QVBoxLayout(box)
            lay.setContentsMargins(0, 0, 0, 0)
            lay.setSpacing(2)
            t = Toggle(tr(m.title, init=service.INIT_TITLES.get(m.id.split("-", 1)[-1], "")), m.enabled)
            t.setEnabled(m.available)
            t.toggled.connect(lambda on, mid=m.id: self._method(mid, on))
            lay.addWidget(t)
            detail = label(tr(m.detail) if m.available else tr("Not available in this session."), "dim", wrap=True)
            detail.setContentsMargins(44, 0, 0, 0)
            lay.addWidget(detail)
            self.method_toggles[m.id] = t
            self.methods_box.addWidget(box)

    def _method(self, method_id: str, on: bool) -> None:
        def done(_):
            self.win.notify(tr("Background agent settings saved"))
            QTimer.singleShot(1200, self.update_state)

        def failed(message: str):
            self.win.error(message)
            self._fill_methods()

        self.win.run(lambda: (service.enable if on else service.disable)(method_id), done,
                     tr("Changing how the agent starts…"), device=False, failed=failed)

    def update_state(self) -> None:
        running = runtime.agent_running()
        self.agent_dot.set_color(theme.OK if running else theme.INK_OFF)
        self.agent_state.setText(tr("Running") if running else tr("Not running"))
        access = permissions.accessible()
        if access is None:
            self.access_dot.set_color(theme.INK_OFF)
            self.access_text.setText(tr("Keyboard not connected") if not permissions.rule_installed()
                                     else tr("Access rule installed"))
            self.access_button.setVisible(not permissions.rule_installed())
        elif access:
            self.access_dot.set_color(theme.OK)
            self.access_text.setText(tr("Your user can use the keyboard"))
            self.access_button.hide()
        else:
            self.access_dot.set_color(theme.WARN)
            self.access_text.setText(tr("No access to the keyboard yet"))
            self.access_button.show()

    def _interval(self, v: int) -> None:
        self.cfg.settings.sync_interval_min = v
        self.win.save()

    def _slot(self, v: int) -> None:
        self.cfg.settings.clock_slot = v
        self.win.save()

    def _language(self, _) -> None:
        self.cfg.settings.language = self.language.currentData()
        self.win.save()
        self.lang_note.setText(tr("The language changes the next time the app starts."))

    def _animations(self, on: bool) -> None:
        self.cfg.settings.animations = on
        self.stage.animations = on
        self.win.save()

    def _tray(self, on: bool) -> None:
        self.cfg.settings.close_to_tray = on
        self.win.save()

    def on_enter(self) -> None:
        self._fill_methods()
        self.update_state()
        self.timer.start()

    def on_leave(self) -> None:
        self.timer.stop()

    def connection_changed(self, state) -> None:
        self.options_button.setEnabled(state == "usb")
        self.update_state()
