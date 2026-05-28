#!/bin/sh
set -eu

ROOT_DIR="$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)"
RELEASE_TAG="${RELEASE_TAG:-v1.0.0}"
RELEASE_COMMIT="${RELEASE_COMMIT:-f694e16}"
INPUT_DIR="${INPUT_DIR:-$ROOT_DIR/yunex-release}"
OUT_DIR="${OUT_DIR:-$ROOT_DIR/out/github-release/$RELEASE_TAG}"

require_cmd() {
    command -v "$1" >/dev/null 2>&1 || {
        echo "Missing required command: $1" >&2
        exit 1
    }
}

require_cmd git
require_cmd tar
require_cmd cp
require_cmd chmod

panel_bin="$INPUT_DIR/yunexal-panel"
setup_bin="$INPUT_DIR/yunexal-setup"

if [ ! -f "$panel_bin" ] || [ ! -f "$setup_bin" ]; then
    echo "Missing release binaries in $INPUT_DIR" >&2
    echo "Expected: $panel_bin and $setup_bin" >&2
    exit 1
fi

mkdir -p "$OUT_DIR"

cp "$panel_bin" "$OUT_DIR/yunexal-panel.bin"
cp "$setup_bin" "$OUT_DIR/yunexal-setup.bin"
chmod 0755 "$OUT_DIR/yunexal-panel.bin" "$OUT_DIR/yunexal-setup.bin"

git -C "$ROOT_DIR" archive \
    --format=tar.gz \
    --prefix="yunexal-panel-${RELEASE_TAG#v}-source/" \
    "$RELEASE_COMMIT" \
    > "$OUT_DIR/yunexal-panel-${RELEASE_TAG#v}-source.tar.gz"

cat > "$OUT_DIR/README.txt" <<EOF
Yunexal Panel GitHub Release bundle

Release tag: $RELEASE_TAG
Source commit: $RELEASE_COMMIT

Assets:
- yunexal-panel.bin
- yunexal-setup.bin
- yunexal-panel-${RELEASE_TAG#v}-source.tar.gz

Upload these files to the GitHub Release page for $RELEASE_TAG.
EOF

echo "Prepared GitHub release bundle in: $OUT_DIR"