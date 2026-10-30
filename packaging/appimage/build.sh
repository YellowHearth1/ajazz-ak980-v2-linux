#!/bin/bash
# Builds dist/Ajazz_AK980_V2-<version>-<arch>.AppImage from the release binaries.
#
# Both programs link only libc, libm and libgcc_s; the window loads the display libraries of the
# system (Wayland or X11, Vulkan or OpenGL) at run time, so nothing else goes into the image.
# Downloads appimagetool from AppImage/appimagetool (~15 MB) once, into build/appimage-cache.
# The keyboard's udev rule cannot ship inside an AppImage: the app offers to install it.
#
#   build.sh             cargo build --release, then pack
#   build.sh --no-build  pack the binaries already in target/release
set -euo pipefail
# Where AppImageLauncher is installed, running appimagetool goes through its integration dialog
export APPIMAGELAUNCHER_DISABLE=1

ARCH="${ARCH:-x86_64}"
here="$(cd "$(dirname "$0")" && pwd)"
root="$(cd "$here/../.." && pwd)"
work="${WORK:-$root/build/appimage}"
cache="${CACHE:-$root/build/appimage-cache}"
version="$(sed -n 's/^version = "\(.*\)"/\1/p' "$root/Cargo.toml" | head -n 1)"
appid="io.github.yellowhearth1.AjazzAK980V2"
out="$root/dist/Ajazz_AK980_V2-$version-$ARCH.AppImage"
mkdir -p "$cache" "$root/dist"

if [ "${1:-}" != "--no-build" ]; then
    echo "==> cargo build --release"
    (cd "$root" && cargo build --release --locked)
fi

echo "==> AppDir"
rm -rf "$work"
mkdir -p "$work/AppDir/usr/bin"
cd "$work"
install -m755 "$root/target/release/ak980" "$root/target/release/ak980-gui" AppDir/usr/bin/
install -m755 "$here/AppRun" AppDir/AppRun
install -Dm644 "$root/packaging/linux/$appid.desktop" "AppDir/$appid.desktop"
sed -i 's|^Exec=ak980-gui|Exec=AppRun|; s|^Exec=ak980 time|Exec=AppRun time|' "AppDir/$appid.desktop"
install -Dm644 "$root/ak980/data/$appid.svg" "AppDir/$appid.svg"
install -Dm644 "$root/ak980/data/$appid.svg" "AppDir/usr/share/icons/hicolor/scalable/apps/$appid.svg"
install -Dm644 "$root/packaging/linux/$appid.metainfo.xml" "AppDir/usr/share/metainfo/$appid.appdata.xml"
if command -v rsvg-convert >/dev/null; then
    rsvg-convert -w 256 -h 256 "$root/ak980/data/$appid.svg" -o AppDir/.DirIcon
else
    cp "$root/crates/ak980-gui/assets/icon-128.png" AppDir/.DirIcon
fi

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
