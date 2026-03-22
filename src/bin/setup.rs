// yunexal-setup — interactive setup wizard (replaces setup.sh)
// Compiled as a separate binary alongside the main yunexal-panel server.

use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};
use anyhow::{Context, Result};
use yunexal_panel::{db, password};

// ── Colour / print helpers ────────────────────────────────────────────────────

macro_rules! info {
    ($($t:tt)*) => { println!("\x1b[34m[INFO]\x1b[0m  {}", format!($($t)*)) };
}
macro_rules! ok {
    ($($t:tt)*) => { println!("\x1b[32m[OK]\x1b[0m    {}", format!($($t)*)) };
}
macro_rules! warn {
    ($($t:tt)*) => { println!("\x1b[33m[WARN]\x1b[0m  {}", format!($($t)*)) };
}
macro_rules! header {
    ($($t:tt)*) => { println!("\n\x1b[1m\x1b[34m══ {} ══\x1b[0m", format!($($t)*)) };
}

// ── I/O helpers ───────────────────────────────────────────────────────────────

/// Prompt with an optional default. Returns entered text or default.
fn prompt(question: &str, default: Option<&str>) -> Result<String> {
    let default_hint = default.map(|d| format!(" [{}]", d)).unwrap_or_default();
    print!("\x1b[34m{}{}\x1b[0m: ", question, default_hint);
    io::stdout().flush()?;
    let line = read_line()?;
    if line.is_empty() {
        Ok(default.unwrap_or("").to_string())
    } else {
        Ok(line)
    }
}

/// Yes/No prompt. Returns `true` for `y`, `false` otherwise. `default_yes` controls
/// what happens when the user presses Enter without input.
fn prompt_yn(question: &str, default_yes: bool) -> Result<bool> {
    let hint = if default_yes { "Y/n" } else { "y/N" };
    print!("\x1b[33m{} [{}]\x1b[0m: ", question, hint);
    io::stdout().flush()?;
    let line = read_line()?.to_lowercase();
    if line.is_empty() {
        Ok(default_yes)
    } else {
        Ok(line.starts_with('y'))
    }
}

/// Read a password without echoing it to the terminal.
fn prompt_password(question: &str) -> Result<String> {
    print!("\x1b[34m{}\x1b[0m: ", question);
    io::stdout().flush()?;
    rpassword::read_password().context("Failed to read password")
}

fn read_line() -> Result<String> {
    let stdin = io::stdin();
    let mut line = String::new();
    stdin.lock().read_line(&mut line)?;
    Ok(line.trim_end_matches(['\n', '\r']).to_string())
}

// ── Root check ────────────────────────────────────────────────────────────────

fn check_root() -> bool {
    std::process::Command::new("id")
        .arg("-u")
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim() == "0")
        .unwrap_or(false)
}

/// Returns the real invoking user (strips sudo).
fn real_user() -> String {
    std::env::var("SUDO_USER").unwrap_or_else(|_| {
        std::process::Command::new("logname")
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .unwrap_or_else(|_| "root".to_string())
    })
}

// ── Secret generation ─────────────────────────────────────────────────────────

/// Generates a 64-byte random hex string using /dev/urandom.
fn gen_secret() -> Result<String> {
    use std::io::Read;
    let mut buf = [0u8; 64];
    std::fs::File::open("/dev/urandom")
        .context("Failed to open /dev/urandom")?
        .read_exact(&mut buf)
        .context("Failed to read /dev/urandom")?;
    Ok(buf.iter().map(|b| format!("{:02x}", b)).collect())
}

