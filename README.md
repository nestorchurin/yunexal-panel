# Yunexal Panel

> **v0.1.0** — Beta  
> A self-hosted web panel for managing Docker game-server containers.

Built with **Rust + Axum**, **HTMX**, **SQLite**, and **Bollard** (Docker SDK).

---

## Installation (pre-built binary)

Download the latest binary from the [Releases](https://github.com/nestorchurin/yunexal-panel/releases) page.

```bash
# 1. Download and extract
wget https://github.com/nestorchurin/yunexal-panel/releases/download/v0.1.0/yunexal-panel-v0.1.0-linux-x86_64.tar.gz
tar -xzf yunexal-panel-v0.1.0-linux-x86_64.tar.gz
cd yunex-release

# 2. Generate .env (interactive — sets admin credentials + cookie secret)
./setup.sh

# 3. Run
./yunexal-panel
```

That's it. No Rust, no Cargo, no extra files needed — templates and static assets are embedded in the binary. The SQLite database and volume directories are created automatically on first run.

---

## Requirements

| Requirement | Notes |
|---|---|
| **Docker Engine** 24.0+ | Must be running |
| **Docker image `alpine`** | Pulled automatically by `setup.sh` |
| **OS** | Linux x86_64 |
| **RAM** | 256 MB for the panel process |

> **Docker socket access** — add your user to the `docker` group:
> ```bash
> sudo usermod -aG docker $USER && newgrp docker
> ```

---

## Configuration

All configuration is done via `.env` in the **same directory as the binary**.  
Use `setup.sh` to generate it, or create it manually:

```dotenv
# Admin credentials — applied to the DB on every startup
PANEL_USERNAME=admin
PANEL_PASSWORD=your_secure_password

# 128-char hex string (64 random bytes) — signs session cookies.
# Generate with:  openssl rand -hex 64
COOKIE_SECRET=<128 hex chars>
```

> Changing `PANEL_PASSWORD` takes effect on next restart.  
> Changing `COOKIE_SECRET` invalidates all active sessions.

Alternatively pass values as environment variables without a `.env` file:
```bash
PANEL_USERNAME=admin PANEL_PASSWORD=secret COOKIE_SECRET=<128hex> ./yunexal-panel
```

---

## Features

### Dashboard
- Live list of all registered containers with CPU, RAM, and uptime stats
- Auto-refreshing server cards (polled every 5 s)
- Quick status badge: Running / Stopped / Error

### Server Management
- **Start / Stop / Restart / Kill** containers via Docker API
- **Rename** a server (updates SQLite + display name)
- **Delete** a server — stops the container, wipes the volume directory (via Alpine helper container), and removes the DB record

### Real-time Console
- WebSocket terminal attached directly to the Docker container TTY
- Live log streaming with ANSI colour support
- Send commands to `stdin` from the browser

### File Manager
- Browse volume directories with folder/file icons and a breadcrumb bar
- **Edit** text files in a full-screen code editor
- **Create** new files and directories from the browser
- **Rename** files and directories (right-click context menu)
- **Copy / Paste** files and directories
- **Delete** files and directories (with confirmation)
- **Drag-and-drop upload** with per-file progress bar (supports large files, streamed directly to disk)
- All write operations use an Alpine Docker helper container to bypass root-owned volume permissions

### Networking
- View all port bindings (host ↔ container)
- **Add / remove** port mappings (admin only)
- **Tag** ports with a friendly label (e.g. `Minecraft`, `RCON`)
- **Enable / disable** individual port mappings
- **Bandwidth limiting** via `tc` inside the container (admin only, set in Mbit/s)

### Server Creation
- Create a new Docker container from a Docker Hub image name
- Optional Docker Compose-style YAML config (image, ports, environment, restart policy, CPU/RAM limits)
- Auto-detects environment variables declared in the image (`EULA`, `MEMORY`, etc.)
- Assign an owner user to the server

### Admin Panel
- **User management**: create users, set passwords, delete users
- **Role-based access**: `admin` vs `user` roles
- Admins can edit any container (image, name, owner) after creation
- **Stop all** containers at once
- **Change own password**

### Authentication
- Session-based login with encrypted private cookies (Argon2 password hashing)
- Route-level middleware: unauthenticated → redirect to `/login`; non-admin on admin routes → 403

---

## Building from Source

```bash
git clone https://github.com/nestorchurin/yunexal-panel.git
cd yunexal-panel
./setup.sh          # generate .env
cargo build --release
./target/release/yunexal-panel
```

Requires Rust 1.78+ — install via [rustup.rs](https://rustup.rs).

---

## Project Structure

```
src/
├── main.rs               # Entry point, router setup
├── state.rs              # AppState (DB pool, Docker client, cookie key)
├── auth.rs               # Session middleware, admin helpers
├── db.rs                 # SQLite schema & queries
├── docker.rs             # Bollard wrappers (start/stop/stats/volumes/bandwidth)
├── compose.rs            # Docker Compose YAML parser
└── handlers/
    ├── mod.rs            # Router (public / protected / admin_only), embedded assets
    ├── auth.rs           # Login / logout
    ├── dashboard.rs      # Dashboard + server list fragment
    ├── servers.rs        # Console, Files, Settings, Stats, lifecycle
    ├── files.rs          # File manager API
    ├── network.rs        # Networking + port/bandwidth API
    ├── create.rs         # New server creation
    ├── admin.rs          # Admin panel
    ├── ws.rs             # WebSocket console
    └── templates.rs      # Askama template structs
templates/                # Embedded at compile time (Askama)
static/                   # Embedded at compile time (rust-embed)
setup.sh                  # Interactive .env generator
```

---

## Tech Stack

| Layer | Technology |
|---|---|
| Web framework | [Axum](https://github.com/tokio-rs/axum) 0.8 |
| Async runtime | [Tokio](https://tokio.rs) |
| Docker SDK | [Bollard](https://github.com/fussybeaver/bollard) |
| Database | SQLite via [SQLx](https://github.com/launchbadge/sqlx) |
| Templates | [Askama](https://github.com/djc/askama) — compiled into binary |
| Static assets | [rust-embed](https://github.com/pyros2097/rust-embed) — compiled into binary |
| Frontend | [HTMX](https://htmx.org) + Bootstrap 5 + vanilla JS |
| Auth / cookies | [axum-extra](https://docs.rs/axum-extra) private cookies + Argon2 |

---

## License

MIT