#!/bin/sh
set -eu

ROOT_DIR="$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)"
BASE_DIR="${BASE_DIR:-$ROOT_DIR/.build/alpine-base}"
CUSTOM_DIR="${CUSTOM_DIR:-$BASE_DIR/custom-tree}"
OUT_DIR="${OUT_DIR:-$ROOT_DIR/out/images}"
ISO_LABEL="${ISO_LABEL:-YUNEXAL_ALPINE}"
ISO_NAME="${ISO_NAME:-yunexal-alpine-custom-x86_64.iso}"
OUTPUT_ISO="$OUT_DIR/$ISO_NAME"

choose_output_path() {
    if [ -e "$OUTPUT_ISO" ] && [ ! -w "$OUTPUT_ISO" ]; then
        ts="$(date +%Y%m%d-%H%M%S)"
        case "$ISO_NAME" in
            *.iso) ISO_NAME="${ISO_NAME%.iso}-$ts.iso" ;;
            *) ISO_NAME="$ISO_NAME-$ts.iso" ;;
        esac
        OUTPUT_ISO="$OUT_DIR/$ISO_NAME"
        echo "NOTICE: existing ISO is not writable, using: $OUTPUT_ISO"
    fi
}

require_cmd() {
    command -v "$1" >/dev/null 2>&1 || {
        echo "Missing required command: $1" >&2
        exit 1
    }
}

require_cmd xorriso
require_cmd sha256sum

if [ ! -d "$CUSTOM_DIR" ]; then
    echo "Custom tree not found: $CUSTOM_DIR" >&2
    echo "Run: ./scripts/iso/prepare-custom-tree.sh" >&2
    exit 1
fi

for p in \
    boot/syslinux/isohdpfx.bin \
    boot/syslinux/isolinux.bin \
    boot/syslinux/boot.cat \
    boot/grub/efi.img
do
    if [ ! -f "$CUSTOM_DIR/$p" ]; then
        echo "Missing boot asset in custom tree: $p" >&2
        exit 1
    fi
done

mkdir -p "$OUT_DIR"
choose_output_path

xorriso -as mkisofs \
    -iso-level 3 \
    -full-iso9660-filenames \
    -volid "$ISO_LABEL" \
    -output "$OUTPUT_ISO" \
    -isohybrid-mbr "$CUSTOM_DIR/boot/syslinux/isohdpfx.bin" \
    -c boot/syslinux/boot.cat \
    -b boot/syslinux/isolinux.bin \
        -no-emul-boot -boot-load-size 4 -boot-info-table \
    -eltorito-alt-boot \
    -e boot/grub/efi.img \
        -no-emul-boot \
    -isohybrid-gpt-basdat \
    -J -R \
    "$CUSTOM_DIR"

(
    cd "$OUT_DIR"
    sha256sum "$ISO_NAME" > "$ISO_NAME.sha256"
)

echo "Built ISO: $OUTPUT_ISO"
echo "Checksum: $OUTPUT_ISO.sha256"
