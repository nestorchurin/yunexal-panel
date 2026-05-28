# Changelog

## [1.0.1b] - 2026-05-28

### Fixed
- **fix(console):** macro delete in the Command Macros modal now stays available when the container is stopped; only macro run actions are blocked.
- **fix(console):** popular-macros hover panel now waits 500ms before hiding after mouse leave, so moving the cursor into the popup is reliable.
- **fix(console):** bumped `console.js` cache-bust to `v=6` to ship the hotfix immediately.

## [1.0.1] - 2026-05-28

### Added
- **feat(console):** command macro strip above the terminal with quick-run chips and a manage modal.
- **feat(console):** per-server browser-stored command history with ArrowUp / ArrowDown recall and deduped submission history.
- **feat(console):** autocomplete dropdown for terminal commands that combines macros and recent history, including custom project-specific commands saved by the user.

### Changed
- **feat(console):** bumped console asset cache-bust versions so the updated command UI loads immediately after deploy.
- **chore(release):** added `scripts/release/package-github-release.sh` to bundle `yunexal-panel.bin`, `yunexal-setup.bin`, and a 1.0.0 source archive for GitHub Releases.
- **fix(console):** command macros now stay disabled until the container is running, and `type a command` autocomplete now uses recent history only.
- **fix(console):** deleting a macro now asks for confirmation before removing it.
- **fix(console):** removed command history from the console input; Enter now sends only the current line, with no recall or typeahead.
- **fix(console):** removed the extra wrapper around the command input so the field stretches full width again.
- **feat(console):** macro bar can now collapse/expand, and it defaults to a compact collapsed state on phones.
- **fix(console):** macro bar collapse state is now stored separately for mobile vs desktop, so phones stay compact by default.
- **fix(console):** moved the macro chips out of the page flow and into the macros modal to keep the console header compact.
- **fix(console):** moved the macros access button into the command bar so it sits inline with `type a command`.
- **feat(console):** macros launcher is now icon-only, with a hover popover showing the top 5 most-used user macros and a click opening macro settings.
- **fix(console):** phone users now get a long-press preview for popular macros, while a normal tap still opens macro settings.
- **feat(console):** macros can now have editable hotkeys, and matching key combos trigger the saved macro instantly.
- **fix(console):** macro chips and the popular-macros popup now sort by usage and recency metadata instead of saved order.
- **feat(schedules):** added script schedules with ordered steps, so a cron entry can run command, delay, power, and backup actions in sequence.
- **fix(schedules):** optimized the scenario editor design with clearer step hierarchy, type accents, and mobile-friendly spacing.
- **fix(schedules):** moved the schedule modal inside `.yu-main` for SPA navigation and added null-safe guards in `openModal()` to prevent DOM null crashes.
- **fix(schedules):** script-step numbers now auto-renumber after deletions, so numbering always stays continuous (1, 2, 3...).
- **style(schedules):** redesigned the modal `Enabled` control into a clearer toggle switch with helper copy for better desktop/mobile UX.
- **docs(schedules):** added a small cron format instruction with examples below `Cron Expression` in the schedule modal.
- **fix(files):** prevented SPA race on Files tab entry by deduplicating rapid page-shown eager loads and using safe element-targeted HTMX reload calls (fixes `querySelector` null crash).
- **fix(console):** switched to a local patched xterm CSS bundle and rewrote overline+underline decoration rules, removing browser `text-decoration` parse warnings.
- **fix(console):** moved console modals (`macroModal`, `killModal`) inside `.yu-main`, so macro launcher and kill dialog work immediately after sidebar SPA navigation.
- **fix(spa):** sidebar navigation now force-detaches all non-target `.yu-main` views before attach, preventing runaway page scroll after leaving Console.
- **fix(console):** cleanup now removes orphan xterm measurement probe nodes (`top:-50000px`, `width:50000px`) on SPA page switches, fixing runaway bi-directional scrolling after leaving Console.
- **fix(console):** bumped `console.js` cache-bust to `v=5` so the orphan xterm cleanup ships immediately.
- **fix(spa):** sidebar tab cache now stores the initial page title, so `document.title` correctly updates when returning to cached tabs (e.g., Settings -> Console).
- **fix(spa):** bumped `sidebar.js` cache-bust to `v=12` so the title-sync fix loads without stale browser assets.
- **fix(files):** cancelling the Upload file picker no longer crashes `fbUploadFiles`; added empty-selection early return and null-safe progress UI updates.
- **fix(files):** bumped `files.js` cache-bust to `v=5` so the upload-cancel crash fix is delivered immediately.
- **fix(files):** moved Files page auxiliary UI nodes (toasts, upload progress, rename/create modals, context menu) inside `.yu-main`, so they are available after sidebar SPA tab swaps.
- **fix(files):** normalized Files modal/backdrop z-index on open, fixing blocked modal controls when backdrop intercepted clicks after SPA attach.
- **fix(files):** bumped `files.js` cache-bust to `v=6` for the modal stacking fix.
- **fix(files):** Files modals now open with Bootstrap `backdrop: false` and cleanup stray backdrops, fixing pointer interception in SPA stacking contexts.
- **fix(files):** bumped `files.js` cache-bust to `v=7` for the backdrop interception fix.
- **fix(sidebar):** pressing `Escape` now closes the mobile sidebar in both server pages and admin panel.
- **fix(sidebar):** bumped `sidebar.js` to `v=13` and `admin.js` to `v=25` so the Escape shortcut ships without stale cache.
- **fix(admin):** added explicit `/admin/` route alias to `admin_page`, so trailing-slash admin URL no longer returns 404.
- **feat(admin):** implemented the `Notifications` tab with real persisted settings (global toggle, webhook URL, email recipients, and event toggles) instead of a placeholder panel.
- **feat(admin):** added root-only `POST /api/admin/notifications/test` webhook test API with URL validation and audit logging.
- **fix(admin):** removed the `SOON` marker for `Notifications` in admin sidebar and bumped `admin.js` cache-bust to `v=26`.

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