// ── Main ──────────────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let opt_reset          = args.iter().any(|a| a == "--reset");
    let opt_non_interactive = args.iter().any(|a| a == "--non-interactive");

    if args.iter().any(|a| a == "--help" || a == "-h") {
        println!("Usage: yunexal-setup [--reset] [--non-interactive]");
        println!();
        println!("  --reset              Wipe DB and .env without prompting");
        println!("  --non-interactive    Read credentials from PANEL_USERNAME / PANEL_PASSWORD env vars");
        return Ok(());
    }

    // ── Header ────────────────────────────────────────────────────────────────
    println!("\n\x1b[1m╔══════════════════════════════════════════╗\x1b[0m");
    println!("\x1b[1m║      Yunexal Panel — Setup Wizard        ║\x1b[0m");
    println!("\x1b[1m╚══════════════════════════════════════════╝\x1b[0m\n");

    if !check_root() {
        eprintln!("\x1b[31m[ERROR]\x1b[0m This tool must be run as root (use sudo).");
        std::process::exit(1);
    }

    let real_user = real_user();
    let script_dir = std::env::current_dir()
        .context("Failed to determine working directory")?;

    // ── Step 1: Reset ─────────────────────────────────────────────────────────
    header!("Step 1: Reset");

    let do_reset = if opt_reset {
        true
    } else {
        prompt_yn("Wipe existing database and .env?", false)?
    };

    if do_reset {
        step_reset(&script_dir).await;
    } else {
        info!("Skipping reset.");
    }

    // ── Step 2: Docker ────────────────────────────────────────────────────────
    header!("Step 2: Docker");
    step_docker(&real_user).await?;

    // ── Step 3: .env ─────────────────────────────────────────────────────────
    header!("Step 3: Environment (.env)");
    step_env(&script_dir, &real_user)?;

    // ── Step 4: Admin user ───────────────────────────────────────────────────
    header!("Step 4: Admin user");
    step_admin_user(opt_non_interactive, &script_dir, &real_user).await?;

    // ── Step 5: Import containers ─────────────────────────────────────────────
    header!("Step 5: Import Docker containers");
    step_import_containers(&script_dir).await;

    // ── Step 6: systemd service ───────────────────────────────────────────────
    header!("Step 6: systemd service");
    step_systemd(&script_dir, &real_user)?;

    // ── Summary ───────────────────────────────────────────────────────────────
    let panel_port = read_env_port(&script_dir).unwrap_or_else(|| "3000".to_string());
    println!();
    println!("\x1b[1m\x1b[32m╔══════════════════════════════════════════╗\x1b[0m");
    println!("\x1b[1m\x1b[32m║            Setup complete!               ║\x1b[0m");
    println!("\x1b[1m\x1b[32m╚══════════════════════════════════════════╝\x1b[0m");
    println!();
    println!("  Panel URL  : \x1b[1mhttp://localhost:{}\x1b[0m", panel_port);
    println!("  Service    : \x1b[1msystemctl status yunexal-panel\x1b[0m");
    println!("  Logs       : \x1b[1mjournalctl -u yunexal-panel -f\x1b[0m");
    println!();

    Ok(())
}

// ── Step implementations ──────────────────────────────────────────────────────

async fn step_reset(dir: &Path) {
    info!("Stopping yunexal-panel service (if running)…");
    let _ = std::process::Command::new("systemctl")
        .args(["stop", "yunexal-panel"])
        .status();

    for f in &["yunexal.db", "yunexal.db-shm", "yunexal.db-wal", ".env"] {
        let p = dir.join(f);
        if p.exists() {
            let _ = std::fs::remove_file(&p);
            info!("Removed {}", f);
        }
    }
    ok!("Reset complete.");
}

