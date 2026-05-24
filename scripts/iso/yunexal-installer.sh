#!/bin/sh
# Yunexal Musl Panel Installer
# Runs as the terminal process on tty1 / ttyS0 instead of getty.
set -e

# Ensure canonical mode and echo — getty initialises the tty, but add this as
# a safety net for any edge-case launch path where stty state is unknown.
stty sane 2>/dev/null || true

# ── Colour helpers ────────────────────────────────────────────────────────────
CY='\033[0;36m' WH='\033[1;37m' GR='\033[0;32m'
YL='\033[1;33m' RD='\033[0;31m' DM='\033[2m'   RS='\033[0m' BD='\033[1m'

c()  { printf "${CY}%s${RS}"        "$*"; }
w()  { printf "${WH}${BD}%s${RS}"   "$*"; }
gr() { printf "${GR}%s${RS}"        "$*"; }
yl() { printf "${YL}%s${RS}"        "$*"; }
rd() { printf "${RD}%s${RS}"        "$*"; }
dm() { printf "${DM}%s${RS}"        "$*"; }
hr() { printf "${CY}──────────────────────────────────────────────────────────${RS}\n"; }

# ── Input helpers ─────────────────────────────────────────────────────────────
ask() {
    # ask "prompt" "default" → REPLY
    printf "%b [%s]: " "$(c "$1")" "$2"
    read -r REPLY; [ -z "$REPLY" ] && REPLY="$2"
}

ask_secret() {
    # ask_secret "prompt" → REPLY  (no echo)
    printf "%b: " "$(c "$1")"
    stty -echo 2>/dev/null; read -r REPLY; stty echo 2>/dev/null
    printf "\n"
}

confirm() {
    printf "%b [y/N]: " "$(yl "$1")"
    read -r _a; case "$_a" in [yY]*) return 0;; *) return 1;; esac
}

wait_key() { printf "\n%b" "$(dm 'Press Enter to continue...')"; read -r _; }

step_label() {
    _total=7
    printf "\n${DM}  Step %s of %s${RS}  ${WH}${BD}%s${RS}\n\n" "$1" "$_total" "$2"
}

# ── Header ────────────────────────────────────────────────────────────────────
header() {
    clear
    printf "\n"
    printf "${CY}  ┌──────────────────────────────────────────────────────────┐${RS}\n"
    printf "${CY}  │${RS}  %s  %s${CY}│${RS}\n" \
        "$(w '  YUNEXAL MUSL PANEL INSTALLER')" \
        "                      "
    printf "${CY}  │${RS}  %s  %s${CY}│${RS}\n" \
        "$(dm '  Version 0.5.1  ·  x86_64')" \
        "                    "
    printf "${CY}  └──────────────────────────────────────────────────────────┘${RS}\n"
    printf "\n"
}

# ── Disk helpers ──────────────────────────────────────────────────────────────
part_path() {
    case "$1" in *[0-9]) printf "%sp%s" "$1" "$2";; *) printf "%s%s" "$1" "$2";; esac
}

list_disks_raw() {
    lsblk -dnpo NAME,SIZE,TYPE,MODEL 2>/dev/null | awk '$3=="disk"{print $1, $2, $4}'
}

count_disks() { list_disks_raw | wc -l; }

disk_by_index() { list_disks_raw | awk "NR==$1{print \$1}"; }

print_disk_table() {
    i=1
    list_disks_raw | while read -r _n _s _m; do
        printf "  $(yl "$i")  %-14s %-8s %s\n" "$_n" "$_s" "$_m"
        i=$((i+1))
    done
}

list_wired_ifaces() {
    ip -o link show 2>/dev/null |
        awk -F': ' '{print $2}' |
        sed 's/@.*//' |
        awk '$1 != "lo" && $1 !~ /^wl/ && $1 !~ /^wlan/ && $1 !~ /^wwan/ {print $1}'
}

is_wireless_iface() {
    case "$1" in
        wl*|wlan*|wwan*) return 0 ;;
        *) return 1 ;;
    esac
}

# ── State (collected across steps) ───────────────────────────────────────────
INTERNET_OK=0
OFFLINE_ACCEPTED=0
KEYMAP_LAYOUT="us"
KEYMAP_VARIANT="us"
NET_IFACE=""
SYS_DISK=""
DATA_DISKS=""        # space-separated list
INST_HOSTNAME="yunexal"
ROOT_PASS=""
PANEL_USER="admin"
PANEL_PASS=""
TIMEZONE="UTC"

