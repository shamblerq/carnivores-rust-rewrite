#!/bin/sh
# Builds the release archives into dist/:
#
#   omnivores-rust-<version>-linux-x86_64.tar.xz
#   omnivores-rust-<version>-windows-x86_64.zip
#
# each holding the launcher, the three games, README.md, LICENSE and
# QUICKSTART.txt in a folder of the same name. Give it `linux`, `windows`
# or both (the default). It needs what README.md's Building section lists
# for each, and xz or zip to pack them.
set -eu
cd "$(dirname "$0")/.."

version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n 1)
programs="omnivores-rust carnivores1-rs carnivores2-rs carnivores-iceage-rs"
# Programs to hand out keep no trace of the builder's home folder (README.md,
# Builds for sharing).
export RUSTFLAGS="${RUSTFLAGS:-"--remap-path-prefix=$HOME=~"}"

mkdir -p dist
for target in ${*:-linux windows}; do
    case $target in
    linux)
        cargo build --release --locked
        from=target/release
        ext=
        ;;
    windows)
        cargo build --release --locked --target x86_64-pc-windows-gnu
        from=target/x86_64-pc-windows-gnu/release
        ext=.exe
        ;;
    *)
        echo "usage: $0 [linux] [windows]" >&2
        exit 2
        ;;
    esac
    name=omnivores-rust-$version-$target-x86_64
    stage=dist/$name
    rm -rf "$stage"
    mkdir -p "$stage"
    for p in $programs; do
        cp "$from/$p$ext" "$stage/"
    done
    cp README.md LICENSE "$stage/"
    sed "s/@VERSION@/$version/g" packaging/QUICKSTART.txt >"$stage/QUICKSTART.txt"

    if [ "$target" = linux ]; then
        # The three games are one engine and differ in a few bytes: a window
        # reaching back over one of them (80 MB) packs all three in the
        # space of one.
        tar --owner=0 --group=0 --numeric-owner --sort=name -C dist -cf - "$name" |
            xz -T1 --lzma2=preset=6,dict=128MiB >"dist/$name.tar.xz"
        echo "dist/$name.tar.xz"
    else
        # Notepad's line ends, for the text files.
        for f in QUICKSTART.txt LICENSE; do
            sed 's/$/\r/' "$stage/$f" >"$stage/$f.crlf" && mv "$stage/$f.crlf" "$stage/$f"
        done
        (cd dist && rm -f "$name.zip" && zip -9 -q -r "$name.zip" "$name")
        echo "dist/$name.zip"
    fi
done
