# Omnivores Rust

Omnivores Rust is a Carnivores game launcher and a Rust rewrite of
**Carnivores**, **Carnivores 2** and **Carnivores: Ice Age**, with mod
support. It runs on Linux and Windows.

The rewrite is built on the [Bevy](https://bevyengine.org) engine. It reads
the original games' data files as they are and plays the way the originals
did: the same creatures and behaviour, weapons, maps, menus, hunter profiles
and trophy rooms. On top of that it adds widescreen support, an FOV slider, a frame limiter, a V-Sync toggle, anti-aliasing and more.

> **You need your own copies of the games.** This repository contains no
> game data: no maps, models, textures, sounds or artwork from the original
> games. Omnivores Rust loads them at runtime from a legally owned
> installation that the user needs to provide themself.
>
> This is an unofficial fan project. It is not affiliated with or endorsed
> by Action Forms, Tatem Games or any other holder of the Carnivores rights.
> "Carnivores" is a trademark of its respective owner.

## Contents

- [Download](#download)
- [What's in it](#whats-in-it)
- [What is needed to play](#what-is-needed-to-play)
- [Setting up games](#setting-up-games)
- [Mods](#mods)
- [Building](#building)
- [The launcher](#the-launcher)
- [Where things are saved](#where-things-are-saved)
- [Command line](#command-line)
- [Known limitations](#known-limitations)
- [Project layout](#project-layout)
- [License](#license)
- [AI disclosure](#ai-disclosure)
- [Credits](#credits)

## Download

Ready-built programs for Linux and Windows are on the
[releases page](https://github.com/2cupscarn/carnivores-rust-rewrite/releases)

- **Linux**: `omnivores-rust-<version>-linux-x86_64.tar.xz`. Unpack it and
  run `omnivores-rust`. It should support most up to date Linux distros from 2022 and later (Ubuntu 22.04,
  Debian 12, Fedora 36 or newer, etc.).
- **Windows**: `omnivores-rust-<version>-windows-x86_64.zip`. Unpack it and
  run `omnivores-rust.exe`. Windows may warn about a program from an unknown
  publisher: choose **More info → Run anyway**.

## What's in it

The games, as they were:

- the areas: ground, water, objects, sky, ground fog, cloud shadows, the
  light map with object shadows, time of day; the sun or the moon, and the
  glare of looking into the sun
- the hunter: walking, running, crouching, jumping, swimming, running out
  of breath under water, the death camera
- the creatures, with the originals' behaviour: seeing, hearing and
  smelling the hunter, fleeing, charging, attacking, flying
- the weapons (drawing, firing, reloading, the originals' hit detection and
  damage), binoculars, compass, wind gauge, the map, the dinosaur calls
  and their answers, the drop ship and the trophy card
- the games' own menus, drawn from their artwork: hunter profiles, the hunt
  screen and its prices, statistics, options, credits; hunt options such as
  tranquillisers, observer mode, camouflage, cover scent, radar and double
  ammo; the trophy room with its mounts
- sound through the games' own mixer (sixteen voices, the ambient bed, the
  distance roll-off and pan), resampled cleanly to the sound card's rate
- the HUD, the map and the menus drawn pixel for pixel at any resolution

Added:

- support for any resolution, including widescreen, windowed or full
  screen
- field of view slider (50-120), frame limiter, V-Sync toggle, anti-aliasing (MSAA) options
- object level of detail settings
- a show FPS toggle setting
- Switching between hold or toggle for crouching and running
- Clearer radar markers
- F12 screenshots

## What is needed to play

- The original game files for the game you want to play (Carnivores, Carnivores 2, Carnivores: Ice Age).
- Linux or Windows 10 and later, 64-bit.
- A graphics card and driver with Vulkan (Linux, Windows) or Direct3D 12
  (Windows) support.

## Setting up games

The games and mods live in one games folder, the *library*:

- Linux: `~/.omnivores`
- Windows: `%USERPROFILE%\Omnivores` (for example `C:\Users\You\Omnivores`)

The launcher's **Add games…** button fills it.

Folders and archives can also be dropped on the launcher's window, or given
to it on the command line. It shows what it found, each with its name, game
or mod, and which game it runs on, all of which can be changed, and copies
the ticked ones into the library. Nothing is changed where they came from.

Or copy each game's install folder (the folder that holds `HUNTDAT`) into
the library yourself:

```
Omnivores/
  Carnivores/
    HUNTDAT/
  Carnivores 2/
    HUNTDAT/
  Carnivores Ice Age/
    HUNTDAT/
  Carnivores 2 mods/
    Some Mod/
      HUNTDAT/
  Carnivores Ice Age mods/
    Another Mod/
      HUNTDAT/
```

The folder names are up to you; the launcher recognises the games from
what is inside them. Other folders can be added to the library in the
launcher's settings.

## Mods

All mods should work as long as they do not use a custom or modified engine
as their base.

## Building

Rust 1.87 or newer is needed to build.

### Linux

Install the libraries the build links against:

```sh
# Debian, Ubuntu
sudo apt install build-essential pkg-config libasound2-dev libudev-dev \
    libwayland-dev libxkbcommon-dev libx11-dev
# Fedora
sudo dnf install gcc-c++ pkgconf alsa-lib-devel systemd-devel \
    wayland-devel libxkbcommon-devel libX11-devel
# Arch
sudo pacman -S --needed base-devel pkgconf alsa-lib systemd-libs \
    wayland libxkbcommon libx11
```

Then, in this folder:

```sh
cargo build --release
```

This builds four binaries in `target/release`:

| Binary | |
|---|---|
| `omnivores-rust` | the launcher |
| `carnivores1-rs` | Carnivores |
| `carnivores2-rs` | Carnivores 2 |
| `carnivores-iceage-rs` | Carnivores: Ice Age |

Run `target/release/omnivores-rust`.

### Windows

The Windows binaries are cross-compiled from Linux with the MinGW-w64
toolchain. Install it and the Rust target once:

```sh
rustup target add x86_64-pc-windows-gnu
# and the MinGW-w64 compiler:
sudo apt install gcc-mingw-w64-x86-64      # Debian, Ubuntu
sudo dnf install mingw64-gcc               # Fedora
sudo pacman -S mingw-w64-gcc               # Arch
```

Then:

```sh
cargo build --release --target x86_64-pc-windows-gnu
```

The binaries land in `target/x86_64-pc-windows-gnu/release`:
`omnivores-rust.exe`, `carnivores1-rs.exe`, `carnivores2-rs.exe` and
`carnivores-iceage-rs.exe`

Run `omnivores-rust.exe`.

Building on Windows itself should also work with rustup's
`x86_64-pc-windows-gnu` toolchain and MSYS2's MinGW, but only the
cross-build above is tested. The MSVC toolchain should build them too, but
without their icons.

## The launcher

`omnivores-rust` lists every game and mod in the library, each with its
own name and icon, and starts it with the right engine.

- Double-click an entry, or select it and press Enter or **Play**.
- Type in the search box (Ctrl+F) to filter the list; F5 rescans the
  library.
- **Add games…** brings games and mods into the library (see
  [Setting up your games](#setting-up-your-games)); so does dropping
  folders and archives on the window.
- Right-click an entry to rename it, give it another icon, choose which
  game's engine it **Runs on**, reset it, or open its folder. Renames and
  icons only change the launcher's view; nothing in a game folder is
  touched. Ticking **In the applications menu** (**Start menu**) or **On
  the desktop** there gives that game or mod a shortcut of its own, which
  starts it without the launcher.
- **Settings** (Ctrl+,): the library folders, where the game programs are,
  your customised entries, the icon size, and **Install**.

On the first start it uses the library folder if it exists, and otherwise
offers to create it or to pick another folder. It finds the game programs
beside itself, then on the `PATH`; **Settings → Engines** can point it
elsewhere.

### Installing the launcher

Until it is in the applications menu the launcher offers to install itself;
**Settings → Install** does it any time. Installing copies the launcher and
the three games to a folder of their own, so that wherever they were
unpacked or built can go, and adds the launcher to the applications menu
(the Start menu on Windows) and the desktop, each as ticked:

| | Programs | Menu entry | Desktop shortcut |
|---|---|---|---|
| Linux | `~/.local/bin` | `~/.local/share/applications/omnivores-rust.desktop` | `omnivores-rust.desktop` on the desktop |
| Windows | `%LOCALAPPDATA%\Programs\Omnivores Rust` | `Omnivores Rust` in the Start menu | `Omnivores Rust` on the desktop |

It needs no administrator rights. Installing a newer version over an older
one, even while that is running, updates it. **Uninstall** on the same page
takes away the programs and every shortcut the launcher made; on Windows it
is also in the system's list of installed apps. The library and the
settings stay.

The launcher's own command line:

| | |
|---|---|
| `omnivores-rust FOLDER\|ARCHIVE…` | opens the launcher offering those for adding |
| `--install` | installs as above, without a window; `--no-copy`, `--no-menu` and `--no-desktop` leave out a part |
| `--uninstall` | uninstalls, without a window |
| `--version`, `--help` | |

The games can also be started without the launcher: run one from its game
folder (the one that holds `HUNTDAT`), or put the program in that folder,
or pass `data=PATH`.

## Where things are saved

| What | Where |
|---|---|
| Hunter profiles, trophies | `trophy0N.sav` in the game's folder, shared with the original games |
| Game options | `~/.config/carnivores-rs/<game>.cfg`; Windows: `%APPDATA%\carnivores-rs` |
| Launcher settings, custom icons, shortcut icons | `~/.config/omnivores-rust`; Windows: `%APPDATA%\omnivores-rust` |
| The installed launcher and games | see [Installing the launcher](#installing-the-launcher) |
| Screenshots | `~/Pictures/<game> (Rust)`; Windows: `%USERPROFILE%\Pictures\<game> (Rust)` |

Hunter profiles are the games' own files, so a hunter made or a trophy won
in the original game is there in the rewrite and the other way round. The
rewrite writes only the profile record and leaves the options the originals
keep after it as they were; its own options go in its own file. On the
first start, with no options file yet, it takes its options from the game
folder's profile.

On Linux, where the system's default ALSA sound device will not open, the
PipeWire or PulseAudio device is used instead.

## Command line

| | |
|---|---|
| `data=PATH` | the game folder, instead of the working directory |
| `-window`, `-fullscreen` | window mode |
| `res=1280x800` | window size |
| `fov=75` | field of view, 50-120 |
| `fps=144` | frame limit, 0 for none |
| `vsync=0` | V-Sync off |
| `msaa=4` | anti-aliasing: 0, 2, 4 or 8 samples |
| `-nosound` | no sound |
| `prj=HUNTDAT/AREAS/AREA1` | skip the menus and hunt in that area |
| `x=300.5 y=300.5 a=90` | start at that cell, facing that many degrees from north |
| `dtm=2` | time of day (Carnivores 2, Ice Age): 0 dawn, 1 day, 2 night |
| `campos=512.50,512.50,45` | start at that spot, facing that way (as F10 shows it) |
| `-showpos` | the position readout from the start |
| `obj_lod=0`, `obj_lod_dist=N` | far objects as full models, or as sprites from N cells |
| `-debug` | the developer keys for this session, and debug mode in Carnivores 2 and Ice Age |

For testing: `spawn=TYPE,DIST` puts a creature of that type ahead of you,
`-nodinos` leaves out the rest, `-freeze` holds creatures still until they
are hit, `weapon=N` starts with that weapon drawn, `b=DEG` starts looking
down, and `shot=N shotfile=F.png` renders N frames of the hunt, saves a
screenshot and quits.

## Known limitations

- Mods built on a modified engine, such as the Modders Edition Engine, are
  not supported.
- The Windows builds have been tested only under Wine.

## Project layout

```
crates/carn_formats      reading the data files: areas, models,
                         characters, pictures, sounds (plain Rust)
crates/carn_engine       the games on Bevy: world, rendering, the hunter,
                         creatures, menus, sound, options
games/carnivores1        -> carnivores1-rs
games/carnivores2        -> carnivores2-rs
games/carnivores-iceage  -> carnivores-iceage-rs
launcher                 -> omnivores-rust
icons                    the programs' icons (PNG) and the script that
                         makes the .ico files
packaging                the script that makes the release archives, and
                         what goes in them
.github/workflows        the release builds
```

`cargo test` runs the tests. Some need a game's data and are skipped
without it; `CARN2_DATA="/path/to/Carnivores 2" cargo test` runs them too.

World units are the games' own: a map cell is 256 units across. Each
surface has a small shader (`crates/carn_engine/src/render/shaders`)
written to reproduce the originals' picture; colour arithmetic is done on
the games' stored values and converted for the screen only at the end.

## License

Omnivores Rust is free software under the GNU General Public License,
version 3 or later; see [LICENSE](LICENSE).

The license covers the code in this repository. The Carnivores games, their
data and their artwork belong to their respective owners and are not part
of this project.

## AI disclosure

This project is 99.99% vibe-coded with Claude.

## Credits

- Action Forms, for Carnivores, Carnivores 2 and Carnivores: Ice Age.
- The [Carn2-Menu](https://github.com/carnivores-cpe/Carn2-Menu) project
  (CC0), whose reconstruction of the Carnivores 2 menu guided the menus
  here.
- [Bevy](https://bevyengine.org), [egui](https://github.com/emilk/egui),
  [cpal](https://github.com/RustAudio/cpal) and the other crates listed in
  `Cargo.lock`.