# ── Step 1: Requirements ──────────────────────────────────────────────────────
step_requirements() {
    header
    step_label 1 "Check requirements"

    printf "  Checking system requirements...\n\n"

    # RAM
    _ram_mb="$(awk '/^MemTotal/{print int($2/1024)}' /proc/meminfo 2>/dev/null || echo 0)"
    if [ "$_ram_mb" -ge 512 ]; then
        printf "  $(gr '✓')  RAM: %s MiB\n" "$_ram_mb"
    else
        printf "  $(yl '!')  RAM: %s MiB (512 MiB minimum recommended)\n" "$_ram_mb"
    fi

    # Disk presence
    if [ "$(count_disks)" -gt 0 ]; then
        printf "  $(gr '✓')  Block devices found: %s\n" "$(count_disks)"
    else
        printf "  $(rd '✗')  No block devices found\n"
    fi

    # Internet
    printf "  %b  Internet..." "$(dm '…')"
    if ping -c1 -W3 8.8.8.8 >/dev/null 2>&1 || \
       ping -c1 -W3 1.1.1.1 >/dev/null 2>&1; then
        INTERNET_OK=1
        printf "\r  $(gr '✓')  Internet: reachable              \n"
    else
        INTERNET_OK=0
        printf "\r  $(rd '✗')  Internet: not reachable          \n"
    fi

    printf "\n"
    hr
    printf "\n"

    if [ "$INTERNET_OK" = "0" ]; then
        printf "  $(yl '⚠')  Internet access is $(w 'required') to download base OS packages\n"
        printf "      and security updates during installation.\n\n"
        printf "  Docker images for Yunexal are embedded in this ISO and\n"
        printf "  will work offline, but the base OS installation will fail\n"
        printf "  without APK repository access.\n\n"
        if confirm "  I understand and accept responsibility for proceeding offline"; then
            OFFLINE_ACCEPTED=1
        else
            printf "\n  $(rd 'Aborting.')  Please connect to the network and reboot.\n\n"
            wait_key
            return 1
        fi
    fi

    return 0
}

# ── Step 2: Keyboard layout ───────────────────────────────────────────────────
step_keyboard() {
    header
    step_label 2 "Keyboard layout"

    # List available layouts (top-level dirs under /usr/share/bkeymaps)
    _kmdir="/usr/share/bkeymaps"
    if [ ! -d "$_kmdir" ]; then
        printf "  $(yl '!')  Keymap directory not found, skipping.\n"
        wait_key; return 0
    fi

    printf "  Available layouts:\n\n"
    _layouts="$(ls "$_kmdir" 2>/dev/null | tr '\n' ' ')"
    printf "  %s\n\n" "$_layouts"

    ask "  Layout" "us"
    KEYMAP_LAYOUT="$REPLY"

    if [ -d "$_kmdir/$KEYMAP_LAYOUT" ]; then
        printf "\n  Variants for $(c "$KEYMAP_LAYOUT"):\n\n"
        _variants="$(ls "$_kmdir/$KEYMAP_LAYOUT" 2>/dev/null | sed 's/\.bmap\.gz$//' | tr '\n' ' ')"
        printf "  %s\n\n" "$_variants"
        ask "  Variant" "$KEYMAP_LAYOUT"
        KEYMAP_VARIANT="$REPLY"
    else
        KEYMAP_VARIANT="$KEYMAP_LAYOUT"
    fi

    # Apply immediately to live session
    _kmap="$_kmdir/$KEYMAP_LAYOUT/$KEYMAP_VARIANT.bmap.gz"
    if [ -f "$_kmap" ]; then
        zcat "$_kmap" | loadkmap 2>/dev/null && \
            printf "\n  $(gr '✓')  Applied: %s / %s\n" "$KEYMAP_LAYOUT" "$KEYMAP_VARIANT" || \
            printf "\n  $(yl '!')  Could not apply keymap\n"
    else
        printf "\n  $(yl '!')  Keymap file not found, layout will be applied during install\n"
    fi

    printf "\n"
    printf "  $(dm 'To identify your keyboard, type a few special characters now.')\n"
    wait_key
}

