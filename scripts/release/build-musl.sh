#!/bin/sh
set -eu

ROOT_DIR="$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)"
TARGETS="${TARGETS:-x86_64-unknown-linux-musl}"
BINS="${BINS:-yunexal-panel yunexal-setup}"

require_cmd() {
    if ! command -v "$1" >/dev/null 2>&1; then
        echo "Missing required command: $1" >&2
        exit 1
    fi
}

require_cmd cargo
require_cmd rustup
require_cmd file
require_cmd readelf

check_musl_binary() {
    bin_path="$1"

    if ! file "$bin_path" | grep -qi "musl"; then
        echo "ERROR: $bin_path is not recognized as musl-linked" >&2
        return 1
    fi

    if readelf -l "$bin_path" | grep -q "ld-linux"; then
        echo "ERROR: glibc interpreter detected in $bin_path" >&2
        return 1
    fi

    return 0
}

cd "$ROOT_DIR"

for target in $TARGETS; do
    echo "==> Building target: $target"
    rustup target add "$target" >/dev/null

    for bin in $BINS; do
        cargo build --release --target "$target" --bin "$bin"
        check_musl_binary "$ROOT_DIR/target/$target/release/$bin"
    done

done

echo "Musl-only release binaries built successfully."
