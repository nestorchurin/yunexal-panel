profile_yunexal() {
    profile_standard
    profile_abbrev="yunexal"
    title="Yunexal Musl Panel Installer"
    desc="Alpine installer image with Yunexal Panel integrated"

    # Primary supported ISO target.
    arch="x86_64"
    hostname="yunexal-installer"
    image_ext="iso"
    output_format="iso"

    # Keep required host-side tooling in the generated package repository.
    apks="$apks docker docker-cli-compose nginx sudo curl bash ca-certificates util-linux e2fsprogs xfsprogs btrfs-progs gptfdisk dosfstools"

    # Custom overlay with Yunexal binaries and OpenRC service wiring.
    apkovl="genapkovl-yunexal.sh"
}