# ── Step 3: Networking ────────────────────────────────────────────────────────
step_networking() {
    header
    step_label 3 "Networking (Ethernet only)"

    printf "  Detected wired interfaces:\n\n"
    _wired_list="$(list_wired_ifaces)"
    if [ -n "$_wired_list" ]; then
        printf "%s\n" "$_wired_list" | while read -r _ifn; do
            [ -n "$_ifn" ] && printf "  •  %s\n" "$_ifn"
        done
    else
        printf "  $(yl '!')  No wired interfaces detected; defaulting to eth0.\n"
    fi
    printf "\n"

    _default_iface="$(printf "%s\n" "$_wired_list" | head -n1)"
    [ -n "$_default_iface" ] || _default_iface="eth0"

    ask "  Interface to configure" "$_default_iface"
    NET_IFACE="$REPLY"

    if is_wireless_iface "$NET_IFACE"; then
        printf "\n  $(rd 'Wireless interfaces are disabled in this installer image.')\n"
        printf "  $(yl 'Use Ethernet and try again.')\n\n"
        wait_key
        return 1
    fi

    printf "\n  $(c '1')  DHCP  $(dm '(automatic)')\n"
    printf "  $(c '2')  Static IP\n\n"
    ask "  Mode" "1"

    case "$REPLY" in
        2)
            ask   "  IP address (CIDR, e.g. 192.168.1.10/24)" ""
            _ip="$REPLY"
            ask   "  Gateway" ""
            _gw="$REPLY"
            ask   "  DNS servers (space-separated)" "8.8.8.8 8.8.4.4"
            _dns="$REPLY"

            ip addr flush dev "$NET_IFACE" 2>/dev/null || true
            ip addr add "$_ip" dev "$NET_IFACE" 2>/dev/null
            ip link set "$NET_IFACE" up 2>/dev/null
            ip route add default via "$_gw" 2>/dev/null || true

            _dnsfile="/etc/resolv.conf"
            printf "" > "$_dnsfile"
            for _d in $_dns; do printf "nameserver %s\n" "$_d" >> "$_dnsfile"; done

            printf "\n  $(gr '✓')  Static IP configured: %s\n" "$_ip"
            NET_MODE="static"
            ;;
        *)
            ip link set "$NET_IFACE" up 2>/dev/null || true
            udhcpc -i "$NET_IFACE" -q 2>/dev/null && \
                printf "\n  $(gr '✓')  DHCP lease obtained on %s\n" "$NET_IFACE" || \
                printf "\n  $(yl '!')  DHCP failed — you can proceed but package download may fail\n"
            NET_MODE="dhcp"
            ;;
    esac

    printf "\n"
    ask "  Timezone" "UTC"
    TIMEZONE="$REPLY"

    printf "\n"
    wait_key
}

# ── Step 4: Configure storage ─────────────────────────────────────────────────
step_storage() {
    header
    step_label 4 "Configure storage"

    if [ "$(count_disks)" -eq 0 ]; then
        rd "  No block devices found. Cannot continue.\n"
        wait_key; return 1
    fi

    # ── System disk ──────────────────────────────────────────────────────────
    printf "  $(w 'System disk')  $(dm '(OS + Yunexal Musl Panel)')\n\n"
    print_disk_table
    printf "\n"
    ask "  Disk number for system" "1"
    SYS_DISK="$(disk_by_index "$REPLY")"
    [ -n "$SYS_DISK" ] || { rd "\n  Invalid selection.\n"; wait_key; return 1; }

    printf "\n"
    printf "  Layout for $(gr "$SYS_DISK"):\n"
    printf "  $(dm '  p1  EFI      512 MiB   FAT32')\n"
    printf "  $(dm '  p2  SYSTEM   100 GiB   ext4   ← OS + Yunexal Musl Panel')\n"
    printf "  $(dm '  p3  DATA     rest       ext4 + prjquota  ← local volumes')\n\n"

    # ── Additional data disks ─────────────────────────────────────────────────
    _ndisks="$(count_disks)"
    if [ "$_ndisks" -gt 1 ]; then
        printf "  $(w 'Additional data disks')  $(dm '(optional — container volumes, prjquota)')\n\n"
        print_disk_table
        printf "\n"
        printf "  Enter disk numbers separated by spaces, or leave empty to skip.\n"
        printf "  $(rd 'Each selected disk will be fully erased and formatted.')\n\n"
        ask "  Additional disk numbers" ""
        _extra_nums="$REPLY"

        DATA_DISKS=""
        for _n in $_extra_nums; do
            _d="$(disk_by_index "$_n")"
            if [ -n "$_d" ] && [ "$_d" != "$SYS_DISK" ]; then
                DATA_DISKS="$DATA_DISKS $_d"
            fi
        done
        DATA_DISKS="${DATA_DISKS# }"
        [ -n "$DATA_DISKS" ] && printf "\n  $(gr '✓')  Additional disks: %s\n" "$DATA_DISKS"
    fi

    printf "\n"
    wait_key
}

