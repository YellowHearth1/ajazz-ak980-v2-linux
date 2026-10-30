# Ajazz AK980 V2 | Software for Linux
# Build with `make` (cargo, release). Packagers: `make DESTDIR=... PREFIX=/usr install` plus one init
# target (install-openrc, -dinit, -runit, -s6). The Python package in ak980/ is the 0.0.1
# implementation, kept as the reference the Rust parity tests compare against (python-* targets).

PREFIX     ?= /usr
DESTDIR    ?=
CARGO      ?= cargo
PYTHON     ?= python3
TARGETDIR  ?= target/release
SYSCONFDIR ?= /etc
BINDIR      = $(PREFIX)/bin
DATADIR     = $(PREFIX)/share
UDEVDIR    ?= $(PREFIX)/lib/udev/rules.d
SYSTEMDDIR ?= $(PREFIX)/lib/systemd
RUNITDIR   ?= $(SYSCONFDIR)/runit/sv
S6DIR      ?= $(SYSCONFDIR)/s6/sv

APPID   = io.github.yellowhearth1.AjazzAK980V2
SLUG    = ajazz-ak980-v2-linux
VERSION = $(shell sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n 1)
EXE     = $(BINDIR)/ak980
INIT    = ak980/data/init
SUBST   = sed "s|@AK980@|$(EXE)|g"

.PHONY: all build test lint screenshots install install-bin install-data install-systemd install-openrc \
        install-dinit install-runit install-s6 uninstall dist appimage clean python-test python-lint golden

all: build

build:
	$(CARGO) build --release --locked

test:
	$(CARGO) test --workspace --locked

lint:
	$(CARGO) clippy --workspace --all-targets --locked -- -D warnings

# Every page in both themes and languages, drawn without a window, for the README
screenshots:
	AK980_SHOTS=$(CURDIR)/docs/screenshots $(CARGO) test -p ak980-gui --locked -- --ignored screenshots

python-test:
	QT_QPA_PLATFORM=offscreen $(PYTHON) -m pytest -q

python-lint:
	$(PYTHON) -m pyflakes ak980 tests tools

# What the Python version sends to the keyboard, for crates/ak980-core/tests/parity.rs
golden:
	$(PYTHON) tools/golden.py

install: install-bin install-data install-systemd

install-bin:
	install -Dm755 $(TARGETDIR)/ak980 "$(DESTDIR)$(BINDIR)/ak980"
	install -Dm755 $(TARGETDIR)/ak980-gui "$(DESTDIR)$(BINDIR)/ak980-gui"

install-data:
	install -Dm644 ak980/data/60-ajazz-ak980.rules "$(DESTDIR)$(UDEVDIR)/60-ajazz-ak980.rules"
	install -Dm644 packaging/linux/$(APPID).desktop "$(DESTDIR)$(DATADIR)/applications/$(APPID).desktop"
	install -Dm644 packaging/linux/$(APPID).metainfo.xml "$(DESTDIR)$(DATADIR)/metainfo/$(APPID).metainfo.xml"
	install -Dm644 ak980/data/$(APPID).svg "$(DESTDIR)$(DATADIR)/icons/hicolor/scalable/apps/$(APPID).svg"
	install -Dm644 packaging/man/ak980.1 "$(DESTDIR)$(DATADIR)/man/man1/ak980.1"
	install -Dm644 packaging/completions/ak980.bash "$(DESTDIR)$(DATADIR)/bash-completion/completions/ak980"
	install -Dm644 packaging/completions/_ak980 "$(DESTDIR)$(DATADIR)/zsh/site-functions/_ak980"
	install -Dm644 packaging/completions/ak980.fish "$(DESTDIR)$(DATADIR)/fish/vendor_completions.d/ak980.fish"
	install -Dm644 packaging/linux/agent.conf "$(DESTDIR)$(SYSCONFDIR)/ak980/agent.conf"
	install -dm755 "$(DESTDIR)$(SYSCONFDIR)/xdg/autostart"
	$(SUBST) $(INIT)/xdg/ak980-agent.desktop > "$(DESTDIR)$(SYSCONFDIR)/xdg/autostart/ak980-agent.desktop"
	chmod 644 "$(DESTDIR)$(SYSCONFDIR)/xdg/autostart/ak980-agent.desktop"

install-systemd:
	install -dm755 "$(DESTDIR)$(SYSTEMDDIR)/system" "$(DESTDIR)$(SYSTEMDDIR)/user"
	$(SUBST) $(INIT)/systemd/ak980d.service > "$(DESTDIR)$(SYSTEMDDIR)/system/ak980d.service"
	$(SUBST) $(INIT)/systemd/ak980-agent.service > "$(DESTDIR)$(SYSTEMDDIR)/user/ak980-agent.service"
	chmod 644 "$(DESTDIR)$(SYSTEMDDIR)/system/ak980d.service" "$(DESTDIR)$(SYSTEMDDIR)/user/ak980-agent.service"

