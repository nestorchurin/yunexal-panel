# Yunexal Panel

> **v0.2.3** — Self-hosted web panel for managing Docker game-server containers.

Built with **Rust + Axum**, **SQLite**, and **Bollard** (Docker SDK).  
Templates and static assets are embedded into a single binary — no external files needed.

---

## Installation

Download the latest binary from the [Releases](https://github.com/nestorchurin/yunexal-panel/releases) page.

```bash
# 1. Download and extract
wget https://github.com/nestorchurin/yunexal-panel/releases/latest/download/yunexal-panel-linux-x86_64.tar.gz
tar -xzf yunexal-panel-linux-x86_64.tar.gz
cd yunex-release

# 2. Run the setup wizard (interactive — installs Docker, sets admin credentials, writes .env)
sudo ./yunexal-setup

# 3. Run
./yunexal-panel
```

The SQLite database and volume directories are created automatically on first run.

---

## Requirements

| Requirement | Notes |
|---|---|
| **Docker Engine** 24.0+ | Must be running |
| **Docker image `alpine`** | Pulled automatically by `yunexal-setup` |
| **OS** | Linux x86_64 |
| **RAM** | ~256 MB for the panel process |

> **Docker socket access** — add your user to the `docker` group:
> ```bash
> sudo usermod -aG docker $USER && newgrp docker
> ```

---

## Configuration

All configuration is done via `.env` in the **same directory as the binary**.  
Use `yunexal-setup` to generate it, or create it manually:

```dotenv
# Panel port (default: 3000)
PANEL_PORT=3000

# 128-char hex string (64 random bytes) — signs session cookies.
# Generate with:  openssl rand -hex 64
COOKIE_SECRET=<128 hex chars>
```

> Changing `COOKIE_SECRET` invalidates all active sessions.

Initial admin credentials are set interactively by `yunexal-setup`.  
Additional users are created from the Admin Panel.

Alternatively pass values as environment variables without a `.env` file:
```bash
PANEL_PORT=3000 COOKIE_SECRET=<128hex> ./yunexal-panel
```

---

## Features

### Dashboard
- Live list of all managed containers with CPU, RAM, network I/O, and uptime
- **In-place updates** — server cards refresh without DOM re-creation (no animation flicker)
- Auto-polling every 5 s; quick status badge: Running / Stopped / Error
- Change own password from the dashboard

### Server Management
- **Start / Stop / Restart / Kill** containers via Docker API
- **Rename** a server (updates SQLite + display name)
- **Delete** a server — stops the container, wipes the volume directory (via Alpine helper), removes the DB record
- Only the server owner (or admin) can manage a server — enforced both on backend and frontend

### Real-time Console
- WebSocket terminal attached directly to the Docker container TTY (xterm.js)
- Live log streaming with full ANSI colour support
- Send commands to `stdin` from the browser
- Live CPU / RAM / Network charts (Chart.js, 1 s polling)
- **Per-server DNS panel** — view and manage DNS records linked to this server

### File Manager
- Browse volume directories with folder/file icons and a breadcrumb bar
- **Edit** text files in a full-screen code editor (Ace editor)
- **Create** new files and directories
- **Rename** files and directories (right-click context menu)
- **Copy / Paste** files and directories
- **Delete** files and directories (with confirmation)
- **Drag-and-drop upload** with per-file progress bar (supports large files, streamed to disk)
- All write operations use an Alpine Docker helper container to handle root-owned volume permissions
- Path traversal protection — validated on the backend

### Networking
- View all port bindings (host ↔ container) with protocol (TCP / UDP / TCP+UDP)
- **Add / Remove** port mappings (admin only) with port conflict pre-check
- **Tag** ports with a friendly label (e.g. `Minecraft`, `RCON`)
- **Enable / Disable** individual port mappings
- **Bandwidth limiting** via Linux `tc` TBF qdisc (admin only, set in Mbit/s) — persisted and reapplied on restart

### Server Creation (admin only)
- Create a new Docker container from any Docker Hub or local image
- **Local images datalist** — autocomplete from locally available images
- Full **Docker Compose-style YAML** config via Monaco editor (live GUI ↔ YAML sync):
  - `image`, `ports`, `environment`, `volumes`, `restart`
  - `cpus` (fractional cores), `mem_limit` (MB/GB), `disk_limit` (GB)
- **Port bindings**: dynamic rows with host/container port and protocol selector (TCP / UDP / TCP+UDP)
- **"Fetch ENV"** — auto-detects environment variables from Docker image metadata
- **Image ENV overrides** — admin-configured DB defaults applied on top of image defaults
- **Port conflict detection** — pre-flight TCP/UDP bind check before creation
- **Duplicate name check** — unique server names enforced
- **Owner assignment** — assign container to any user
- **DNS/SRV auto-record** — optionally auto-create an SRV record on container creation (and auto-delete on removal)

### DNS Management (admin only)

Full multi-provider DNS management with 5 supported providers:

| Provider | Zones | Record CRUD | DDNS | Proxy toggle |
|---|---|---|---|---|
| **Cloudflare** | Full API zone list | Full (all types) | Yes | Yes (orange cloud) |
| **GoDaddy** | Active domains | Full | Yes | — |
| **DuckDNS** | Single domain | — | Yes | — |
| **Namecheap** | Single domain | — | Yes | — |
| **Generic** | Single domain | — | Yes (templated URL) | — |

- **Record types**: A, AAAA, CNAME, MX, TXT, SRV, NS, CAA, and more
- **A + SRV auto-records** — auto-create DNS records when a server is created, auto-delete on removal
- **DDNS** — per-record toggle with configurable interval; auto-updates A records with the server's public IP
- **Provider sync** — pull live records from the provider API and update local DB
- **Public IP detection** — cascading fallback: `api.ipify.org` → `api4.my-ip.io` → `checkip.amazonaws.com`
- **Cloudflare proxy toggle** — one-click orange cloud on/off
- **`yunexal.managed=true` tag** — marks records managed by the panel on the provider side
- **Provider test** — per-provider connectivity verification
- **Credential redaction** — API keys shown as `••••` in the UI; partial updates merge with stored values
- **Container-linked records** — records can be bound to a specific server, visible in the console DNS panel
- **TTL presets** — Auto / 1 min / 5 min / 1 hour / 1 day / Custom
- **Type-coloured badges** + search + filter chips in the admin DNS table

### Admin Panel
- **Tabs**: Overview, Containers, Images, Users, DNS, Audit Log
- **User management**: create users, set passwords, delete users
- **Role-based access**: `admin` vs `user` roles
- **Container management**: edit any container (image, name, owner); stop all at once
- **Image management**: in-place refresh, pull, delete, duplicate, ENV override editor
- **DNS management**: providers, records, sync, DDNS (see above)
- **In-place updates** — all admin tabs poll and update without full page reload
- **Change own password**
- **Update checker** — check for new stable releases or unstable branch commits from the admin overview; one-click download & install with automatic restart (requires systemd)

### Authentication & Security
- Session-based login with encrypted private cookies
- **Argon2id** password hashing with random salt
- Route-level middleware: unauthenticated → redirect to `/login`; non-admin on admin routes → 403
- Ownership checks on all server operations (backend-enforced)
- Path traversal protection on file manager endpoints
- Admin-only access for port manipulation and bandwidth control
- XSS protection: Askama auto-escaping in templates + `escHtml()` / `escAttr()` in JavaScript
- All `fetch` calls hardened with `credentials: 'same-origin'`
- **Rate limiting** — 5 failed login attempts per IP → 60 s lockout
- **Security headers** — CSP, X-Frame-Options, HSTS, Referrer-Policy, Permissions-Policy
- **SameSite=Strict** session cookies prevent CSRF
- **Custom error pages** — no framework fingerprinting on 404/500

### UI / UX
- Responsive **Bootstrap 5** layout — works on desktop and mobile
- **AMOLED mode** — pure-black theme for mobile OLED screens, auto-enables fullscreen
- **Mobile optimizations**: `visibilitychange` polling resume, 7-day session cookie, tap-delay removed
- **PWA support** — `manifest.json` + service worker for installable web app
- **HTMX** for partial page updates without full reloads

---

## Building from Source

```bash
git clone https://github.com/nestorchurin/yunexal-panel.git
cd yunexal-panel
cargo build --release
sudo ./target/release/yunexal-setup  # interactive wizard: Docker, .env, admin user, systemd
./target/release/yunexal-panel
```

Requires **Rust 1.78+** — install via [rustup.rs](https://rustup.rs).

---

## Project Structure

```
src/
├── main.rs               # Entry point, router, config
├── lib.rs                # Library crate (exposes modules to binaries)
├── state.rs              # AppState (DB pool, Docker client, cookie key)
├── auth.rs               # Session middleware, admin guard
├── db/
│   ├── mod.rs            # SQLite schema, init_db, seed_root_user
│   ├── users.rs          # User CRUD
│   ├── servers.rs        # Server CRUD
│   ├── ports.rs          # Port mappings
│   ├── dns.rs            # DNS records & providers
│   └── images.rs         # Image ENV overrides
├── docker/
│   ├── mod.rs            # Docker client, ContainerInfo
│   ├── containers.rs     # Lifecycle, attach, list
│   ├── stats.rs          # CPU/RAM/network stats
│   ├── images.rs         # Pull, delete, duplicate, ENV fetch
│   ├── files.rs          # Volume file operations
│   ├── network.rs        # Bandwidth limiting, isolated networks
│   └── edit.rs           # Inspect & recreate containers
├── dns.rs                # DNS provider API clients (Cloudflare, GoDaddy, DuckDNS, Namecheap, Generic)
├── compose.rs            # Docker Compose YAML parser
├── password.rs           # Argon2id hash / verify
├── bin/
│   └── setup.rs          # yunexal-setup: interactive wizard (Docker, .env, admin, systemd)
└── handlers/
    ├── mod.rs            # Router (public / protected / admin_only), embedded assets
    ├── auth.rs           # Login / logout
    ├── dashboard.rs      # Dashboard + server list fragment
    ├── servers.rs        # Console, Files, Settings, Stats, lifecycle
    ├── files.rs          # File manager API
    ├── network.rs        # Networking + port/bandwidth API
    ├── create.rs         # New server creation
    ├── admin.rs          # Admin panel (users, images, containers)
    ├── dns.rs            # DNS management API (providers, records, DDNS, sync)
    ├── ws.rs             # WebSocket console
    └── templates.rs      # Askama template structs
templates/                # Embedded at compile time (Askama)
static/                   # Embedded at compile time (rust-embed)
```

---

## Tech Stack

| Layer | Technology |
|---|---|
| Web framework | [Axum](https://github.com/tokio-rs/axum) 0.8 |
| Async runtime | [Tokio](https://tokio.rs) |
| Docker SDK | [Bollard](https://github.com/fussybeaver/bollard) |
| Database | SQLite via [SQLx](https://github.com/launchbadge/sqlx) (WAL mode) |
| DNS client | [reqwest](https://github.com/seanmonstar/reqwest) (Cloudflare, GoDaddy, DuckDNS, Namecheap APIs) |
| Templates | [Askama](https://github.com/djc/askama) — compiled into binary |
| Static assets | [rust-embed](https://github.com/pyros2097/rust-embed) — compiled into binary |
| Password hashing | [Argon2](https://github.com/RustCrypto/password-hashes) (Argon2id) |
| Frontend | Bootstrap 5 + [HTMX](https://htmx.org) + vanilla JS |
| Terminal | [xterm.js](https://xtermjs.org) |
| Charts | [Chart.js](https://www.chartjs.org) |
| Code editors | [Ace](https://ace.c9.io) (file editor) · [Monaco](https://microsoft.github.io/monaco-editor/) (server creation YAML) |
| Auth / cookies | [axum-extra](https://docs.rs/axum-extra) private cookies |

---

## License

[MIT](LICENSE)
