# Yunexal Panel

> **v1.0.0** — Self-hosted server management platform built on Docker.

Built with **Rust + Axum**, **SQLite**, and **Bollard** (Docker SDK).  
Templates and static assets are compiled into a single binary — no external runtime files needed.

---

## Table of Contents

- [Features](#features)
- [Roadmap](#roadmap)
- [Installation](#installation)
- [Repository Status](#repository-status)
- [Reverse Proxy (nginx)](#reverse-proxy-nginx)
- [Requirements](#requirements)
- [Configuration](#configuration)
- [Building from Source](#building-from-source)
- [Project Structure](#project-structure)
- [Tech Stack](#tech-stack)
- [License](#license)
- [Contributors](CONTRIBUTORS.md)

---

## Features

### Dashboard
- Live grid of all managed containers with CPU %, RAM, Network I/O, Disk I/O and uptime
- **In-place updates** — cards refresh state without DOM re-creation (no animation flicker)
- **"My only" toggle** — admins can filter to show only their own servers; placed in the topbar next to "New Server"
- Auto-polling every 5 s; status badges: Running / Stopped / Error
- **Settings modal** — change own password and manage active devices (sessions) without leaving the dashboard

### Server Console
- WebSocket terminal attached to Docker container TTY via **xterm.js**
- Full ANSI colour support + HTML-tag converter for servers that emit `<b>`, `<span style="color:">`, etc.
- Dedicated command input field below the terminal (Enter sends to stdin)
- Live metric charts (1 s polling, 200-point history):
  - CPU % · RAM % (used / limit) · Network KB/s · Disk I/O KB/s
- **Storage card** — volume size (MB) fetched once on open

### Server Sidebar — SPA Navigation
- Server tabs (Console · Files · Backups · Networking · Schedules · Settings · Audit · Users) load without full page reloads
- Only `.yu-main` is replaced on navigation; sidebar and assets remain intact
- Each tab re-initialises its own polling/WS lifecycle via `yu:page-shown` events
- Back/forward browser navigation works correctly via `history.pushState`

### File Manager
- Folder/file browsing with breadcrumb navigation
- **150+ format icons** across 14 colour-coded categories — code, config, archive, image, audio, video,
  binary, lock, shell/scripts, Python, Java/JVM, HTML templates, CSS, data/CSV and more;
  special exact-name detection for `Makefile`, `Dockerfile`, `README`, `LICENSE`, etc.
- **Edit** text/config files in a full-screen Ace code editor
- **Create** new files and directories
- **Rename**, **Copy/Paste**, **Delete** (right-click context menu)
- **Drag-and-drop upload** with per-file progress (streamed to disk, root-permission safe via helper container);
  large files use chunked upload (85 MiB threshold) with parallel workers and retry policy
- **Archive & Extract** — create `.tar.gz` archives; extract `.tar.gz`, `.tar.bz2`, `.tar.xz`, `.zip`, `.jar`, `.rar`, `.7z`, `.gz`, `.bz2`, `.xz`;
  two extract modes: *Extract to folder* or *Extract here*
- **Non-editable guard** — binary/archive/media files are blocked from opening in the text editor
- Path traversal and symlink-escape protection enforced on all backend endpoints

### Backups
- Per-container backup tab at `/servers/{id}/backups`
- Create `.tar.gz` snapshots of the container volume on demand; spinner + download lock during creation
- Backups stored in a **sibling directory** (`{volume}_backups/`) adjacent to the container volume — never inside it, never included in future backups
- **Max backups** limit per container (configurable in Admin Panel, default 10); oldest-first enforcement
- Download individual backups; delete with confirmation
- **Upload & Restore** — upload any `.tar.gz` archive to fully replace the container's volume contents;
  clear warning modal, XHR progress bar; falls back to `docker run alpine` for root-owned volumes

### Schedules
- Per-container schedule manager at `/servers/{id}/schedules`
- Cron-expression based triggers with human-readable preview
- Three task types: **Command** (docker exec inside container), **Power Action** (start/stop/restart), **Backup** (tar.gz snapshot)
- Enable/disable individual schedules without deleting them
- Run-now button for immediate manual execution
- Per-entry run history with status badges (success / error) and output
- Background Tokio runner checks due schedules every 30 s; history capped at `max_schedule_runs` per container (default 20)

### Server Settings
- **Environment Variables** — row-based editor: each `KEY=VALUE` rendered as its own row
  - Regular users can edit values; only admins can add, delete or rename keys
  - "Save ENV" recreates the container with the new environment
- **Factory Reset** — wipes the volume and restarts the container; requires password confirmation
  - Redesigned modal: danger-styled border, eye-toggle on the password field
- **Danger Zone** — Delete Server (admin only)

### Server Members (User Access Control)
- Add users to a specific container without granting global admin rights
- Members are looked up by **UID** — unique user identity string (9–16 characters)
- Per-capability access policy (`none / read / write`) for: **console · files · networking · settings · audit · power · schedules**
- Users tab in the server sidebar — visible only when the container has members
- Non-admin members see only their permitted tabs; power actions (start/stop/restart/kill) respect the `power:write` policy

### Server Audit Log
- Per-server immutable event log at `/servers/{id}/audit`
- Filter by action type, actor, free-text search; paginated (50 per page)
- **Actor role badge** — `root` (red), `admin` (amber), named roles (grey) displayed inline next to username
- **IP privilege masking** — IPs of users with a higher privilege level than the viewer are hidden (`[hidden]`); applies to both the live table and downloaded log export
- Device column with parsed User-Agent; full UA in tooltip
- Download complete log as `.log` file with current filters applied; export includes `role=` field per line

### Networking
- View all port bindings (host ↔ container) with protocol (TCP / UDP / TCP+UDP)
- **Add / Remove** port mappings (admin only) with port conflict pre-check
- **Tag** ports with a friendly label (e.g. `Game`, `RCON`)
- **Enable / Disable** individual port mappings
- **UFW block** — per-port shield button blocks/unblocks traffic at OS level via `sudo -n ufw`
  - Visible only when UFW is enabled in Panel Settings
  - Permission-aware: shows a sudoers fix command if `sudo -n` is denied
- **Bandwidth limiting** via Linux `tc` TBF qdisc (Mbit/s) — persisted and reapplied on restart

### Container Creation (admin only)
- Create containers from any Docker Hub or local image
  - **Local-first** — prefers an already-pulled local image before attempting a pull
  - **In-input image picker** — dropdown with local image suggestions, filterable as you type
  - **Build from Dockerfile** — upload a Dockerfile directly in the UI to build a custom local image
- Full **Docker Compose-style YAML** config via Monaco editor (live GUI ↔ YAML sync)
- Dynamic port-binding rows with host/container fields and protocol selector
- **"Fetch ENV"** — auto-detects environment variables from Docker image metadata
- **Image ENV overrides** — admin-configured DB defaults applied on top of image defaults
- Port conflict detection and duplicate name check before creation
- Owner assignment — assign any container to any user
- **Quota hard-block** — creation is blocked if the selected storage path is not quota-capable (ext4 + prjquota required)

### Admin Panel
**Tabs:** Overview · Containers · Images · Users · Roles · Audit Log · Panel Settings

- **Users** — create, delete and set passwords; tri-state RBAC roles; UID + Nickname identity model;
  admins cannot delete other admins — only `root` can
- **Roles & Permissions (RBAC)** — Role Studio UI: create custom roles, edit per-permission policy (`read / none / write`),
  assign colour; `root` role is immutable; topbar badge shows the current user's role with its colour
- **Images** — pull, delete, duplicate, ENV override editor
- **Containers** — edit any container (disk limit, bandwidth, ENV, ports, max backups, max schedule run history); stop all at once; per-row state updates without animation flicker
- **Audit Log** — immutable; 200 records per page; multi-select action filter; Device column (parsed User-Agent); full UA in tooltip
- **Panel Updates** — check for new releases (stable/unstable), one-click download & install with auto-restart
- **Panel Settings** — categorical layout (Storage · Security · Operations · Interface)

### Panel Settings (root only)
- **UFW toggle** — enable/disable UFW port-blocking globally
- **Bandwidth toggle** — show/hide the Bandwidth section on Networking pages
- **Sidebar Visibility** — toggle SOON (upcoming feature) badges in the admin sidebar
- **ZRAM hint** — collapsible "How to enable ZRAM" block when ZRAM is inactive
- **Storage** — default container storage path; cross-disk migration; disk filesystem conversion (ext4);
  unsafe override mode for advanced setups
- **API Key** — service API key for third-party integrations (`YUNEXAL_API_KEY`)

### Session Management
- Password change **immediately invalidates all active sessions** across all devices
- **Devices modal** (Dashboard › Settings) — lists all active sessions with device/browser info
- Per-device logout without affecting other sessions; current session is protected from remote logout
- Session cookies carry a password-hash stamp — stale cookies trigger automatic re-login

### API Access
- Service API key stored in panel settings (or `YUNEXAL_API_KEY` / `PANEL_API_KEY` env vars)
- `POST /api/auth/service-login` — exchange API key for a standard session cookie;
  third-party services can then call all panel API endpoints without browser login

### Authentication & Security
- Session-based login with **encrypted private cookies** (AES-GCM via axum-extra)
- **Argon2id** password hashing (random salt)
- Route-level middleware: unauthenticated → `/login`; non-admin on admin routes → 403
- **Rate limiting** — 5 failed logins per IP → 60 s lockout
- **Security headers** — CSP, X-Frame-Options, HSTS, Referrer-Policy, Permissions-Policy
- **SameSite=Strict** session cookies prevent CSRF
- XSS protection: Askama auto-escaping + `escHtml()` / `escAttr()` in JavaScript
- Path traversal and symlink-escape protection on all file endpoints
- `X-Forwarded-For` / `X-Real-IP` trusted only from local/private proxy addresses

### UI / UX
- Responsive **Bootstrap 5** dark-mode layout
- **AMOLED mode** — pure-black theme for mobile OLED screens with auto-fullscreen
- **PWA** — `manifest.json` + service worker for installable web app
- Load-time footer badge (seconds) on every page

---

## Roadmap

> The panel is in active development with a focus on stability and core features first.
> The following features are planned for the next few releases.
> Spoiler: it's a dont completed roadmap, because we has a dynamic and crazy ideas that we want to implement, but we will try to stick to this roadmap as much as possible.
> If you have any suggestions or want to contribute, feel free to open an issue or a pull request!

### General
| Status | Feature |
|---|---|
| ✅ | Overview — system stats, ZRAM, panel updates |
| ✅ | All Containers — manage any container across all users |
| ✅ | Images — pull, delete, duplicate, ENV overrides |

### Management
| Status | Feature |
|---|---|
| ✅ | Users — create, delete, roles with full RBAC permission matrix |
| ✅ | Roles — custom roles with granular permissions and colour coding |
| ✅ | Schedules — cron-based task scheduling (command / power / backup) |
| ✅ | Backups — on-demand volume snapshots, upload restore, per-container retention limit |
| 🔜 | Distpatchers — task dispatchers and future agent manager workflows |
| 🔜 | Firewall — global IP allow/block rules beyond per-port UFW |
| 🔜 | Tickets — built-in support ticket system for end users |

### Analytics
| Status | Feature |
|---|---|
| ✅ | Audit Log — immutable, global + per-server, multi-select filter, role badge, IP privilege masking |
| 🔜 | Insights — historical resource usage charts and trend analysis |

### Configuration
| Status | Feature |
|---|---|
| ✅ | Panel Settings — UFW, bandwidth, sidebar visibility, panel updates |
| ✅ | API Keys — service API key for external integrations |
| 🔜 | Notifications — email / webhook alerts for events (container down, login, etc.) |
| 🔜 | Themes — custom colour schemes and branding per installation |

### Other
| Status | Feature |
|---|---|
| 🔜 | Support Windows as a host level (Yes, it's possible I think) |
| 🔜 | Mobile app (Flutter or React Native) |
| 🔜 | Support ARM servers |
| 🔜 | Marketplace — pre-configured server templates for popular games and applications |
| 🔜 | Community plugins — allow third-party extensions for additional features and integrations |
| 🔜 | Localization — multi-language support with user-selectable languages |
| 🔜 | Accessibility — ensure the panel is usable with screen readers and keyboard navigation |

And much more! The roadmap is flexible and will evolve based on user feedback and new ideas.
You can make a pull request to add your own features or upvote existing ones in the [Issues](https://github.com/nestorchurin/yunexal-panel/issues)
Or help to implement features by joining the development on the [Discussions](https://github.com/nestorchurin/yunexal-panel/discussions) page.

---

## Installation

Download the latest binaries from the [Releases](https://github.com/nestorchurin/yunexal-panel/releases) page.

```bash
# 1. Download and extract
wget https://github.com/nestorchurin/yunexal-panel/releases/latest/download/yunexal-panel-linux-x86_64.tar.gz
tar -xzf yunexal-panel-linux-x86_64.tar.gz
cd yunex-release

# 2. Run the setup wizard
#    Auto-detects init system (systemd / OpenRC) and configures dependencies/services.
sudo ./yunexal-setup

# 3. Run
./yunexal-panel
```

The SQLite database (`panel.db`) and `volumes/` directory are created automatically on first run.

---

## Repository Status

Installer-image customization work is currently paused.

Repository scripts are currently kept as local-only files and are ignored by git, so they are not part of the GitHub-tracked source set.

The active and supported flow in this repository is direct host installation using release binaries and `yunexal-setup`.

---

## Reverse Proxy (nginx)

The panel itself speaks plain HTTP. For production use with a domain and HTTPS (e.g. `https://panel.yunexal.com`), you need a reverse proxy in front of it.

> **Important:** The WebSocket console (`wss://`) will **not work** unless the reverse proxy is configured to forward WebSocket upgrade headers. Without this, Firefox shows *"can't establish a connection to the server at wss://…"*.

The setup wizard (`yunexal-setup`) detects nginx and can generate this config automatically (Step 7). If you prefer to configure it manually:

**Config file path**

- `/etc/nginx/sites-available/yunexal-panel.conf` (Debian/Ubuntu — enable via symlink in `/etc/nginx/sites-enabled/`)
- `/etc/nginx/http.d/yunexal-panel.conf` (Alpine Linux)

```nginx
# HTTP → HTTPS redirect
server {
    listen 80;
    server_name panel.example.com;
    return 301 https://$host$request_uri;
}

server {
    listen 443 ssl;
    server_name panel.example.com;

    ssl_certificate     /etc/letsencrypt/live/panel.example.com/fullchain.pem;
    ssl_certificate_key /etc/letsencrypt/live/panel.example.com/privkey.pem;

    # WebSocket + HTTP reverse proxy (required for console)
    location / {
        proxy_pass http://127.0.0.1:3000;

        # These three lines are mandatory for WebSocket (wss://) to work:
        proxy_http_version 1.1;
        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection "upgrade";

        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
        proxy_read_timeout 3600s;
        proxy_send_timeout 3600s;
    }
}
```

Enable the site and reload nginx:

```bash
sudo nginx -t

sudo systemctl reload nginx   # Debian/Ubuntu
# or
sudo rc-service nginx reload  # Alpine Linux
```

To add SSL via Let's Encrypt:

```bash
sudo apt-get install -y certbot python3-certbot-nginx

sudo certbot --nginx -d panel.example.com
```

---

## Requirements

| Requirement | Notes | Minimum | Recommended |
|---|---|---|---|
| **OS** | Distribution for the panel | Linux with Docker and service manager support | Ubuntu LTS or Alpine Linux |
| **Init system** | Service management | systemd (Debian/Ubuntu) or OpenRC (Alpine) | Either — auto-detected by `yunexal-setup` |
| **Docker Engine** | Must be running; socket at `/var/run/docker.sock` | 24.0 | 29.0 + |
| **Helper shell image** | Pulled automatically by `yunexal-setup` | latest | latest |
| **RAM** | For the panel process | 64 MB if using minimal features with containers | 2 GB if using full features with containers |
| **CPU** | For the panel process | 1 vCPU | 2 vCPU |
| **Disk space** | For the panel binary, database, and volumes | 100 MB | 500 MB |
| **Filesystem** | For volume management and quotas | Any (quotas disabled) | **ext4 with `prjquota`** for per-container disk limits |
| **Ports** | Panel port (default: 3000) + container ports | 1 free port for the panel + container ports | Multiple free ports for the panel and containers |
| **Reverse proxy** | For production use with a domain and HTTPS | Optional (HTTP-only access) | Recommended (HTTPS + SSL with WebSocket support) |
| **Ethernet** | For network connectivity | 100 Mbps | 1 Gbps or higher |


> **Docker socket access** — add your user to the `docker` group:
> ```bash
> sudo usermod -aG docker $USER && newgrp docker
> ```

> **UFW sudo access** — to use per-port UFW blocking without a password prompt, add a sudoers rule
> (shown by the panel automatically if access is denied):
> ```bash
> echo "www-data ALL=(ALL) NOPASSWD: /usr/sbin/ufw" | sudo tee /etc/sudoers.d/yunexal-ufw
> ```

> **Disk quotas (ext4)** — to enable per-container disk limits, mount the volume partition with `prjquota`:
> ```
> # /etc/fstab entry example
> /dev/sdb1  /var/lib/yunexal-volumes  ext4  defaults,prjquota  0 2
> ```

---

## Configuration

All configuration is read from a `.env` file in the **same directory as the binary**, or from environment variables directly.
`yunexal-setup` generates this file interactively.

```dotenv
# Panel port (default: 3000)
PANEL_PORT=3000

# 128-character hex string (64 random bytes) — signs and encrypts session cookies.
# Changing this value invalidates all active sessions.
# Generate with:  openssl rand -hex 64
COOKIE_SECRET=<128 hex chars>

# Optional: service API key for third-party integrations
# Can also be set in Admin Panel › Panel Settings › API Key
YUNEXAL_API_KEY=<your-api-key>
```

Initial credentials are set by `yunexal-setup`.
Additional users and all panel settings are managed from the Admin Panel at `/admin`.

---

## Building from Source

Requires **Rust 1.78+** — install via [rustup.rs](https://rustup.rs).

```bash
git clone https://github.com/nestorchurin/yunexal-panel.git
cd yunexal-panel
cargo build --release

# Musl-only release artifacts for x86_64 (no glibc loader)
cargo build --release --target x86_64-unknown-linux-musl --bin yunexal-panel --bin yunexal-setup

# Interactive setup (Docker, .env, root user, optional OpenRC/systemd service)
sudo ./target/release/yunexal-setup

./target/release/yunexal-panel
```

---

## Project Structure

```
src/
├── main.rs               # Entry point, router, middleware
├── lib.rs                # Library crate (shared between binaries)
├── state.rs              # AppState — DB pool, Docker client, login limiter state
├── auth.rs               # Session helpers, admin guard, RBAC checks, rate limiter
├── compose.rs            # Docker Compose YAML parser
├── password.rs           # Argon2id hash / verify
├── host.rs               # Host command abstraction (init-system detection)
├── scheduler.rs          # Background Tokio cron runner (30 s tick)
├── db/
│   ├── mod.rs            # Schema init, migrations, seed defaults
│   ├── users.rs          # User CRUD (uid + nickname model)
│   ├── servers.rs        # Server CRUD + container-scoped access
│   ├── ports.rs          # Port mappings + UFW state
│   ├── images.rs         # Image ENV overrides
│   ├── audit.rs          # Audit log (immutable, global + per-server)
│   ├── settings.rs       # panel_settings key/value store
│   ├── roles.rs          # RBAC roles, permissions, tri-state policy
│   ├── sessions.rs       # User session records (device tracking)
│   └── schedules.rs      # Schedule + run history CRUD
├── docker/
│   ├── mod.rs            # Docker client, ContainerInfo
│   ├── containers.rs     # Lifecycle, attach, list
│   ├── stats.rs          # CPU/RAM/network/disk I/O stats
│   ├── images.rs         # Pull, delete, duplicate, ENV fetch
│   ├── files.rs          # Volume file operations; backup_dir_for_volume helper
│   ├── network.rs        # Bandwidth limiting (tc TBF), isolated networks
│   ├── edit.rs           # Inspect & recreate containers
│   └── quota.rs          # ext4 project-quota enforcement
├── bin/
│   └── setup.rs          # yunexal-setup: interactive wizard (systemd + OpenRC)
└── handlers/
    ├── mod.rs            # Router, embedded assets, track_requests middleware
    ├── auth.rs           # Login / logout / service-login
    ├── dashboard.rs      # Dashboard + server list fragment + device sessions API
    ├── servers.rs        # Console, Settings, Stats, lifecycle, ENV update, Factory Reset, Members API, Audit API
    ├── files.rs          # File manager API (upload, extract, edit, safe path resolution)
    ├── network.rs        # Networking + port / bandwidth / UFW API
    ├── backups.rs        # Backups page + create / download / delete / restore API
    ├── schedules.rs      # Schedules page + CRUD / toggle / run-now / run history API
    ├── create.rs         # Container creation (local-first, Dockerfile build, quota preflight)
    ├── admin.rs          # Admin panel — users, images, containers, roles, storage, panel settings
    ├── ws.rs             # WebSocket console
    └── templates.rs      # Askama template structs

templates/                # Askama HTML templates — compiled into binary
static/                   # CSS, JS, icons — compiled into binary via rust-embed
```

---

## Tech Stack

| Layer | Technology |
|---|---|
| Web framework | [Axum](https://github.com/tokio-rs/axum) 0.8 |
| Async runtime | [Tokio](https://tokio.rs) |
| Docker SDK | [Bollard](https://github.com/fussybeaver/bollard) 0.21 |
| Database | SQLite via [SQLx](https://github.com/launchbadge/sqlx) 0.8 (WAL mode) |
| HTTP client | [reqwest](https://github.com/seanmonstar/reqwest) |
| Templates | [Askama](https://github.com/djc/askama) 0.16 — compiled into binary |
| Static assets | [rust-embed](https://github.com/pyros2097/rust-embed) — compiled into binary |
| Password hashing | [Argon2](https://github.com/RustCrypto/password-hashes) (Argon2id) |
| Session cookies | [axum-extra](https://docs.rs/axum-extra) private cookies (AES-GCM) |
| Concurrent maps | [DashMap](https://github.com/xacrimon/dashmap) — L7 per-IP counters |
| Frontend | Bootstrap 5 · vanilla JS |
| Terminal | [xterm.js](https://xtermjs.org) with FitAddon |
| Charts | [Chart.js](https://www.chartjs.org) |
| Code editors | [Ace](https://ace.c9.io) (file editor) · [Monaco](https://microsoft.github.io/monaco-editor/) (YAML / compose) |

---

## License

[MIT](LICENSE)
