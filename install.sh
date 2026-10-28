#!/bin/sh
# Installs Ajazz AK980 V2 | Software for Linux for the current user, straight from this checkout.
# Packages (PKGBUILD, AppImage) are the better choice where they exist; see README.md.
set -e
cd "$(dirname "$0")"
PREFIX="${PREFIX:-$HOME/.local}"
APPID=io.github.yellowhearth1.AjazzAK980V2
LIB="$PREFIX/share/ajazz-ak980-v2-linux"

missing=""
python3 -c "import PyQt6.QtWidgets, PyQt6.QtSvg" 2>/dev/null || missing="$missing PyQt6 (with QtSvg)"
python3 -c "import PIL" 2>/dev/null || missing="$missing Pillow"
python3 -c "import numpy" 2>/dev/null || missing="$missing numpy"
if [ -n "$missing" ]; then
    echo "Missing Python packages:$missing"
    echo "Arch/Artix: sudo pacman -S python-pyqt6 qt6-svg python-pillow python-numpy"
    echo "Debian/Ubuntu: sudo apt install python3-pyqt6 python3-pyqt6.qtsvg python3-pil python3-numpy"
    echo "Fedora: sudo dnf install python3-pyqt6 python3-pillow python3-numpy"
    exit 1
fi

echo "==> Program: $LIB"
rm -rf "$LIB/ak980"
mkdir -p "$LIB" "$PREFIX/bin"
cp -r ak980 "$LIB/"
find "$LIB" -name __pycache__ -prune -exec rm -rf {} +
for name in ak980 ak980-gui; do
    cmd="-m ak980"
    [ "$name" = ak980-gui ] && cmd="-m ak980 gui"
    printf '#!/bin/sh\nPYTHONPATH="%s${PYTHONPATH:+:$PYTHONPATH}" exec python3 %s "$@"\n' "$LIB" "$cmd" > "$PREFIX/bin/$name"
    chmod 755 "$PREFIX/bin/$name"
done

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
best=$(PYTHONPATH="$LIB" python3 -c "from ak980 import service; print(service.preferred_user_method())")
"$PREFIX/bin/ak980" service enable "$best" && echo "    started as: $best"

case ":$PATH:" in
    *":$PREFIX/bin:"*) ;;
    *) echo "Note: $PREFIX/bin is not in PATH." ;;
esac
echo "Done. Open “Ajazz AK980 V2” from the menu or run: ak980"
