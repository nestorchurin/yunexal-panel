#!/bin/sh
set -eu

ROOT_DIR="$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)"
BASE_DIR="${BASE_DIR:-$ROOT_DIR/.build/alpine-base}"
UNPACK_DIR="${UNPACK_DIR:-$BASE_DIR/unpacked}"
CUSTOM_DIR="${CUSTOM_DIR:-$BASE_DIR/custom-tree}"
HOSTNAME="${YUNEXAL_HOSTNAME:-yunexal-installer}"
YUNEXAL_RELEASE_DIR="${YUNEXAL_RELEASE_DIR:-$ROOT_DIR/yunex-release}"
YUNEXAL_PROJECT_DIR="${YUNEXAL_PROJECT_DIR:-$ROOT_DIR}"
YUNEXAL_IMAGES_DIR="${YUNEXAL_IMAGES_DIR:-}"

require_cmd() {
    command -v "$1" >/dev/null 2>&1 || {
        echo "Missing required command: $1" >&2
        exit 1
    }
}

require_cmd rsync

if [ ! -d "$UNPACK_DIR" ]; then
    echo "Unpacked Alpine ISO directory not found: $UNPACK_DIR" >&2
    echo "Run: ./scripts/iso/fetch-base-iso.sh" >&2
    exit 1
fi

if [ ! -f "$YUNEXAL_RELEASE_DIR/yunexal-panel" ] || [ ! -f "$YUNEXAL_RELEASE_DIR/yunexal-setup" ]; then
    echo "Missing release binaries in: $YUNEXAL_RELEASE_DIR" >&2
    echo "Expected files: yunexal-panel and yunexal-setup" >&2
    exit 1
fi

if [ ! -f "$YUNEXAL_PROJECT_DIR/Cargo.toml" ]; then
    echo "Project root is invalid: $YUNEXAL_PROJECT_DIR (Cargo.toml not found)" >&2
    exit 1
fi

mkdir -p "$CUSTOM_DIR"
rsync -a --delete "$UNPACK_DIR/" "$CUSTOM_DIR/"

tmp_overlay_dir="$(mktemp -d)"
cleanup() {
    rm -rf "$tmp_overlay_dir"
}
trap cleanup EXIT

if [ "$(id -u)" = "0" ]; then
    (
        cd "$tmp_overlay_dir"
        YUNEXAL_RELEASE_DIR="$YUNEXAL_RELEASE_DIR" \
        YUNEXAL_PROJECT_DIR="$YUNEXAL_PROJECT_DIR" \
        YUNEXAL_IMAGES_DIR="$YUNEXAL_IMAGES_DIR" \
        "$ROOT_DIR/scripts/iso/genapkovl-yunexal.sh" "$HOSTNAME"
    )
else
    if ! command -v fakeroot >/dev/null 2>&1; then
        echo "Missing required command: fakeroot" >&2
        echo "Install fakeroot to generate apkovl as non-root, or run as root." >&2
        exit 1
    fi
    (
        cd "$tmp_overlay_dir"
        YUNEXAL_RELEASE_DIR="$YUNEXAL_RELEASE_DIR" \
        YUNEXAL_PROJECT_DIR="$YUNEXAL_PROJECT_DIR" \
        YUNEXAL_IMAGES_DIR="$YUNEXAL_IMAGES_DIR" \
        fakeroot "$ROOT_DIR/scripts/iso/genapkovl-yunexal.sh" "$HOSTNAME"
    )
fi

overlay_name="$HOSTNAME.apkovl.tar.gz"
overlay_path="$tmp_overlay_dir/$overlay_name"
if [ ! -f "$overlay_path" ]; then
    echo "Failed to generate overlay: $overlay_name" >&2
    exit 1
fi

cp "$overlay_path" "$CUSTOM_DIR/$overlay_name"

# ── Patch boot menu labels ─────────────────────────────────────────────────
# console=tty1 pins the active VT so QEMU shows the installer on the VGA
# screen without needing a VT switch.

cat > "$CUSTOM_DIR/boot/grub/grub.cfg" <<'EOF'
set timeout=1

menuentry "Yunexal Musl Panel Installer" {
linux	/boot/vmlinuz-lts modules=loop,squashfs,sd-mod,usb-storage quiet console=tty1
initrd	/boot/initramfs-lts
}
EOF

cat > "$CUSTOM_DIR/boot/syslinux/syslinux.cfg" <<'EOF'
TIMEOUT 10
PROMPT 1
DEFAULT yunexal

LABEL yunexal
MENU LABEL Yunexal Musl Panel Installer
KERNEL /boot/vmlinuz-lts
INITRD /boot/initramfs-lts
FDTDIR /boot/dtbs-lts
APPEND modules=loop,squashfs,sd-mod,usb-storage quiet console=tty1
EOF

# nlplug-findfs auto-discovers *.apkovl.tar.gz from boot media at init time —
# do not add apkovl= to kernel cmdline; that path causes prepare_apkovl() to
# set ovl to a bare filename which fails the [ -f "$ovl" ] check in init.

cat > "$CUSTOM_DIR/YUNEXAL_BUILD_INFO.txt" <<EOF
base_unpack_dir=$UNPACK_DIR
custom_tree_dir=$CUSTOM_DIR
overlay=$overlay_name
hostname=$HOSTNAME
release_dir=$YUNEXAL_RELEASE_DIR
project_dir=$YUNEXAL_PROJECT_DIR
EOF

echo "Prepared custom tree: $CUSTOM_DIR"
echo "Injected overlay: $overlay_name"
