# Yunexal Panel

> **v0.1.0** — Beta  
> A self-hosted web panel for managing Docker game-server containers.

Built with **Rust + Axum**, **HTMX**, **SQLite**, and **Bollard** (Docker SDK).

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
- **Edit** text files in a full-screen code editor (Monaco-style textarea)
- **Create** new files and directories from the browser
- **Rename** files and directories (right-click context menu or rename modal)
- **Copy** files/directories (right-click → Copy, navigate, Paste)
- **Delete** files and directories (with confirmation)
- **Drag-and-drop upload** — drop files onto the panel; progress bar shows per-file and overall progress
- All file operations fall back to an Alpine Docker helper container to bypass root-owned volume permissions

### Networking
- View all port bindings (host ↔ container)
- **Add / remove** port mappings (admin only)
- **Tag** ports with a friendly label (e.g. `Minecraft`, `RCON`)
- **Enable / disable** individual port mappings
- **Bandwidth limiting** via `tc` inside the container (admin only, set in Mbit/s)

### Server Creation
- Create a new Docker container from a Docker Hub image name
- Optional Docker Compose-style YAML config field (image, ports, environment, restart policy, CPU/RAM limits)
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

## Requirements

| Requirement | Minimum version |
|---|---|
| **Rust** (stable) | 1.78+ |
| **Cargo** | bundled with Rust |
| **Docker Engine** | 24.0+ |
| **Docker image `alpine`** | latest (pulled automatically by `setup.sh`) |
| **OS** | Linux (x86_64 or arm64); macOS works for dev |
| **RAM** | 256 MB for the panel process |
| **Disk** | Space for server volumes + SQLite DB |

> **Docker socket access** — the panel process must be able to reach the Docker daemon.  
> Either run as root or add your user to the `docker` group:
> ```bash
> sudo usermod -aG docker $USER && newgrp docker
> ```

---

## Quick Start

```bash
# 1. Clone
git clone https://github.com/nestorchurin/yunexal-panel.git
cd yunexal-panel

# 2. Generate .env (interactive — sets admin credentials + cookie secret)
./setup.sh

# 3. Build and run
cargo run

# 4. Open http://localhost:3000
```

---

## Configuration

All configuration is done via `.env` (never committed to git).  
Use `setup.sh` to generate it, or create it manually:

```dotenv
# Admin credentials — applied to the DB on every startup
PANEL_USERNAME=admin
PANEL_PASSWORD=your_secure_password

# 128-char hex string (64 random bytes) — signs session cookies.
# Generate with:  openssl rand -hex 64
COOKIE_SECRET=<128 hex chars>
```

> Changing `COOKIE_SECRET` invalidates all active sessions.

---

## Project Structure

```
├── src/
│   ├── main.rs               # Entry point, Axum router setup
│   ├── state.rs              # Shared AppState (DB pool, Docker client, cookie key)
│   ├── auth.rs               # Session middleware, is_admin helpers
│   ├── db.rs                 # SQLite schema, queries (users, servers, ports)
│   ├── docker.rs             # Bollard wrappers (start/stop/stats/volumes/bandwidth)
│   ├── compose.rs            # Docker Compose YAML parser
│   └── handlers/
│       ├── mod.rs            # Router definition (public / protected / admin_only)
│       ├── auth.rs           # Login / logout handlers
│       ├── dashboard.rs      # Dashboard + server list fragment
│       ├── servers.rs        # Console, Files, Settings, Stats, lifecycle actions
│       ├── files.rs          # File manager API (list/edit/create/rename/copy/delete/upload)
│       ├── network.rs        # Networking page + port/bandwidth API
│       ├── create.rs         # New server form + creation handler
│       ├── admin.rs          # Admin panel (user/server management)
│       ├── ws.rs             # WebSocket console handler
│       └── templates.rs      # Askama template structs
├── templates/                # Askama HTML templates
├── static/                   # CSS, JS, icons
├── volumes/                  # Docker volume mounts (git-ignored)
├── yunexal.db                # SQLite database (git-ignored)
├── .env                      # Secrets (git-ignored)
├── setup.sh                  # Interactive environment generator
└── Cargo.toml
```

---

## Tech Stack

| Layer | Technology |
|---|---|
| Web framework | [Axum](https://github.com/tokio-rs/axum) 0.8 |
| Async runtime | [Tokio](https://tokio.rs) |
| Docker SDK | [Bollard](https://github.com/fussybeaver/bollard) |
| Database | SQLite via [SQLx](https://github.com/launchbadge/sqlx) |
| Templates | [Askama](https://github.com/djc/askama) (type-safe Jinja2) |
| Frontend | [HTMX](https://htmx.org) + Bootstrap 5 + vanilla JS |
| Auth / cookies | [axum-extra](https://docs.rs/axum-extra) private cookies + Argon2 |

---

## License

MIT
