#!/bin/sh -e

HOSTNAME="$1"
if [ -z "$HOSTNAME" ]; then
    echo "usage: $0 hostname" >&2
    exit 1
fi

if [ -z "${YUNEXAL_RELEASE_DIR:-}" ]; then
    echo "YUNEXAL_RELEASE_DIR is required" >&2
    exit 1
fi

YUNEXAL_PROJECT_DIR="${YUNEXAL_PROJECT_DIR:-$(dirname "$YUNEXAL_RELEASE_DIR")}"
YUNEXAL_IMAGES_DIR="${YUNEXAL_IMAGES_DIR:-}"

PANEL_BIN="$YUNEXAL_RELEASE_DIR/yunexal-panel"
SETUP_BIN="$YUNEXAL_RELEASE_DIR/yunexal-setup"

if [ ! -f "$PANEL_BIN" ] || [ ! -f "$SETUP_BIN" ]; then
    echo "Missing Yunexal binaries in YUNEXAL_RELEASE_DIR=$YUNEXAL_RELEASE_DIR" >&2
    exit 1
fi

if [ ! -f "$YUNEXAL_PROJECT_DIR/Cargo.toml" ]; then
    echo "Missing project sources in YUNEXAL_PROJECT_DIR=$YUNEXAL_PROJECT_DIR" >&2
    echo "Expected file: Cargo.toml" >&2
    exit 1
fi

cleanup() {
    rm -rf "$tmp"
}

makefile() {
    OWNER="$1"
    PERMS="$2"
    FILENAME="$3"
    cat > "$FILENAME"
    chown "$OWNER" "$FILENAME"
    chmod "$PERMS" "$FILENAME"
}

rc_add() {
    mkdir -p "$tmp"/etc/runlevels/"$2"
    ln -sf /etc/init.d/"$1" "$tmp"/etc/runlevels/"$2"/"$1"
}

tmp="$(mktemp -d)"
trap cleanup EXIT

mkdir -p "$tmp"/etc
makefile root:root 0644 "$tmp"/etc/hostname <<EOF
$HOSTNAME
EOF

makefile root:root 0644 "$tmp"/etc/issue <<'EOF'
Yunexal Musl Panel Installer
EOF

makefile root:root 0644 "$tmp"/etc/os-release <<'EOF'
NAME="Yunexal Musl Panel"
VERSION="0.5.1"
ID=yunexal
ID_LIKE=alpine
PRETTY_NAME="Yunexal Musl Panel 0.5.1"
EOF

mkdir -p "$tmp"/etc/network
makefile root:root 0644 "$tmp"/etc/network/interfaces <<'EOF'
auto lo
iface lo inet loopback

auto eth0
iface eth0 inet dhcp
EOF

mkdir -p "$tmp"/etc/modprobe.d
makefile root:root 0644 "$tmp"/etc/modprobe.d/yunexal-server-only.conf <<'EOF'
# Yunexal server image: only drivers required for a headless server are kept.
# Everything desktop/consumer-specific is disabled here.

# ── Wireless ──────────────────────────────────────────────────────────────────
blacklist cfg80211
blacklist mac80211
blacklist rfkill
install cfg80211 /bin/false
install mac80211 /bin/false
install rfkill /bin/false

# ── Bluetooth ─────────────────────────────────────────────────────────────────
blacklist bluetooth
blacklist btusb
blacklist bnep
blacklist rfcomm
blacklist hidp
blacklist btbcm
blacklist btintel
blacklist btrtl
blacklist btqca
blacklist btmtksdio
blacklist btsdio
install bluetooth /bin/false
install btusb /bin/false

# ── Audio / Sound ─────────────────────────────────────────────────────────────
blacklist snd
blacklist snd_pcm
blacklist snd_timer
blacklist snd_hda_intel
blacklist snd_hda_codec
blacklist snd_hda_codec_generic
blacklist snd_hda_codec_realtek
blacklist snd_hda_codec_analog
blacklist snd_hda_codec_idt
blacklist snd_hda_codec_via
blacklist snd_hda_core
blacklist snd_hwdep
blacklist snd_ac97_codec
blacklist snd_rawmidi
blacklist snd_seq
blacklist snd_seq_device
blacklist snd_seq_midi
blacklist snd_seq_midi_event
blacklist snd_seq_oss
blacklist snd_pcm_oss
blacklist snd_mixer_oss
blacklist snd_soc_core
blacklist snd_compress
blacklist ac97_bus
blacklist soundcore
install snd /bin/false
install snd_hda_intel /bin/false