# ── Step 5: Profile ───────────────────────────────────────────────────────────
step_profile() {
    header
    step_label 5 "Setup profile"

    printf "  $(w 'System')\n\n"
    ask "  Hostname" "yunexal"
    INST_HOSTNAME="$REPLY"

    printf "\n"
    while true; do
        ask_secret "  System root password"
        _p1="$REPLY"
        ask_secret "  Confirm root password"
        [ "$REPLY" = "$_p1" ] && break
        rd "  Passwords do not match, try again.\n\n"
    done
    ROOT_PASS="$_p1"

    printf "\n"
    hr
    printf "\n"
    printf "  $(w 'Yunexal Musl Panel admin account')\n\n"

    ask "  Admin username" "admin"
    PANEL_USER="$REPLY"

    while true; do
        ask_secret "  Admin password (min 8 chars)"
        _p1="$REPLY"
        [ "${#_p1}" -ge 8 ] || { rd "  Password must be at least 8 characters.\n\n"; continue; }
        ask_secret "  Confirm admin password"
        [ "$REPLY" = "$_p1" ] && break
        rd "  Passwords do not match, try again.\n\n"
    done
    PANEL_PASS="$_p1"

    printf "\n"
    wait_key
}

# ── Step 6: Confirm partitions ────────────────────────────────────────────────
step_confirm_partitions() {
    header
    step_label 6 "Confirm partition layout"

    printf "  $(w 'System disk:')  $(gr "$SYS_DISK")\n\n"
    printf "  %-6s  %-20s  %-10s  %s\n" "Part" "Mount" "Size" "Format"
    hr
    printf "  %-6s  %-20s  %-10s  %s\n" "p1" "EFI" "512 MiB" "FAT32"
    printf "  %-6s  %-20s  %-10s  %s\n" "p2" "/  (system)" "100 GiB" "ext4"
    printf "  %-6s  %-20s  %-10s  %s\n" "p3" "/var/lib/yunexal/volumes" "remaining" "ext4 + prjquota"

    if [ -n "$DATA_DISKS" ]; then
        printf "\n  $(w 'Additional data disks:')\n\n"
        _i=1
        for _d in $DATA_DISKS; do
            _sz="$(lsblk -dnpo SIZE "$_d" 2>/dev/null || echo '?')"
            printf "  %-14s  %-30s  %-10s  %s\n" "$_d" \
                "/var/lib/yunexal/volumes/disk-$_i" "$_sz" "ext4 + prjquota"
            _i=$((_i+1))
        done
    fi

    printf "\n"
    printf "  $(w 'Hostname:')   %s\n" "$INST_HOSTNAME"
    printf "  $(w 'Timezone:')   %s\n" "$TIMEZONE"
    printf "  $(w 'Keyboard:')   %s / %s\n" "$KEYMAP_LAYOUT" "$KEYMAP_VARIANT"
    printf "  $(w 'Panel admin:')%s\n" "$PANEL_USER"
    printf "\n"

    wait_key
}

# ── Step 7: Confirm changes ───────────────────────────────────────────────────
step_confirm_changes() {
    header
    step_label 7 "Confirm changes"

    printf "  $(rd 'The following disks will be ERASED:')\n\n"
    printf "  $(rd '  •  %s  (system)')\n" "$SYS_DISK"
    for _d in $DATA_DISKS; do
        printf "  $(rd '  •  %s  (data)')\n" "$_d"
    done

    printf "\n"
    printf "  After confirmation, installation will proceed automatically.\n"
    printf "  $(dm 'Do not power off the system during installation.')\n\n"
    hr
    printf "\n"

    confirm "  Erase disks and install Yunexal Musl Panel" || return 1
    return 0
}

