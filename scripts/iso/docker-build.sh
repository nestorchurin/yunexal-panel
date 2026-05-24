#!/bin/sh
# Build the Yunexal Alpine ISO using Docker (no local Rust or Alpine required).
# Requires: Docker 18.09+ with BuildKit support.
#
# Usage:
#   ./scripts/iso/docker-build.sh [--out DIR]
#
# Output: out/images/yunexal-alpine-custom-x86_64.iso (+ .sha256)
set -eu

SCRIPT_DIR="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
ROOT_DIR="$(CDPATH= cd -- "$SCRIPT_DIR/../.." && pwd)"
OUT_DIR="${OUT_DIR:-$ROOT_DIR/out/images}"
DOCKER_IMAGES_DIR="$ROOT_DIR/out/docker-images"

while [ $# -gt 0 ]; do
    case "$1" in
        --out)
            [ $# -ge 2 ] || { echo "--out requires a value" >&2; exit 1; }
            OUT_DIR="$2"; shift 2 ;;
        *) echo "Unknown argument: $1" >&2; exit 1 ;;
    esac
done

export DOCKER_BUILDKIT=1

mkdir -p "$OUT_DIR" "$DOCKER_IMAGES_DIR"
cd "$ROOT_DIR"

echo "==> Pulling alpine:latest (helper image for yunexal-panel)..."
docker pull alpine:latest
docker save alpine:latest -o "$DOCKER_IMAGES_DIR/alpine.tar"
echo "    Saved: $DOCKER_IMAGES_DIR/alpine.tar"
echo ""

echo "==> Building Yunexal Alpine ISO (Docker)..."
echo "    Output: $OUT_DIR"
echo ""

docker build \
    -f scripts/iso/Dockerfile.iso \
    --target artifact \
    --output "type=local,dest=$OUT_DIR" \
    --build-context docker-images="$DOCKER_IMAGES_DIR" \
    .

echo ""
echo "Done."
ls -lh "$OUT_DIR"/*.iso 2>/dev/null || true