install-openrc:
	install -dm755 "$(DESTDIR)$(SYSCONFDIR)/init.d"
	$(SUBST) $(INIT)/openrc/ak980d > "$(DESTDIR)$(SYSCONFDIR)/init.d/ak980d"
	chmod 755 "$(DESTDIR)$(SYSCONFDIR)/init.d/ak980d"

install-dinit:
	install -dm755 "$(DESTDIR)$(SYSCONFDIR)/dinit.d/user"
	$(SUBST) $(INIT)/dinit/ak980d > "$(DESTDIR)$(SYSCONFDIR)/dinit.d/ak980d"
	$(SUBST) $(INIT)/dinit/user/ak980-agent > "$(DESTDIR)$(SYSCONFDIR)/dinit.d/user/ak980-agent"
	chmod 644 "$(DESTDIR)$(SYSCONFDIR)/dinit.d/ak980d" "$(DESTDIR)$(SYSCONFDIR)/dinit.d/user/ak980-agent"

install-runit:
	install -dm755 "$(DESTDIR)$(RUNITDIR)/ak980d"
	$(SUBST) $(INIT)/runit/ak980d/run > "$(DESTDIR)$(RUNITDIR)/ak980d/run"
	chmod 755 "$(DESTDIR)$(RUNITDIR)/ak980d/run"

install-s6:
	install -dm755 "$(DESTDIR)$(S6DIR)/ak980d-srv/dependencies.d"
	install -m644 $(INIT)/s6/ak980d-srv/type "$(DESTDIR)$(S6DIR)/ak980d-srv/type"
	install -m644 $(INIT)/s6/ak980d-srv/dependencies.d/base "$(DESTDIR)$(S6DIR)/ak980d-srv/dependencies.d/base"
	$(SUBST) $(INIT)/s6/ak980d-srv/run > "$(DESTDIR)$(S6DIR)/ak980d-srv/run"
	chmod 755 "$(DESTDIR)$(S6DIR)/ak980d-srv/run"

uninstall:
	rm -f "$(DESTDIR)$(BINDIR)/ak980" "$(DESTDIR)$(BINDIR)/ak980-gui"
	rm -f "$(DESTDIR)$(UDEVDIR)/60-ajazz-ak980.rules" "$(DESTDIR)$(DATADIR)/applications/$(APPID).desktop" \
	      "$(DESTDIR)$(DATADIR)/metainfo/$(APPID).metainfo.xml" "$(DESTDIR)$(DATADIR)/icons/hicolor/scalable/apps/$(APPID).svg" \
	      "$(DESTDIR)$(DATADIR)/man/man1/ak980.1" "$(DESTDIR)$(DATADIR)/bash-completion/completions/ak980" \
	      "$(DESTDIR)$(DATADIR)/zsh/site-functions/_ak980" "$(DESTDIR)$(DATADIR)/fish/vendor_completions.d/ak980.fish" \
	      "$(DESTDIR)$(SYSCONFDIR)/xdg/autostart/ak980-agent.desktop" "$(DESTDIR)$(SYSTEMDDIR)/system/ak980d.service" \
	      "$(DESTDIR)$(SYSTEMDDIR)/user/ak980-agent.service" "$(DESTDIR)$(SYSCONFDIR)/init.d/ak980d" \
	      "$(DESTDIR)$(SYSCONFDIR)/dinit.d/ak980d" "$(DESTDIR)$(SYSCONFDIR)/dinit.d/user/ak980-agent"
	rm -rf "$(DESTDIR)$(RUNITDIR)/ak980d" "$(DESTDIR)$(S6DIR)/ak980d-srv"

dist:
	mkdir -p dist
	tar --exclude-vcs --exclude="__pycache__" --exclude=".pytest_cache" --exclude="./dist" --exclude="./build" \
	    --exclude="*.egg-info" --exclude="./.claude" --exclude="./CLAUDE.md" --exclude="./target" \
	    --exclude="./src" --exclude="./pkg" --exclude="./packaging/arch/src" --exclude="./packaging/arch/pkg" \
	    --exclude="*.pkg.tar.*" \
	    --exclude="*.AppImage" --exclude="./packaging/arch/*.tar.gz" \
	    --transform "s|^\.|$(SLUG)-$(VERSION)|" -czf dist/$(SLUG)-$(VERSION).tar.gz .
	@echo dist/$(SLUG)-$(VERSION).tar.gz

appimage:
	packaging/appimage/build.sh

clean:
	$(CARGO) clean
	rm -rf build dist *.egg-info src pkg packaging/arch/src packaging/arch/pkg
	find . -name __pycache__ -prune -exec rm -rf {} +
