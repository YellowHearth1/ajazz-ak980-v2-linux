# Maintainer: yellowhearth1
#
# Ajazz AK980 V2 | Software for Linux, built from the checkout this file is in:
#
#   git clone https://github.com/yellowhearth1/ajazz-ak980-v2-linux
#   cd ajazz-ak980-v2-linux
#   makepkg -si
#
# One package for every init system. The agent starts with the desktop session; a service of
# systemd, dinit, OpenRC, runit or s6 is switched on in the app (the Device page, or
# `ak980 service enable`), which writes it for the init system that runs and lets that init
# system check it first.

pkgname=ajazz-ak980-v2-linux
pkgver=0.0.5
pkgrel=1
pkgdesc="Ajazz AK980 V2 | Software for Linux: screen clock and GIFs, lighting, PC lighting effects, keys, macros, profiles"
arch=(x86_64 aarch64)
url="https://github.com/yellowhearth1/ajazz-ak980-v2-linux"
license=(MIT Ubuntu-font-1.0)      # the Ubuntu fonts are built into the window
# The window loads the display libraries at run time (Wayland or X11, Vulkan or OpenGL)
depends=(gcc-libs glibc libxkbcommon wayland libx11 libxcursor libxi libxrandr vulkan-icd-loader)
makedepends=(cargo)
optdepends=('libpulse: music lighting effects through parec'
            'pipewire: music lighting effects through pw-record'
            'polkit: allow keyboard access, reconnect the receiver and set up system services from the app'
            'libnotify: notifications from the background agent (low battery, lost 2.4 GHz link, reminders)'
            'dbus: the keyboard flashes when a desktop notification comes (dbus-monitor)'
            'xdg-utils: launcher keys that open links'
            'xdg-desktop-portal: file dialogs of the desktop')
backup=(etc/ak980/agent.conf)
install=packaging/arch/ajazz-ak980-v2-linux.install
# Up to 0.0.3 the service of each init system came in a package of its own
conflicts=(ajazz-ak980-v2-linux-openrc ajazz-ak980-v2-linux-dinit ajazz-ak980-v2-linux-runit ajazz-ak980-v2-linux-s6)
replaces=(ajazz-ak980-v2-linux-openrc ajazz-ak980-v2-linux-dinit ajazz-ak980-v2-linux-runit ajazz-ak980-v2-linux-s6)
# The release profile strips the programs: a -debug package would hold no symbols
options=(!debug)

# The sources are the checkout itself; cargo builds into src/, which makepkg -c removes afterwards.

prepare() {
  cd "$startdir"
  export RUSTUP_TOOLCHAIN=stable
  cargo fetch --locked --target "$(rustc -vV | sed -n 's/host: //p')"
}

build() {
  cd "$startdir"
  export RUSTUP_TOOLCHAIN=stable CARGO_TARGET_DIR="$srcdir/target"
  cargo build --frozen --release
}

check() {
  cd "$startdir"
  export RUSTUP_TOOLCHAIN=stable CARGO_TARGET_DIR="$srcdir/target"
  cargo test --frozen --release --workspace
}

package() {
  cd "$startdir"
  make DESTDIR="$pkgdir" PREFIX=/usr TARGETDIR="$srcdir/target/release" install

  install -Dm644 LICENSE "$pkgdir/usr/share/licenses/$pkgname/LICENSE"
  install -Dm644 ak980/data/fonts/UFL.txt "$pkgdir/usr/share/licenses/$pkgname/UFL.txt"
  install -Dm644 README.md "$pkgdir/usr/share/doc/$pkgname/README.md"
  install -Dm644 docs/PROTOCOL.md "$pkgdir/usr/share/doc/$pkgname/PROTOCOL.md"
}
