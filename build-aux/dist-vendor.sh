#!/bin/sh
# Called by Meson and the release workflow with DIST SOURCE_ROOT.
set -eu
mkdir -p "$1"
dist=$(cd "$1" && pwd -P)
source_root=$(cd "$2" && pwd -P)
mkdir -p "$dist/.cargo"

cd "$source_root"
# Preserve the full lockfile closure, including GNU and musl targets. Keeping
# each crate intact also preserves its Cargo checksum (including gettext).
cargo vendor --locked "$dist/vendor" > "$dist/.cargo/config.toml.tmp"
sed 's/^directory = ".*"/directory = "vendor"/' \
    "$dist/.cargo/config.toml.tmp" > "$dist/.cargo/config.toml"
rm "$dist/.cargo/config.toml.tmp"
