# Changelog

## [0.5.1] - 2026-05-24

### Added
- **feat(sftp):** built-in SFTP server with Pterodactyl-style authentication
  — username `{server_id}.{panel_username}`, password verified via Argon2id,
  session chrooted to server volume, Ed25519 host key auto-generated and
  encrypted in DB, configurable port (default 2022), Cloudflare Spectrum and
  DNS-only modes documented in admin settings.

### Fixed
- **fix(docker):** `--cpus` now always passed to `docker update`, including
  when the value is `0`, so clearing the CPU limit is correctly applied.
- **fix(installer):** export `PATH` explicitly for busybox init environments.
- **fix(installer):** replace `lsblk` with `/sys/block` for disk detection.
- **fix(installer):** auto-select system disk when only one is available.

### Security
- **feat(audit):** IP addresses in audit log hidden behind `audit.view_ip`
  permission.
