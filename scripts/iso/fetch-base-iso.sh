#!/bin/sh
set -eu

ROOT_DIR="$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)"
BASE_DIR="${BASE_DIR:-$ROOT_DIR/.build/alpine-base}"
TRACK="${TRACK:-latest-stable}"
ARCH="${ARCH:-x86_64}"
FLAVOR="${FLAVOR:-standard}"
BASE_URL="${BASE_URL:-https://dl-cdn.alpinelinux.org/alpine/$TRACK/releases/$ARCH/}"
ISO_NAME="${ISO_NAME:-}"
UNPACK="${UNPACK:-1}"

require_cmd() {
    command -v "$1" >/dev/null 2>&1 || {
        echo "Missing required command: $1" >&2
        exit 1
    }
}

resolve_iso_name() {
    if [ -n "$ISO_NAME" ]; then
        echo "$ISO_NAME"
        return 0
    fi

    curl -fsSL "$BASE_URL" \
        | grep -Eo "alpine-${FLAVOR}-[0-9]+\.[0-9]+\.[0-9]+-${ARCH}\.iso" \
        | sort -Vu \
        | tail -n1
}

unpack_iso() {
    iso_path="$1"
    out_dir="$2"
    rm -rf "$out_dir"
    mkdir -p "$out_dir"

    if command -v bsdtar >/dev/null 2>&1; then
        bsdtar -xf "$iso_path" -C "$out_dir"
    elif command -v 7z >/dev/null 2>&1; then
        7z x "$iso_path" -o"$out_dir" >/dev/null
    elif command -v xorriso >/dev/null 2>&1; then
        xorriso -osirrox on -indev "$iso_path" -extract / "$out_dir" >/dev/null
    else
        echo "No ISO extractor found (need bsdtar, 7z, or xorriso)." >&2
        exit 1
    fi
}

require_cmd curl
require_cmd grep
require_cmd sort
require_cmd tail
require_cmd sha256sum

mkdir -p "$BASE_DIR"

resolved_iso="$(resolve_iso_name)"
if [ -z "$resolved_iso" ]; then
    echo "Failed to resolve Alpine ISO name from: $BASE_URL" >&2
    exit 1
fi

iso_path="$BASE_DIR/$resolved_iso"
sha_path="$BASE_DIR/$resolved_iso.sha256"
unpack_dir="$BASE_DIR/unpacked"

echo "Resolved ISO: $resolved_iso"
curl -fL "$BASE_URL$resolved_iso" -o "$iso_path"
curl -fL "$BASE_URL$resolved_iso.sha256" -o "$sha_path"

(
    cd "$BASE_DIR"
    sha256sum -c "$resolved_iso.sha256"
)

if [ "$UNPACK" = "1" ]; then
    unpack_iso "$iso_path" "$unpack_dir"
fi

cat > "$BASE_DIR/BASE_ISO_INFO.txt" <<EOF
track=$TRACK
arch=$ARCH
flavor=$FLAVOR
base_url=$BASE_URL
iso_name=$resolved_iso
iso_path=$iso_path
sha_path=$sha_path
unpack_dir=$unpack_dir
EOF

echo ""
echo "ISO: $iso_path"
echo "SHA: $sha_path"
if [ "$UNPACK" = "1" ]; then
    echo "Unpacked: $unpack_dir"
fi