async fn step_docker(real_user: &str) -> Result<()> {
    // Check if docker is installed
    let docker_version = std::process::Command::new("docker")
        .args(["version", "--format", "{{.Server.Version}}"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string());

    if let Some(ref ver) = docker_version {
        info!("Docker detected: v{}", ver);

        // Check for latest version via GitHub API
        let latest = fetch_latest_docker_version().await;
        if let Some(ref latest_ver) = latest {
            if latest_ver != ver {
                warn!("Newer Docker available: v{} (installed: v{})", latest_ver, ver);
                if prompt_yn("Upgrade Docker now?", false)? {
                    run_docker_install()?;
                    ok!("Docker upgraded to v{}.", latest_ver);
                } else {
                    info!("Skipping Docker upgrade.");
                }
            } else {
                ok!("Docker is up-to-date (v{}).", ver);
            }
        } else {
            ok!("Docker v{} (could not check for updates).", ver);
        }
    } else {
        info!("Docker not found. Installing latest stable Docker…");
        run_docker_install()?;

        // Add real user to docker group
        let _ = std::process::Command::new("usermod")
            .args(["-aG", "docker", real_user])
            .status();

        // Enable + start Docker daemon
        let _ = std::process::Command::new("systemctl").args(["enable", "docker"]).status();
        let _ = std::process::Command::new("systemctl").args(["start", "docker"]).status();
        ok!("Docker installed.");
    }

    // Ensure Docker daemon is running
    let running = std::process::Command::new("systemctl")
        .args(["is-active", "--quiet", "docker"])
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if !running {
        info!("Starting Docker daemon…");
        let _ = std::process::Command::new("systemctl").args(["start", "docker"]).status();
    }

    // Quick reachability test
    let reachable = std::process::Command::new("docker")
        .args(["pull", "alpine:latest", "-q"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);

    if reachable {
        ok!("Docker daemon reachable.");
    } else {
        warn!("Docker pull test failed — verify Docker is working before continuing.");
    }

    Ok(())
}

fn run_docker_install() -> Result<()> {
    let status = std::process::Command::new("bash")
        .args(["-c", "curl -fsSL https://get.docker.com | sh"])
        .status()
        .context("Failed to run Docker install script")?;
    if !status.success() {
        anyhow::bail!("Docker installation script failed");
    }
    Ok(())
}

async fn fetch_latest_docker_version() -> Option<String> {
    let resp = reqwest::get("https://api.github.com/repos/moby/moby/releases/latest")
        .await.ok()?;
    let json: serde_json::Value = resp.json().await.ok()?;
    let tag = json["tag_name"].as_str()?;
    Some(tag.trim_start_matches('v').to_string())
}

fn step_env(dir: &Path, real_user: &str) -> Result<()> {
    let env_path = dir.join(".env");

    let write_env = |port: &str| -> Result<()> {
        let secret = gen_secret()?;
        let now = {
            let secs = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            // Simple ISO-8601 from epoch (UTC, no leap-second correction)
            let s = secs % 60;
            let m = (secs / 60) % 60;
            let h = (secs / 3600) % 24;
            let days = secs / 86400;
            fn civil(d: u64) -> (u64, u64, u64) {
                let z = d + 719468;
                let era = z / 146097;
                let doe = z - era * 146097;
                let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
                let y = yoe + era * 400;
                let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
                let mp = (5 * doy + 2) / 153;
                let dd = doy - (153 * mp + 2) / 5 + 1;
                let mo = if mp < 10 { mp + 3 } else { mp - 9 };
                let y = if mo <= 2 { y + 1 } else { y };
                (y, mo, dd)
            }
            let (y, mo, dd) = civil(days);
            format!("{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z", y, mo, dd, h, m, s)
        };

        let content = format!(
            "# Yunexal Panel — auto-generated by yunexal-setup on {now}\n\
             PANEL_PORT={port}\n\
             COOKIE_SECRET={secret}\n\
             DATABASE_URL=sqlite:yunexal.db\n"
        );
        std::fs::write(&env_path, content).context("Failed to write .env")?;

        // Set ownership and permissions
        let _ = std::process::Command::new("chown")
            .args([&format!("{}:{}", real_user, real_user), env_path.to_str().unwrap()])
            .status();
        let _ = std::process::Command::new("chmod").args(["600", env_path.to_str().unwrap()]).status();

        ok!(".env written (port {}, fresh COOKIE_SECRET).", port);
        Ok(())
    };

    if env_path.exists() {
        warn!(".env already exists.");
        if prompt_yn("Overwrite .env with a new secret?", false)? {
            let port = prompt("Panel port", Some("3000"))?;
            write_env(&port)?;
        } else {
            info!("Keeping existing .env.");
        }
    } else {
        let port = prompt("Panel port", Some("3000"))?;
        write_env(&port)?;
    }

    Ok(())
}

async fn step_admin_user(non_interactive: bool, dir: &Path, real_user: &str) -> Result<()> {
    let (username, pass) = if non_interactive {
        let u = std::env::var("PANEL_USERNAME")
            .context("PANEL_USERNAME env var required with --non-interactive")?;
        let p = std::env::var("PANEL_PASSWORD")
            .context("PANEL_PASSWORD env var required with --non-interactive")?;
        (u, p)
    } else {
        let username = loop {
            let u = prompt("Admin username", None)?;
            if !u.is_empty() { break u; }
            eprintln!("\x1b[31m[ERROR]\x1b[0m Username cannot be empty.");
        };

        let pass = loop {
            let p = prompt_password("Admin password (min 8 chars)")?;
            if p.len() < 8 {
                eprintln!("\x1b[31m[ERROR]\x1b[0m Password too short (minimum 8 characters).");
                continue;
            }
            let p2 = prompt_password("Confirm password")?;
            if p != p2 {
                eprintln!("\x1b[31m[ERROR]\x1b[0m Passwords do not match.");
                continue;
            }
            break p;
        };

        (username, pass)
    };

    let pool = db::init_db().await.context("Database initialization failed")?;
    let hash = password::hash(&pass).context("Failed to hash password")?;
    db::seed_root_user(&pool, &username, &hash, "admin").await?;
    ok!("Admin user '{}' created/updated.", username);

    // Fix ownership: DB files were created by root, but the service runs as real_user.
    let owner_arg = format!("{}:{}", real_user, real_user);
    for f in &["yunexal.db", "yunexal.db-shm", "yunexal.db-wal"] {
        let p = dir.join(f);
        if p.exists() {
            let _ = std::process::Command::new("chown")
                .args([&owner_arg, p.to_str().unwrap_or(f)])
                .status();
            info!("chown {} → {}", f, real_user);
        }
    }

    Ok(())
}

async fn step_import_containers(dir: &Path) {
    if !prompt_yn("Import existing Docker containers into the panel?", false).unwrap_or(false) {
        info!("Skipping container import.");
        return;
    }

    // Connect to Docker
    let docker = match yunexal_panel::docker::get_docker_client().await {
        Ok(d) => d,
        Err(e) => { warn!("Cannot connect to Docker daemon: {}", e); return; }
    };

    // List all containers (not just managed ones — for import we want all)
    let containers = match list_all_containers(&docker).await {
        Ok(c) if !c.is_empty() => c,
        Ok(_) => { info!("No Docker containers found."); return; }
        Err(e) => { warn!("Failed to list containers: {}", e); return; }
    };

    println!();
    println!("\x1b[1mDocker containers:\x1b[0m");
    println!("  {:<4} {:<14} {:<28} {:<24} {}", "#", "ID", "Name", "Image", "Status");
    println!("  {}", "─".repeat(78));
    for (i, c) in containers.iter().enumerate() {
        println!("  {:<4} {:<14} {:<28} {:<24} {}", i + 1, &c.0[..12.min(c.0.len())], c.1, &c.2[..24.min(c.2.len())], c.3);
    }
    println!();

    let selection = match prompt("Enter numbers to import (e.g. 1 3 4) or 'all'", None) {
        Ok(s) => s,
        Err(_) => return,
    };

    let panel_pool = match db::init_db().await {
        Ok(p) => p,
        Err(e) => { warn!("DB init failed: {}", e); return; }
    };

    let selected_indices: Vec<usize> = if selection.trim().eq_ignore_ascii_case("all") {
        (0..containers.len()).collect()
    } else {
        selection.split_whitespace()
            .filter_map(|s| s.parse::<usize>().ok())
            .filter(|&n| n >= 1 && n <= containers.len())
            .map(|n| n - 1)
            .collect()
    };

    let db_path = dir.join("yunexal.db");
    if !db_path.exists() {
        warn!("Database not found at {:?} — run setup again after first start, or the DB was just created above.", db_path);
    }

    for idx in selected_indices {
        let (cid, cname, _cimage, _) = &containers[idx];
        match db::register_server(&panel_pool, cid, cname, 0).await {
            Ok(_) => ok!("Imported: {} ({})", cname, &cid[..12.min(cid.len())]),
            Err(e) => warn!("Failed to import {}: {}", cname, e),
        }
    }
}

/// Lists ALL Docker containers (not just yunexal-managed), returns (id, name, image, status).
async fn list_all_containers(docker: &bollard::Docker) -> Result<Vec<(String, String, String, String)>> {
    use bollard::query_parameters::ListContainersOptions;

    let containers = docker
        .list_containers(Some(ListContainersOptions { all: true, ..Default::default() }))
        .await
        .context("Failed to list containers")?;

    let result = containers.into_iter().map(|c| {
        let id = c.id.unwrap_or_default();
        let name = c.names.as_ref()
            .and_then(|n| n.first())
            .map(|n| n.trim_start_matches('/').to_string())
            .unwrap_or_else(|| "unknown".to_string());
        let image = c.image.unwrap_or_default();
        let status = c.status.unwrap_or_default();
        (id, name, image, status)
    }).collect();

    Ok(result)
}

fn step_systemd(dir: &Path, real_user: &str) -> Result<()> {
    let service_path = PathBuf::from("/etc/systemd/system/yunexal-panel.service");

    // Find binary
    let svc_bin = ["yunexal-panel", "target/release/yunexal-panel", "target/debug/yunexal-panel"]
        .iter()
        .map(|p| dir.join(p))
        .find(|p| p.exists())
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| dir.join("target/release/yunexal-panel").to_string_lossy().to_string());

    let service_content = format!(
        "[Unit]\n\
         Description=Yunexal Panel\n\
         Documentation=https://github.com/nestorchurin/yunexal-panel\n\
         After=network.target docker.service\n\
         Wants=docker.service\n\
         \n\
         [Service]\n\
         Type=simple\n\
         User={real_user}\n\
         WorkingDirectory={workdir}\n\
         EnvironmentFile={workdir}/.env\n\
         ExecStart={svc_bin}\n\
         Restart=on-failure\n\
         RestartSec=5\n\
         StandardOutput=journal\n\
         StandardError=journal\n\
         SyslogIdentifier=yunexal-panel\n\
         \n\
         [Install]\n\
         WantedBy=multi-user.target\n",
        real_user = real_user,
        workdir = dir.display(),
        svc_bin = svc_bin,
    );

    std::fs::write(&service_path, service_content)
        .context("Failed to write systemd service file")?;

    let _ = std::process::Command::new("systemctl").arg("daemon-reload").status();
    let _ = std::process::Command::new("systemctl").args(["enable", "yunexal-panel"]).status();
    ok!("Service installed and enabled: {}", service_path.display());

    if prompt_yn("Start yunexal-panel now?", true)? {
        if Path::new(&svc_bin).exists() {
            let _ = std::process::Command::new("systemctl").args(["start", "yunexal-panel"]).status();
            std::thread::sleep(std::time::Duration::from_secs(1));
            let active = std::process::Command::new("systemctl")
                .args(["is-active", "--quiet", "yunexal-panel"])
                .status()
                .map(|s| s.success())
                .unwrap_or(false);
            if active {
                ok!("yunexal-panel is running.");
            } else {
                warn!("Service did not start cleanly — check: journalctl -u yunexal-panel -n 50");
            }
        } else {
            warn!("Binary not found at {} — build the project first.", svc_bin);
        }
    } else {
        info!("Service not started. Run: systemctl start yunexal-panel");
    }

    Ok(())
}

fn read_env_port(dir: &Path) -> Option<String> {
    let content = std::fs::read_to_string(dir.join(".env")).ok()?;
    for line in content.lines() {
        if line.starts_with("PANEL_PORT=") {
            return Some(line["PANEL_PORT=".len()..].to_string());
        }
    }
    None
}