# ── GPU / DRM (heavy display drivers; vesafb/efifb sufficient for console) ────
# cirrus is blacklist-only so QEMU's Cirrus VGA emulation works during install.
blacklist nouveau
blacklist radeon
blacklist amdgpu
blacklist i915
blacklist ast
blacklist mgag200
blacklist cirrus
install nouveau /bin/false
install radeon /bin/false
install amdgpu /bin/false
install i915 /bin/false

# ── Webcam / Camera ───────────────────────────────────────────────────────────
blacklist uvcvideo
blacklist gspca_main
install uvcvideo /bin/false

# ── TV / DVB / Media capture ──────────────────────────────────────────────────
blacklist dvb_core
blacklist media
blacklist videodev
blacklist v4l2_common
blacklist tveeprom
install dvb_core /bin/false
install videodev /bin/false

# ── Joystick / Gamepad ────────────────────────────────────────────────────────
blacklist joydev
blacklist gameport
install joydev /bin/false

# ── PCMCIA ────────────────────────────────────────────────────────────────────
blacklist pcmcia
blacklist pcmcia_core
blacklist yenta_socket
blacklist rsrc_nonstatic
install pcmcia /bin/false

# ── FireWire / IEEE 1394 ──────────────────────────────────────────────────────
blacklist firewire_core
blacklist firewire_ohci
blacklist firewire_sbp2
blacklist firewire_net
install firewire_core /bin/false

# ── NFC ───────────────────────────────────────────────────────────────────────
blacklist nfc
install nfc /bin/false

# ── CEC (HDMI ARC / consumer electronics control) ────────────────────────────
blacklist cec
install cec /bin/false

# ── Floppy ────────────────────────────────────────────────────────────────────
blacklist floppy
install floppy /bin/false

# ── Infrared ──────────────────────────────────────────────────────────────────
blacklist ite_cir
blacklist mceusb
blacklist nuvoton_cir
blacklist rc_core
install rc_core /bin/false

# ── Amateur radio protocols ───────────────────────────────────────────────────
blacklist ax25
blacklist netrom
blacklist rose
install ax25 /bin/false

# ── Printer ───────────────────────────────────────────────────────────────────
blacklist lp
blacklist usblp
blacklist ppdev
blacklist parport
blacklist parport_pc
install lp /bin/false
install usblp /bin/false
install ppdev /bin/false
install parport /bin/false
install parport_pc /bin/false
EOF

mkdir -p "$tmp"/etc/apk
makefile root:root 0644 "$tmp"/etc/apk/world <<'EOF'
alpine-base
alpine-conf
docker
docker-cli-compose
nginx
sudo
curl
bash
ca-certificates
util-linux
e2fsprogs
xfsprogs
btrfs-progs
gptfdisk
dosfstools
EOF

mkdir -p "$tmp"/opt/yunexal/bin
cp "$PANEL_BIN" "$tmp"/opt/yunexal/bin/yunexal-panel
cp "$SETUP_BIN" "$tmp"/opt/yunexal/bin/yunexal-setup
chmod 0755 "$tmp"/opt/yunexal/bin/yunexal-panel "$tmp"/opt/yunexal/bin/yunexal-setup

mkdir -p "$tmp"/opt/yunexal/project
for entry in \
    Cargo.toml \
    Cargo.lock \
    README.md \
    LICENSE \
    CONTRIBUTING.md \
    CONTRIBUTORS.md \
    CHANGELOGS \
    scripts \
    src \
    static \
    templates
do
    if [ -e "$YUNEXAL_PROJECT_DIR/$entry" ]; then
        cp -a "$YUNEXAL_PROJECT_DIR/$entry" "$tmp"/opt/yunexal/project/
    fi
done

mkdir -p "$tmp"/usr/local/bin
makefile root:root 0755 "$tmp"/usr/local/bin/yunexal-panel <<'EOF'
#!/bin/sh
set -eu
mkdir -p /var/lib/yunexal/panel
cd /var/lib/yunexal/panel
exec /opt/yunexal/bin/yunexal-panel "$@"
EOF

makefile root:root 0755 "$tmp"/usr/local/bin/yunexal-setup <<'EOF'
#!/bin/sh
set -eu

# In live installer image, always route branded command to installer entrypoint.
if [ -x /usr/local/sbin/yunexal-setup ] && [ -f /etc/alpine-release ]; then
    exec /usr/local/sbin/yunexal-setup "$@"