# ── Execute installation ──────────────────────────────────────────────────────
execute_install() {
    header
    printf "  $(w 'Installing Yunexal Musl Panel...')\n\n"

    _sysp2="$(part_path "$SYS_DISK" 2)"
    _sysp3="$(part_path "$SYS_DISK" 3)"

    # ── 1. Partition system disk ──────────────────────────────────────────────
    hr; printf "\n  $(c '[1/6]')  Partitioning system disk %s...\n\n" "$SYS_DISK"
    yunexal-install prepare \
        --disk "$SYS_DISK" \
        --root-size-gib 100 \
        --mode force \
        --yes || { rd "\n  Partitioning failed.\n"; wait_key; return 1; }

    # ── 2. Install Alpine base OS ─────────────────────────────────────────────
    hr; printf "\n  $(c '[2/6]')  Installing base OS to %s...\n\n" "$_sysp2"

    # Write answers file so setup-disk doesn't ask us questions
    _answers="/tmp/yunexal-setup-answers"
    cat > "$_answers" <<ANSWERS
KEYMAPOPTS="$KEYMAP_LAYOUT $KEYMAP_VARIANT"
HOSTNAMEOPTS="-n $INST_HOSTNAME"
INTERFACESOPTS="auto lo
iface lo inet loopback

auto $NET_IFACE
iface $NET_IFACE inet $NET_MODE
"
DNSOPTS="-d local 8.8.8.8"
TIMEZONEOPTS="-z $TIMEZONE"
PROXYOPTS="none"
APKREPOSOPTS="-1"
SSHDOPTS="-c openssh"
NTPOPTS="-c chrony"
DISKOPTS="-m sys $_sysp2"
ANSWERS

    ERASE_DISKS="" setup-alpine -f "$_answers" || \
        { rd "\n  Base OS installation failed.\n"; wait_key; return 1; }

    # setup-alpine may have mounted target at /mnt
    TARGET="${TARGET_ROOT:-/mnt}"
    [ -d "$TARGET/etc" ] || { rd "\n  Target root not found at $TARGET\n"; wait_key; return 1; }

    # ── 3. Set root password ──────────────────────────────────────────────────
    hr; printf "\n  $(c '[3/6]')  Setting system root password...\n\n"
    printf "root:%s" "$ROOT_PASS" | chpasswd -R "$TARGET" 2>/dev/null || \
        echo "root:$ROOT_PASS" | chroot "$TARGET" chpasswd 2>/dev/null || true
    printf "  $(gr '✓')  Root password set\n"

    # ── 4. Format additional data disks ──────────────────────────────────────
    if [ -n "$DATA_DISKS" ]; then
        hr; printf "\n  $(c '[4/6]')  Formatting data disks...\n\n"
        _di=1
        for _d in $DATA_DISKS; do
            printf "  Formatting %s...\n" "$_d"
            wipefs -af "$_d" >/dev/null 2>&1 || true
            mkfs.ext4 -F -O project "$_d" >/dev/null 2>&1 && \
                printf "  $(gr '✓')  %s formatted (ext4 + prjquota)\n" "$_d" || \
                printf "  $(yl '!')  %s format failed\n" "$_d"

            _uuid="$(blkid -s UUID -o value "$_d" 2>/dev/null || true)"
            if [ -n "$_uuid" ]; then
                _mnt="/var/lib/yunexal/volumes/disk-$_di"
                printf "UUID=%s %s ext4 defaults,prjquota 0 2\n" "$_uuid" "$_mnt" \
                    >> "$TARGET/etc/fstab"
                mkdir -p "$TARGET$_mnt"
            fi
            _di=$((_di+1))
        done
    else
        hr; printf "\n  $(c '[4/6]')  No additional data disks — skipping.\n\n"
    fi

    # ── 5. Yunexal finalize ───────────────────────────────────────────────────
    hr; printf "\n  $(c '[5/6]')  Deploying Yunexal Musl Panel...\n\n"
    _disk_arg="$SYS_DISK"
    yunexal-install finalize --disk "$_disk_arg" --target-root "$TARGET" || \
        { rd "\n  Yunexal finalize failed.\n"; wait_key; return 1; }

    # ── 6. Panel admin — first-boot setup service ────────────────────────────
    hr; printf "\n  $(c '[6/6]')  Scheduling Yunexal Musl Panel first-boot setup...\n\n"
    _paneldir="$TARGET/var/lib/yunexal/panel"
    mkdir -p "$_paneldir"

    # Write credentials file (readable only by root, deleted after first boot)
    _credfile="$_paneldir/.first-boot-admin"
    printf "PANEL_USERNAME=%s\nPANEL_PASSWORD=%s\n" "$PANEL_USER" "$PANEL_PASS" \
        > "$_credfile"
    chmod 0600 "$_credfile"

    # One-shot OpenRC service: runs yunexal-setup --non-interactive on first boot
    # after Docker is available, then removes the credentials file and itself.
    cat > "$TARGET/etc/init.d/yunexal-firstboot" <<'EOF_FB'
#!/sbin/openrc-run
name="yunexal-firstboot"
description="Yunexal Musl Panel first-boot initialisation"

depend() {
    need docker yunexal-panel
    after  docker yunexal-panel
}

start() {
    local cred="/var/lib/yunexal/panel/.first-boot-admin"
    [ -f "$cred" ] || return 0

    ebegin "Initialising Yunexal Musl Panel (first boot)"
    (
        set -a
        . "$cred"
        set +a
        cd /var/lib/yunexal/panel
        yunexal-setup --non-interactive
    )
    eend $?

    rm -f "$cred"
    # Disable self so it never runs again
    rc-update del yunexal-firstboot default 2>/dev/null || true
}
EOF_FB
    chmod 0755 "$TARGET/etc/init.d/yunexal-firstboot"

    # Enable the service in the default runlevel of the target
    mkdir -p "$TARGET/etc/runlevels/default"
    ln -sf "/etc/init.d/yunexal-firstboot" \
        "$TARGET/etc/runlevels/default/yunexal-firstboot"

    printf "  $(gr '✓')  Admin $(w "$PANEL_USER") will be created on first boot\n"
    printf "  $(dm '    (credentials stored securely, deleted after first run)')\n"

    # ── Done ──────────────────────────────────────────────────────────────────
    hr
    printf "\n"
    gr "  Installation complete!\n\n"
    printf "  $(gr '✓')  Yunexal Musl Panel will start automatically after reboot\n"
    printf "  $(gr '✓')  Access the web interface at: $(w "http://$INST_HOSTNAME:3000")\n"
    printf "  $(gr '✓')  Run $(w 'yunexal-setup') to reconfigure at any time\n"
    printf "\n"
    confirm "  Reboot now?" && reboot
    printf "\n"
    wait_key
}

