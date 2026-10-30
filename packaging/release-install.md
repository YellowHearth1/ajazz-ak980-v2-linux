
## Install

**Arch Linux, Artix and derivatives:**

```bash
git clone https://github.com/yellowhearth1/ajazz-ak980-v2-linux
cd ajazz-ak980-v2-linux
makepkg -si
```

or the ready package below:

```bash
sudo pacman -U ajazz-ak980-v2-linux-@VERSION@-*-x86_64.pkg.tar.zst
```

**Any distribution:** `Ajazz_AK980_V2-@VERSION@-x86_64.AppImage` below. Make it executable
(`chmod +x`) and start it: without arguments it opens the window, with a command it is the `ak980`
command line (`./Ajazz_AK980_V2-@VERSION@-x86_64.AppImage status`).

One package serves every init system. The background agent starts with your desktop session; to run
it as a service of systemd, dinit, OpenRC, runit or s6, switch it on on the app's *Device* page.
More in the [README](https://github.com/yellowhearth1/ajazz-ak980-v2-linux#install).

## Встановлення

**Arch Linux, Artix і похідні:**

```bash
git clone https://github.com/yellowhearth1/ajazz-ak980-v2-linux
cd ajazz-ak980-v2-linux
makepkg -si
```

або готовий пакет нижче:

```bash
sudo pacman -U ajazz-ak980-v2-linux-@VERSION@-*-x86_64.pkg.tar.zst
```

**Будь-який дистрибутив:** `Ajazz_AK980_V2-@VERSION@-x86_64.AppImage` нижче. Зробіть файл
виконуваним (`chmod +x`) і запустіть: без аргументів відкриється вікно, з командою це командний
рядок `ak980` (`./Ajazz_AK980_V2-@VERSION@-x86_64.AppImage status`).

Один пакет для всіх init-систем. Фонова служба запускається разом із сеансом робочого столу; щоб
вона працювала як служба systemd, dinit, OpenRC, runit чи s6, увімкніть її на сторінці «Пристрій».
Докладніше — у [README українською](https://github.com/yellowhearth1/ajazz-ak980-v2-linux/blob/main/README.uk.md#встановлення).