fi

mkdir -p /var/lib/yunexal/panel
cd /var/lib/yunexal/panel
exec /opt/yunexal/bin/yunexal-setup "$@"
EOF

makefile root:root 0755 "$tmp"/usr/local/bin/yunexal-install <<'EOF'
#!/bin/sh
set -eu

ACTION="prepare"
DISK=""
MODE="safe"
ROOT_SIZE_GIB="${ROOT_SIZE_GIB:-40}"
TARGET_ROOT="/mnt"
DRY_RUN=0
YES=0

usage() {
    cat <<USAGE
Usage:
  yunexal-install prepare --disk /dev/sdX [--mode safe|force] [--root-size-gib 40] [--yes] [--dry-run]
  yunexal-install finalize --disk /dev/sdX [--target-root /mnt]

Actions:
  prepare   Create GPT layout for Yunexal installer:
            p1 EFI   (512MiB, FAT32)
            p2 SYS   (ext4, install target)
            p3 DATA  (ext4 + project quota support)

      finalize  Prepare installed Alpine target:
            - add persistent DATA partition mount in target fstab
            - install Yunexal runtime + full project into target root
            - enable background autostart after reboot
            - print first-boot setup instructions

Safety:
  --mode safe (default) blocks accidental operations on the currently running root disk.
  --mode force bypasses those checks.
USAGE
}

die() {
    echo "ERROR: $*" >&2
    exit 1
}

run() {
    if [ "$DRY_RUN" = "1" ]; then
        echo "+ $*"
        return 0
    fi
    "$@"
}

require_cmd() {
    command -v "$1" >/dev/null 2>&1 || die "missing command: $1"
}

write_target_file() {
    rel_path="$1"
    perms="$2"
    target_file="$TARGET_ROOT/$rel_path"
    target_dir="$(dirname "$target_file")"

    run mkdir -p "$target_dir"

    if [ "$DRY_RUN" = "1" ]; then
        cat >/dev/null
        echo "+ write $target_file"
        return 0
    fi

    cat > "$target_file"
    chmod "$perms" "$target_file"
}

ensure_target_root() {
    [ "$TARGET_ROOT" != "/" ] || die "target root must not be /"
    [ -d "$TARGET_ROOT/etc" ] || die "target root '$TARGET_ROOT' is not mounted"
    [ -f "$TARGET_ROOT/etc/fstab" ] || die "missing fstab in '$TARGET_ROOT/etc/fstab'"

    if [ "$DRY_RUN" != "1" ] && command -v findmnt >/dev/null 2>&1; then
        findmnt -n "$TARGET_ROOT" >/dev/null 2>&1 || die "target root '$TARGET_ROOT' is not a mount point"
    fi
}