# ── Main menu ─────────────────────────────────────────────────────────────────
main_menu() {
    header
    printf "  Welcome to the $(w 'Yunexal Musl Panel') installation wizard.\n\n"
    printf "  This guided installer will:\n"
    printf "    $(gr '✓')  Partition and format your target disk\n"
    printf "    $(gr '✓')  Install base OS\n"
    printf "    $(gr '✓')  Deploy Yunexal Musl Panel as a native service\n"
    printf "    $(gr '✓')  Pre-load Docker images (works offline after install)\n"
    printf "\n"
    hr
    printf "\n"
    printf "  $(yl '1')  Install Yunexal Musl Panel\n"
    printf "  $(yl '2')  Open shell  $(dm '(advanced)')\n"
    printf "  $(yl '3')  Reboot\n"
    printf "\n"
    ask "  Select" "1"
    MENU_CHOICE="$REPLY"
}

# ── Entry point ───────────────────────────────────────────────────────────────
[ -t 0 ] || exec /bin/sh

while true; do
    main_menu
    case "$MENU_CHOICE" in
        1)
            step_requirements  || continue
            step_keyboard
            step_networking    || continue
            step_storage       || continue
            step_profile
            step_confirm_partitions
            step_confirm_changes || continue
            execute_install
            ;;
        2)
            printf "\n  Dropping to shell. Type $(yl 'exit') to return.\n\n"
            /bin/sh || true
            ;;
        3)
            reboot ;;
        *)
            yl "  Unknown option.\n"; sleep 1 ;;
    esac
done
