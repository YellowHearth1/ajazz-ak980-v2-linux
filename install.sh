#!/bin/sh
# Installs Ajazz AK980 V2 | Software for Linux for the current user, straight from this checkout.
# Packages (PKGBUILD, AppImage) are the better choice where they exist; see README.md.
set -e
cd "$(dirname "$0")"
PREFIX="${PREFIX:-$HOME/.local}"
APPID=io.github.yellowhearth1.AjazzAK980V2

if ! command -v cargo >/dev/null; then
    echo "Rust is needed to build it: https://rustup.rs, or your distribution's cargo package"
    echo "(Arch/Artix: sudo pacman -S cargo; Debian/Ubuntu: sudo apt install cargo; Fedora: sudo dnf install cargo)."
    exit 1
fi

echo "==> Building (cargo build --release)"
cargo build --release --locked

echo "==> Programs: $PREFIX/bin"
install -Dm755 target/release/ak980 "$PREFIX/bin/ak980"
install -Dm755 target/release/ak980-gui "$PREFIX/bin/ak980-gui"

echo "==> Menu entry, icon, man page, completions"
install -Dm644 packaging/linux/$APPID.desktop "$PREFIX/share/applications/$APPID.desktop"
sed -i "s|^Exec=ak980-gui|Exec=$PREFIX/bin/ak980-gui|; s|^Exec=ak980 time|Exec=$PREFIX/bin/ak980 time|" \
    "$PREFIX/share/applications/$APPID.desktop"
install -Dm644 ak980/data/$APPID.svg "$PREFIX/share/icons/hicolor/scalable/apps/$APPID.svg"
install -Dm644 packaging/man/ak980.1 "$PREFIX/share/man/man1/ak980.1"
install -Dm644 packaging/completions/ak980.bash "$PREFIX/share/bash-completion/completions/ak980"
install -Dm644 packaging/completions/ak980.fish "${XDG_CONFIG_HOME:-$HOME/.config}/fish/completions/ak980.fish"

echo "==> Access to the keyboard"
"$PREFIX/bin/ak980" setup || echo "    Run '$PREFIX/bin/ak980 setup' later to allow access."

echo "==> Background agent"
"$PREFIX/bin/ak980" service list | grep -q "dinit-user" && best=dinit-user || best=session
"$PREFIX/bin/ak980" service list | grep -q "systemd-user" && best=systemd-user
"$PREFIX/bin/ak980" service enable "$best" && echo "    started as: $best"

case ":$PATH:" in
    *":$PREFIX/bin:"*) ;;
    *) echo "Note: $PREFIX/bin is not in PATH." ;;
esac
echo "Done. Open “Ajazz AK980 V2” from the menu or run: ak980"