install_target_payload() {
    src_panel="/opt/yunexal/bin/yunexal-panel"
    src_setup="/opt/yunexal/bin/yunexal-setup"
    src_project="/opt/yunexal/project"

    [ -f "$src_panel" ] || die "missing source binary: $src_panel"
    [ -f "$src_setup" ] || die "missing source binary: $src_setup"
    [ -d "$src_project" ] || die "missing project source tree: $src_project"

    run mkdir -p "$TARGET_ROOT/opt/yunexal/bin"
    run mkdir -p "$TARGET_ROOT/opt/yunexal"
    run cp "$src_panel" "$TARGET_ROOT/opt/yunexal/bin/yunexal-panel"
    run cp "$src_setup" "$TARGET_ROOT/opt/yunexal/bin/yunexal-setup"
    run chmod 0755 "$TARGET_ROOT/opt/yunexal/bin/yunexal-panel" "$TARGET_ROOT/opt/yunexal/bin/yunexal-setup"
    run rm -rf "$TARGET_ROOT/opt/yunexal/project"
    run cp -a "$src_project" "$TARGET_ROOT/opt/yunexal/project"

    write_target_file "usr/local/bin/yunexal-panel" 0755 <<'EOF_WRAPPER_PANEL'
#!/bin/sh
set -eu
mkdir -p /var/lib/yunexal/panel
cd /var/lib/yunexal/panel
exec /opt/yunexal/bin/yunexal-panel "$@"
EOF_WRAPPER_PANEL

    write_target_file "usr/local/bin/yunexal-setup" 0755 <<'EOF_WRAPPER_SETUP'
#!/bin/sh
set -eu
mkdir -p /var/lib/yunexal/panel
cd /var/lib/yunexal/panel
exec /opt/yunexal/bin/yunexal-setup "$@"
EOF_WRAPPER_SETUP

    run mkdir -p "$TARGET_ROOT/var/lib/yunexal/panel"

    if [ "$DRY_RUN" = "1" ]; then
        echo "+ ensure $TARGET_ROOT/var/lib/yunexal/panel/.env"
    elif [ ! -f "$TARGET_ROOT/var/lib/yunexal/panel/.env" ]; then
        cat > "$TARGET_ROOT/var/lib/yunexal/panel/.env" <<'EOF_TARGET_ENV'
# Generated by yunexal-install finalize on Alpine installer image.
# Re-run yunexal-setup after first boot to rotate COOKIE_SECRET.
PANEL_PORT=3000
COOKIE_SECRET=CHANGE_ME_WITH_YUNEXAL_SETUP
DATABASE_URL=sqlite:yunexal.db
EOF_TARGET_ENV
        chmod 0600 "$TARGET_ROOT/var/lib/yunexal/panel/.env"
    fi

    write_target_file "etc/modprobe.d/yunexal-server-only.conf" 0644 <<'EOF_TARGET_MODPROBE'
# Yunexal server image: only drivers required for a headless server are kept.

# ── Wireless ──────────────────────────────────────────────────────────────────
blacklist cfg80211
blacklist mac80211
blacklist rfkill
install cfg80211 /bin/false
install mac80211 /bin/false
install rfkill /bin/false

# ── Bluetooth ─────────────────────────────────────────────────────────────────
blacklist bluetooth
blacklist btusb
blacklist bnep
blacklist rfcomm
blacklist hidp
blacklist btbcm
blacklist btintel
blacklist btrtl
blacklist btqca
blacklist btmtksdio
blacklist btsdio
install bluetooth /bin/false
install btusb /bin/false

# ── Audio / Sound ─────────────────────────────────────────────────────────────
blacklist snd
blacklist snd_pcm
blacklist snd_timer
blacklist snd_hda_intel
blacklist snd_hda_codec
blacklist snd_hda_codec_generic
blacklist snd_hda_codec_realtek
blacklist snd_hda_codec_analog
blacklist snd_hda_codec_idt
blacklist snd_hda_codec_via
blacklist snd_hda_core
blacklist snd_hwdep
blacklist snd_ac97_codec
blacklist snd_rawmidi
blacklist snd_seq
blacklist snd_seq_device
blacklist snd_seq_midi
blacklist snd_seq_midi_event
blacklist snd_seq_oss
blacklist snd_pcm_oss
blacklist snd_mixer_oss
blacklist snd_soc_core
blacklist snd_compress
blacklist ac97_bus
blacklist soundcore
install snd /bin/false
install snd_hda_intel /bin/false

# ── GPU / DRM (heavy display drivers; vesafb/efifb sufficient for console) ────
# cirrus is blacklist-only so QEMU's Cirrus VGA emulation works during install.
blacklist nouveau
blacklist radeon
blacklist amdgpu
blacklist i915
blacklist ast
blacklist mgag200
blacklist cirrus
install nouveau /bin/false
install radeon /bin/false
install amdgpu /bin/false
install i915 /bin/false

# ── Webcam / Camera ───────────────────────────────────────────────────────────
blacklist uvcvideo
blacklist gspca_main
install uvcvideo /bin/false

# ── TV / DVB / Media capture ──────────────────────────────────────────────────
blacklist dvb_core
blacklist media
blacklist videodev
blacklist v4l2_common
blacklist tveeprom
install dvb_core /bin/false
install videodev /bin/false

# ── Joystick / Gamepad ────────────────────────────────────────────────────────
blacklist joydev
blacklist gameport
install joydev /bin/false

# ── PCMCIA ────────────────────────────────────────────────────────────────────
blacklist pcmcia
blacklist pcmcia_core
blacklist yenta_socket
blacklist rsrc_nonstatic
install pcmcia /bin/false

# ── FireWire / IEEE 1394 ──────────────────────────────────────────────────────
blacklist firewire_core
blacklist firewire_ohci
blacklist firewire_sbp2
blacklist firewire_net
install firewire_core /bin/false

# ── NFC ───────────────────────────────────────────────────────────────────────
blacklist nfc
install nfc /bin/false

# ── CEC (HDMI ARC / consumer electronics control) ────────────────────────────
blacklist cec
install cec /bin/false

# ── Floppy ────────────────────────────────────────────────────────────────────
blacklist floppy
install floppy /bin/false

# ── Infrared ──────────────────────────────────────────────────────────────────
blacklist ite_cir
blacklist mceusb
blacklist nuvoton_cir
blacklist rc_core
install rc_core /bin/false

# ── Amateur radio protocols ───────────────────────────────────────────────────
blacklist ax25
blacklist netrom
blacklist rose
install ax25 /bin/false

# ── Printer ───────────────────────────────────────────────────────────────────
blacklist lp
blacklist usblp
blacklist ppdev
blacklist parport
blacklist parport_pc
install lp /bin/false
install usblp /bin/false
install ppdev /bin/false
install parport /bin/false
install parport_pc /bin/false
EOF_TARGET_MODPROBE

    write_target_file "etc/init.d/yunexal-panel" 0755 <<'EOF_TARGET_SERVICE'
#!/sbin/openrc-run
name="yunexal-panel"
description="Yunexal Musl Panel"

command="/usr/local/bin/yunexal-panel"
directory="/var/lib/yunexal/panel"
pidfile="/run/yunexal-panel.pid"
command_background="yes"
start_stop_daemon_args="--make-pidfile --pidfile ${pidfile} --stdout /var/log/yunexal-panel.log --stderr /var/log/yunexal-panel.log"

depend() {
    need net docker
    after firewall
}

start_pre() {
    checkpath --directory --owner root:root --mode 0755 /run
    checkpath --file --owner root:root --mode 0644 /var/log/yunexal-panel.log
}
EOF_TARGET_SERVICE

    run mkdir -p "$TARGET_ROOT/var/log"
    if [ "$DRY_RUN" = "1" ]; then
        echo "+ touch $TARGET_ROOT/var/log/yunexal-panel.log"
    else
        touch "$TARGET_ROOT/var/log/yunexal-panel.log"
        chmod 0644 "$TARGET_ROOT/var/log/yunexal-panel.log"
    fi

    if [ -d "/opt/yunexal/images" ] && ls /opt/yunexal/images/*.tar >/dev/null 2>&1; then
        run mkdir -p "$TARGET_ROOT/opt/yunexal/images"
        for _img in /opt/yunexal/images/*.tar; do
            [ -f "$_img" ] || continue
            _name="$(basename "$_img")"
            if [ "$DRY_RUN" = "1" ]; then
                echo "+ cp $_img $TARGET_ROOT/opt/yunexal/images/$_name"
            else
                cp "$_img" "$TARGET_ROOT/opt/yunexal/images/$_name"
            fi
        done

        write_target_file "etc/local.d/yunexal-load-images.start" 0755 <<'EOF_LOAD_IMAGES'
#!/bin/sh
IMAGES_DIR="/opt/yunexal/images"
FLAG="/opt/yunexal/.images-loaded"
[ -f "$FLAG" ] && exit 0
for img in "$IMAGES_DIR"/*.tar; do
    [ -f "$img" ] || continue
    echo "Loading Docker image: $(basename "$img")"
    docker load < "$img" && echo "Loaded: $(basename "$img")" || echo "WARN: failed to load $(basename "$img")"
done
touch "$FLAG"
EOF_LOAD_IMAGES
    fi
}

enable_target_service() {
    service_name="$1"
    init_path="$TARGET_ROOT/etc/init.d/$service_name"
    runlevel_dir="$TARGET_ROOT/etc/runlevels/default"
    runlevel_link="$runlevel_dir/$service_name"

    if [ ! -f "$init_path" ]; then
        echo "WARN: target service '$service_name' is missing at $init_path (skip enable)" >&2
        return 0
    fi

    run mkdir -p "$runlevel_dir"

    if [ "$DRY_RUN" = "1" ]; then
        echo "+ ln -sf /etc/init.d/$service_name $runlevel_link"
    else
        ln -sf "/etc/init.d/$service_name" "$runlevel_link"
        echo "Enabled target service: $service_name"
    fi
}

part_path() {
    d="$1"
    n="$2"
    case "$d" in
        *[0-9]) echo "${d}p${n}" ;;
        *) echo "${d}${n}" ;;
    esac
}

wait_for_block() {
    p="$1"
    i=0
    while [ "$i" -lt 20 ]; do
        if [ -b "$p" ]; then
            return 0
        fi
        i=$((i + 1))
        sleep 1
    done
    return 1
}

ensure_safe_disk() {
    [ "$MODE" = "safe" ] || return 0

    mounted="$(lsblk -nrpo NAME,MOUNTPOINT "$DISK" | awk '$2 != "" {print $0}')"
    if [ -n "$mounted" ]; then
        die "disk '$DISK' has mounted partitions; refusing in safe mode"
    fi

    root_src="$(findmnt -n -o SOURCE / 2>/dev/null || true)"
    if [ -n "$root_src" ] && [ "${root_src#/dev/}" != "$root_src" ]; then
        root_parent="$(lsblk -no PKNAME "$root_src" 2>/dev/null | head -n1 | tr -d '[:space:]')"
        disk_base="$(basename "$DISK")"
        root_base="$(basename "$root_src")"
        if [ "$disk_base" = "$root_base" ] || { [ -n "$root_parent" ] && [ "$disk_base" = "$root_parent" ]; }; then
            die "disk '$DISK' appears to be current root disk ($root_src); use another disk or --mode force"
        fi
    fi
}

prepare_disk() {
    [ -b "$DISK" ] || die "disk '$DISK' is not a block device"

    case "$ROOT_SIZE_GIB" in
        ''|*[!0-9]*) die "--root-size-gib must be an integer" ;;
    esac
    [ "$ROOT_SIZE_GIB" -ge 8 ] || die "--root-size-gib must be >= 8"

    require_cmd lsblk
    require_cmd findmnt
    require_cmd wipefs
    require_cmd sgdisk
    require_cmd mkfs.vfat
    require_cmd mkfs.ext4
    require_cmd partprobe

    ensure_safe_disk

    if [ "$YES" != "1" ]; then
        [ -t 0 ] || die "non-interactive mode requires --yes"
        echo "This will ERASE ALL DATA on $DISK"
        printf "Type YES to continue: "
        read -r confirm
        [ "$confirm" = "YES" ] || die "aborted"
    fi

    p1="$(part_path "$DISK" 1)"
    p2="$(part_path "$DISK" 2)"
    p3="$(part_path "$DISK" 3)"

    run wipefs -af "$DISK"
    run sgdisk --zap-all "$DISK"
    run sgdisk -n 1:1MiB:+512MiB -t 1:ef00 -c 1:YUNEXAL_EFI "$DISK"
    run sgdisk -n "2:0:+${ROOT_SIZE_GIB}GiB" -t 2:8300 -c 2:YUNEXAL_SYS "$DISK"
    run sgdisk -n 3:0:0 -t 3:8300 -c 3:YUNEXAL_DATA "$DISK"
    run partprobe "$DISK"

    if [ "$DRY_RUN" != "1" ]; then
        if command -v udevadm >/dev/null 2>&1; then
            run udevadm settle
        fi
        wait_for_block "$p1" || die "partition not found: $p1"
        wait_for_block "$p2" || die "partition not found: $p2"
        wait_for_block "$p3" || die "partition not found: $p3"
    fi

    run mkfs.vfat -F 32 "$p1"
    run mkfs.ext4 -F "$p2"
    run mkfs.ext4 -F -O project "$p3"

    echo ""
    echo "Disk prepared:"
    echo "  EFI : $p1 (FAT32)"
    echo "  SYS : $p2 (ext4)"
    echo "  DATA: $p3 (ext4 project quotas)"
    echo ""
    echo "Next steps:"
    echo "  1) Run yunexal-setup and install system to $p2"
    echo "  2) Ensure target root is mounted at $TARGET_ROOT"
    echo "  3) Run: yunexal-install finalize --disk $DISK --target-root $TARGET_ROOT"
}

finalize_target() {
    [ -b "$DISK" ] || die "disk '$DISK' is not a block device"
    p3="$(part_path "$DISK" 3)"
    [ -b "$p3" ] || die "data partition not found: $p3"

    ensure_target_root

    require_cmd blkid

    uuid="$(blkid -s UUID -o value "$p3" 2>/dev/null || true)"
    [ -n "$uuid" ] || die "failed to read UUID for $p3"

    run mkdir -p "$TARGET_ROOT/var/lib/yunexal/volumes"

    entry="UUID=$uuid /var/lib/yunexal/volumes ext4 defaults,prjquota 0 2"
    if grep -Fq "UUID=$uuid /var/lib/yunexal/volumes " "$TARGET_ROOT/etc/fstab"; then
        echo "fstab entry already present for DATA partition"
    else
        if [ "$DRY_RUN" = "1" ]; then
            echo "+ append to $TARGET_ROOT/etc/fstab: $entry"
        else
            echo "$entry" >> "$TARGET_ROOT/etc/fstab"
        fi
        echo "Added DATA mount entry to $TARGET_ROOT/etc/fstab"
    fi

    install_target_payload
    enable_target_service docker
    enable_target_service local
    enable_target_service yunexal-panel

    echo ""
    echo "Finalize complete for target: $TARGET_ROOT"
    echo "Next steps:"
    echo "  1) Reboot into installed system"
    echo "  2) Yunexal Musl Panel starts automatically in background"
    echo "  3) Project sources are available at /opt/yunexal/project"
    echo "  4) Run yunexal-setup to configure admin user and rotate secrets"
}

if [ $# -gt 0 ]; then
    case "$1" in
        prepare|finalize)
            ACTION="$1"
            shift
            ;;
        help|-h|--help)
            usage
            exit 0
            ;;
    esac
fi

while [ $# -gt 0 ]; do
    case "$1" in
        --disk)
            [ $# -ge 2 ] || die "--disk requires value"
            DISK="$2"
            shift 2
            ;;
        --mode)
            [ $# -ge 2 ] || die "--mode requires value"
            MODE="$2"
            shift 2
            ;;
        --root-size-gib)
            [ $# -ge 2 ] || die "--root-size-gib requires value"
            ROOT_SIZE_GIB="$2"
            shift 2
            ;;
        --target-root)
            [ $# -ge 2 ] || die "--target-root requires value"
            TARGET_ROOT="$2"
            shift 2
            ;;
        --dry-run)
            DRY_RUN=1
            shift
            ;;
        --yes|-y)
            YES=1
            shift
            ;;
        help|-h|--help)
            usage
            exit 0
            ;;
        *)
            die "unknown argument: $1"
            ;;
    esac
done

case "$MODE" in
    safe|force) ;;
    *) die "--mode must be safe or force" ;;
esac

[ -n "$DISK" ] || die "--disk is required"

case "$ACTION" in
    prepare)
        prepare_disk
        ;;
    finalize)
        finalize_target
        ;;
    *)
        die "unsupported action: $ACTION"
        ;;
esac
EOF

makefile root:root 0755 "$tmp"/usr/local/bin/yumexal-install <<'EOF'
#!/bin/sh
set -eu
exec /usr/local/bin/yunexal-install "$@"
EOF

mkdir -p "$tmp"/usr/local/sbin
makefile root:root 0755 "$tmp"/usr/local/sbin/yunexal-setup <<'EOF'
#!/bin/sh
set -eu

BASE_INSTALLER_CMD="/sbin/setup-alpine"
TARGET_ROOT="${YUNEXAL_TARGET_ROOT:-/mnt}"

if [ ! -x "$BASE_INSTALLER_CMD" ]; then
    echo "ERROR: base Alpine installer not found at $BASE_INSTALLER_CMD" >&2
    exit 1
fi

"$BASE_INSTALLER_CMD" "$@"
rc=$?
if [ "$rc" -ne 0 ]; then
    exit "$rc"
fi

if [ ! -d "$TARGET_ROOT/etc" ] || [ ! -f "$TARGET_ROOT/etc/fstab" ]; then
    echo "Yunexal auto-finalize skipped: target root not found at $TARGET_ROOT" >&2
    echo "Run manually: yunexal-install finalize --disk /dev/sdX --target-root $TARGET_ROOT" >&2
    exit 0
fi

if ! command -v findmnt >/dev/null 2>&1; then
    echo "Yunexal auto-finalize skipped: findmnt not available" >&2
    echo "Run manually: yunexal-install finalize --disk /dev/sdX --target-root $TARGET_ROOT" >&2
    exit 0
fi

sys_part="$(findmnt -n -o SOURCE "$TARGET_ROOT" 2>/dev/null || true)"
if [ -z "$sys_part" ]; then
    echo "Yunexal auto-finalize skipped: cannot resolve source for $TARGET_ROOT" >&2
    echo "Run manually: yunexal-install finalize --disk /dev/sdX --target-root $TARGET_ROOT" >&2
    exit 0
fi

disk=""
if [ "${sys_part#/dev/}" != "$sys_part" ] && command -v lsblk >/dev/null 2>&1; then
    pkname="$(lsblk -no PKNAME "$sys_part" 2>/dev/null | head -n1 | tr -d '[:space:]')"
    if [ -n "$pkname" ]; then
        disk="/dev/$pkname"
    fi
fi

if [ -z "$disk" ]; then
    echo "Yunexal auto-finalize skipped: cannot infer install disk from $sys_part" >&2
    echo "Run manually: yunexal-install finalize --disk /dev/sdX --target-root $TARGET_ROOT" >&2
    exit 0
fi

echo "Running Yunexal finalize: yunexal-install finalize --disk $disk --target-root $TARGET_ROOT"
if /usr/local/bin/yunexal-install finalize --disk "$disk" --target-root "$TARGET_ROOT"; then
    echo "Yunexal finalize completed successfully."
else
    echo "Yunexal finalize failed. Run manually: yunexal-install finalize --disk $disk --target-root $TARGET_ROOT" >&2
fi
EOF

makefile root:root 0755 "$tmp"/usr/local/sbin/setup-alpine <<'EOF'
#!/bin/sh
set -eu
exec /usr/local/sbin/yunexal-setup "$@"
EOF

makefile root:root 0644 "$tmp"/etc/motd <<'EOF'
Yunexal Alpine installer image is ready.
First-boot install flow (live USB -> local disk):
    1) yunexal-install prepare --disk /dev/sdX --mode safe --root-size-gib 40
    2) yunexal-setup  # runs installer + Yunexal finalize automatically
    3) if auto-finalize was skipped, run manually:
       yunexal-install finalize --disk /dev/sdX --target-root /mnt
    4) reboot into installed system (panel auto-starts in background)
    5) project is available at /opt/yunexal/project
    6) after reboot, run yunexal-setup to finish credentials/secrets configuration

In live installer session, yunexal-setup starts installer flow.
Use helper: yunexal-install --help
EOF

mkdir -p "$tmp"/var

# ── Installer TUI ─────────────────────────────────────────────────────────────
INSTALLER_SH="$(dirname "$0")/yunexal-installer.sh"
mkdir -p "$tmp"/usr/local/sbin
if [ -f "$INSTALLER_SH" ]; then
    cp "$INSTALLER_SH" "$tmp"/usr/local/sbin/yunexal-installer
    chmod 0755 "$tmp"/usr/local/sbin/yunexal-installer
fi

# getty -n -l: initialises the tty fully (canonical mode, echo, signals),
# then execs the installer instead of a login shell.
# /etc/issue is shown briefly (our custom "Yunexal Musl Panel Installer" message)
# before the installer clears the screen.
makefile root:root 0644 "$tmp"/etc/inittab <<'EOF'
::sysinit:/sbin/openrc sysinit
::sysinit:/sbin/openrc boot
::wait:/sbin/openrc default

::ctrlaltdel:/sbin/reboot

tty1::respawn:/sbin/getty -n -l /usr/local/sbin/yunexal-installer 38400 tty1
tty2::respawn:/sbin/getty 38400 tty2
tty3::respawn:/sbin/getty 38400 tty3
ttyS0::respawn:/sbin/getty -n -l /usr/local/sbin/yunexal-installer 115200 ttyS0

::shutdown:/sbin/openrc shutdown
EOF

if [ -n "$YUNEXAL_IMAGES_DIR" ] && [ -d "$YUNEXAL_IMAGES_DIR" ]; then
    _has_imgs=0
    for _img in "$YUNEXAL_IMAGES_DIR"/*.tar; do
        [ -f "$_img" ] && _has_imgs=1 && break
    done
    if [ "$_has_imgs" = "1" ]; then
        mkdir -p "$tmp"/opt/yunexal/images
        for _img in "$YUNEXAL_IMAGES_DIR"/*.tar; do
            [ -f "$_img" ] || continue
            cp "$_img" "$tmp"/opt/yunexal/images/
        done
        echo "Embedded Docker images from: $YUNEXAL_IMAGES_DIR"
    fi
fi

rc_add devfs sysinit
rc_add dmesg sysinit
rc_add mdev sysinit
rc_add hwdrivers sysinit
rc_add modloop sysinit

rc_add hwclock boot
rc_add modules boot
rc_add sysctl boot
rc_add hostname boot
rc_add bootmisc boot
rc_add syslog boot

rc_add networking default

rc_add mount-ro shutdown
rc_add killprocs shutdown
rc_add savecache shutdown

tar -c -C "$tmp" etc opt usr var | gzip -9n > "$HOSTNAME".apkovl.tar.gz
