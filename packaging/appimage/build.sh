#!/bin/bash
# Builds dist/Ajazz_AK980_V2-<version>-x86_64.AppImage.
#
# Downloads, all from their official release pages:
#   * a relocatable Python from niess/python-appimage (manylinux2014, ~30 MB),
#   * PyQt6, numpy and Pillow wheels from PyPI (~110 MB),
#   * appimagetool from AppImage/appimagetool (~2 MB).
# The keyboard's udev rule cannot ship inside an AppImage: the app offers to install it on first run.
#
#   build.sh            full build; downloads are cached in build/appimage-cache
#   build.sh --refresh  reuse the last AppDir, only reinstall this checkout and repack
set -euo pipefail
# Where AppImageLauncher is installed, running an AppImage (appimagetool, the Python runtime) goes
# through its integration dialog; the build runs them directly.
export APPIMAGELAUNCHER_DISABLE=1

PYVER="${PYVER:-3.12}"
ARCH="${ARCH:-x86_64}"
here="$(cd "$(dirname "$0")" && pwd)"
root="$(cd "$here/../.." && pwd)"
work="${WORK:-$root/build/appimage}"
version="$(cd "$root" && python3 -c 'import ak980; print(ak980.__version__)')"
appid="io.github.yellowhearth1.AjazzAK980V2"
out="$root/dist/Ajazz_AK980_V2-$version-$ARCH.AppImage"

cache="${CACHE:-$root/build/appimage-cache}"
mkdir -p "$cache" "$root/dist"

if [ "${1:-}" = "--refresh" ] && [ -d "$work/AppDir" ]; then
    cd "$work"
    py="$(ls AppDir/opt/python3*/bin/python3.* | grep -v config | head -n 1)"
    echo "==> Reinstalling the app into the existing AppDir"
    "$py" -m pip install --no-deps --force-reinstall --no-warn-script-location "$root"
else
    rm -rf "$work"
    mkdir -p "$work"
    cd "$work"
    echo "==> Python $PYVER runtime"
    if [ ! -f "$cache/python-$PYVER-$ARCH.AppImage" ]; then
        auth=()      # anonymous API calls from shared CI runners run into the rate limit
        [ -n "${GITHUB_TOKEN:-}" ] && auth=(-H "Authorization: Bearer $GITHUB_TOKEN")
        url="$(curl -fsSL "${auth[@]}" "https://api.github.com/repos/niess/python-appimage/releases/tags/python$PYVER" |
               python3 -c "import json,sys; print(next(a['browser_download_url'] for a in json.load(sys.stdin)['assets'] if a['name'].endswith('manylinux2014_$ARCH.AppImage')))")"
        curl -fL --retry 3 -o "$cache/python-$PYVER-$ARCH.AppImage" "$url"
    fi
    cp "$cache/python-$PYVER-$ARCH.AppImage" python.AppImage
    chmod +x python.AppImage
    ./python.AppImage --appimage-extract >/dev/null
    mv squashfs-root AppDir
    py="$(ls AppDir/opt/python3*/bin/python3.* | grep -v config | head -n 1)"
    echo "==> Dependencies and the app"
    "$py" -m pip install --cache-dir "$cache/pip" --no-warn-script-location --upgrade pip
    "$py" -m pip install --cache-dir "$cache/pip" --no-warn-script-location "$root"
fi

echo "==> Launcher, desktop entry and icon"
rm -f AppDir/AppRun AppDir/*.desktop AppDir/*.png AppDir/*.svg AppDir/.DirIcon
rm -rf AppDir/usr/share/applications AppDir/usr/share/metainfo
install -m755 "$here/AppRun" AppDir/AppRun
install -Dm644 "$root/packaging/linux/$appid.desktop" "AppDir/$appid.desktop"
sed -i 's|^Exec=ak980-gui|Exec=AppRun|; s|^Exec=ak980 time|Exec=AppRun time|' "AppDir/$appid.desktop"
install -Dm644 "$root/ak980/data/$appid.svg" "AppDir/$appid.svg"
install -Dm644 "$root/ak980/data/$appid.svg" "AppDir/usr/share/icons/hicolor/scalable/apps/$appid.svg"
install -Dm644 "$root/packaging/linux/$appid.metainfo.xml" "AppDir/usr/share/metainfo/$appid.appdata.xml"
"$py" - "$root/ak980/data/$appid.svg" AppDir/.DirIcon <<'PY'
import sys
from PyQt6.QtGui import QGuiApplication, QImage, QPainter
from PyQt6.QtSvg import QSvgRenderer
app = QGuiApplication(["x", "-platform", "offscreen"])
img = QImage(256, 256, QImage.Format.Format_ARGB32)
img.fill(0)
p = QPainter(img)
QSvgRenderer(sys.argv[1]).render(p)
p.end()
img.save(sys.argv[2], "PNG")
PY

echo "==> Packing"
if [ ! -f "$cache/appimagetool-$ARCH.AppImage" ]; then
    curl -fL --retry 3 -o "$cache/appimagetool-$ARCH.AppImage" \
        "https://github.com/AppImage/appimagetool/releases/download/continuous/appimagetool-$ARCH.AppImage"
fi
cp "$cache/appimagetool-$ARCH.AppImage" appimagetool
chmod +x appimagetool
rm -f "$out"
ARCH="$ARCH" ./appimagetool --appimage-extract-and-run --no-appstream AppDir "$out"
echo "==> $out"
