#!/bin/sh
# Removes what install.sh put into ~/.local. Profiles (~/.config/ak980) and the copies of uploaded
# animations (~/.local/share/ak980/history) stay.
PREFIX="${PREFIX:-$HOME/.local}"
APPID=io.github.yellowhearth1.AjazzAK980V2
if [ -x "$PREFIX/bin/ak980" ]; then
    for m in session systemd-user dinit-user; do "$PREFIX/bin/ak980" service disable "$m" >/dev/null 2>&1; done
fi
rm -rf "$PREFIX/share/ajazz-ak980-v2-linux"
rm -f "$PREFIX/bin/ak980" "$PREFIX/bin/ak980-gui" "$PREFIX/share/applications/$APPID.desktop" \
      "$PREFIX/share/icons/hicolor/scalable/apps/$APPID.svg" "$PREFIX/share/man/man1/ak980.1" \
      "$PREFIX/share/bash-completion/completions/ak980" "${XDG_CONFIG_HOME:-$HOME/.config}/fish/completions/ak980.fish"
if [ -f /etc/udev/rules.d/60-ajazz-ak980.rules ]; then
    echo "The access rule /etc/udev/rules.d/60-ajazz-ak980.rules stays; remove it with:"
    echo "    sudo rm /etc/udev/rules.d/60-ajazz-ak980.rules && sudo udevadm control --reload-rules"
fi
echo "Removed."
