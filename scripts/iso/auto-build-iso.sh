#!/bin/sh
set -eu

ROOT_DIR="$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)"
YUNEXAL_RELEASE_DIR="${YUNEXAL_RELEASE_DIR:-$ROOT_DIR/yunex-release}"
YUNEXAL_PROJECT_DIR="${YUNEXAL_PROJECT_DIR:-$ROOT_DIR}"
OUT_DIR="${OUT_DIR:-$ROOT_DIR/out/images}"
ISO_BUILD_MODE="${ISO_BUILD_MODE:-auto}"
SKIP_BINARY_BUILD="${SKIP_BINARY_BUILD:-0}"

require_cmd() {
    command -v "$1" >/dev/null 2>&1 || {
        echo "Missing required command: $1" >&2
        exit 1
    }
}

resolve_mode() {
    case "$ISO_BUILD_MODE" in
        mkimage|repack)
            echo "$ISO_BUILD_MODE"
            ;;
        auto)
            if command -v apk >/dev/null 2>&1; then
                echo "mkimage"
            else
                echo "repack"
            fi
            ;;
        *)
            echo "Invalid ISO_BUILD_MODE='$ISO_BUILD_MODE' (expected: auto|mkimage|repack)" >&2
            exit 1
            ;;
    esac
}

build_release_binaries() {
    require_cmd cargo
    require_cmd rustup

    echo "Building musl release binaries..."
    (
        cd "$ROOT_DIR"
        rustup target add x86_64-unknown-linux-musl
        cargo build --release --target x86_64-unknown-linux-musl \
            --bin yunexal-panel \
            --bin yunexal-setup
    )

    mkdir -p "$YUNEXAL_RELEASE_DIR"
    cp "$ROOT_DIR/target/x86_64-unknown-linux-musl/release/yunexal-panel" "$YUNEXAL_RELEASE_DIR/yunexal-panel"
    cp "$ROOT_DIR/target/x86_64-unknown-linux-musl/release/yunexal-setup" "$YUNEXAL_RELEASE_DIR/yunexal-setup"
    chmod 0755 "$YUNEXAL_RELEASE_DIR/yunexal-panel" "$YUNEXAL_RELEASE_DIR/yunexal-setup"
    echo "Musl release binaries copied to: $YUNEXAL_RELEASE_DIR"
}

run_mkimage_pipeline() {
    echo "Building installer ISO via mkimage (Alpine path)..."
    (
        cd "$ROOT_DIR"
        YUNEXAL_RELEASE_DIR="$YUNEXAL_RELEASE_DIR" \
        YUNEXAL_PROJECT_DIR="$YUNEXAL_PROJECT_DIR" \
        OUT_DIR="$OUT_DIR" \
        sh ./scripts/iso/build-installer.sh
    )
}

run_repack_pipeline() {
    echo "Fetching base Alpine ISO..."
    (
        cd "$ROOT_DIR"
        sh ./scripts/iso/fetch-base-iso.sh
    )

    echo "Preparing custom tree and repacking ISO..."
    (
        cd "$ROOT_DIR"
        YUNEXAL_RELEASE_DIR="$YUNEXAL_RELEASE_DIR" \
        YUNEXAL_PROJECT_DIR="$YUNEXAL_PROJECT_DIR" \
        sh ./scripts/iso/prepare-custom-tree.sh
        OUT_DIR="$OUT_DIR" sh ./scripts/iso/repack-custom-iso.sh
    )
}

print_artifact_hint() {
    latest_iso="$(find "$OUT_DIR" -type f -name "*.iso" 2>/dev/null | sort | tail -n1 || true)"
    if [ -n "$latest_iso" ]; then
        echo "Done. Latest ISO: $latest_iso"
        if [ -f "$latest_iso.sha256" ]; then
            echo "Checksum: $latest_iso.sha256"
        fi
    else
        echo "Done. Check artifacts under: $OUT_DIR"
    fi
}

mode="$(resolve_mode)"

if [ "$SKIP_BINARY_BUILD" = "1" ]; then
    echo "Skipping binary build (SKIP_BINARY_BUILD=1)."
else
    build_release_binaries
fi

case "$mode" in
    mkimage)
        run_mkimage_pipeline
        ;;
    repack)
        run_repack_pipeline
        ;;
esac

print_artifact_hint
