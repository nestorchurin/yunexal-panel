#!/bin/sh
# Creates a Docker-loadable image tarball from a rootfs directory.
# No Docker daemon required — pure shell + tar + sha256sum.
#
# Usage: make-image-tar.sh <rootfs-dir> <image:tag> <output.tar>
set -eu

ROOTFS_DIR="$1"
IMAGE_TAG="$2"
OUTPUT_TAR="$3"

WORK="$(mktemp -d)"
cleanup() { rm -rf "$WORK"; }
trap cleanup EXIT

# Layer tarball from rootfs
tar -C "$ROOTFS_DIR" -cf "$WORK/layer.tar" .
LAYER_SHA="$(sha256sum "$WORK/layer.tar" | awk '{print $1}')"
mkdir -p "$WORK/$LAYER_SHA"
mv "$WORK/layer.tar" "$WORK/$LAYER_SHA/layer.tar"

CREATED="$(date -u '+%Y-%m-%dT%H:%M:%SZ')"

# Image config (OCI / Docker v2 schema 2 compatible)
cat > "$WORK/config.json" <<CONFIG
{
  "architecture": "amd64",
  "os": "linux",
  "created": "${CREATED}",
  "config": {
    "Env": ["PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin"],
    "Entrypoint": ["/usr/local/bin/yunexal-panel"],
    "ExposedPorts": {"3000/tcp": {}}
  },
  "rootfs": {
    "type": "layers",
    "diff_ids": ["sha256:${LAYER_SHA}"]
  }
}
CONFIG

CONFIG_SHA="$(sha256sum "$WORK/config.json" | awk '{print $1}')"
mv "$WORK/config.json" "$WORK/${CONFIG_SHA}.json"

cat > "$WORK/manifest.json" <<MANIFEST
[{
  "Config": "${CONFIG_SHA}.json",
  "RepoTags": ["${IMAGE_TAG}"],
  "Layers": ["${LAYER_SHA}/layer.tar"]
}]
MANIFEST

mkdir -p "$(dirname "$OUTPUT_TAR")"
tar -C "$WORK" -cf "$OUTPUT_TAR" "${CONFIG_SHA}.json" "manifest.json" "$LAYER_SHA/"

echo "Docker image tar: ${IMAGE_TAG} -> ${OUTPUT_TAR}"
