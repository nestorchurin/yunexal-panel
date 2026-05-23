// yunexal-setup — interactive setup wizard (replaces setup.sh)
// Compiled as a separate binary alongside the main yunexal-panel server.

use std::io::{self, BufRead, Write};
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};
use anyhow::{Context, Result};
use sqlx::{Pool, Sqlite};
use yunexal_panel::{crypto, db, host, password};

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

fn root_fs_type() -> Option<String> {
    let mounts = std::fs::read_to_string("/proc/mounts").ok()?;
    for line in mounts.lines() {
        let mut cols = line.split_whitespace();
        let _source = cols.next()?;
        let mount_point = cols.next()?;
        let fs_type = cols.next()?;
        if mount_point == "/" {
            return Some(fs_type.to_string());
        }
    }
    None
}

fn is_alpine_live_installer() -> bool {
    if !Path::new("/etc/alpine-release").exists() {
        return false;
    }

    let cmdline = std::fs::read_to_string("/proc/cmdline").unwrap_or_default();
    let has_live_boot_token = ["apkovl=", "modloop=", "alpine_dev="]
        .iter()
        .any(|token| cmdline.contains(token));

    let root_fs = root_fs_type().unwrap_or_default();
    let ephemeral_root = matches!(
        root_fs.as_str(),
        "tmpfs" | "overlay" | "squashfs" | "ramfs"
    );

    has_live_boot_token || ephemeral_root
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PackageManager {
    Apk,
    Apt,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ServiceManager {
    OpenRc,
    Systemd,
}

#[derive(Debug, Clone, Copy)]
struct SetupPlatform {
    package_manager: PackageManager,
    service_manager: ServiceManager,
}

fn command_exists(cmd: &str) -> bool {
    if cmd.contains('/') {
        return Path::new(cmd).exists();
    }

    std::env::var_os("PATH")
        .map(|paths| std::env::split_paths(&paths).any(|p| p.join(cmd).exists()))
        .unwrap_or(false)
}

fn ensure_apk_packages(packages: &[&str]) -> Result<()> {
    let mut args = vec!["add", "--no-cache"];
    args.extend_from_slice(packages);

    let status = std::process::Command::new("apk")
        .args(args)
        .status()
        .context("Failed to execute apk add")?;

    if !status.success() {
        anyhow::bail!("apk add failed for required packages");
    }

    Ok(())
}

fn ensure_apt_packages(packages: &[&str]) -> Result<()> {
    let update = std::process::Command::new("apt-get")
        .arg("update")
        .status()
        .context("Failed to execute apt-get update")?;

    if !update.success() {
        anyhow::bail!("apt-get update failed");
    }

    let mut cmd = std::process::Command::new("apt-get");
    cmd.env("DEBIAN_FRONTEND", "noninteractive");
    cmd.arg("install").arg("-y");
    for pkg in packages {
        cmd.arg(pkg);
    }

    let status = cmd
        .status()
        .context("Failed to execute apt-get install")?;

    if !status.success() {
        anyhow::bail!(
            "apt-get install failed for required packages: {}",
            packages.join(", ")
        );
    }

    Ok(())
}

fn ensure_packages(pm: PackageManager, packages: &[&str]) -> Result<()> {
    match pm {
        PackageManager::Apk => ensure_apk_packages(packages),
        PackageManager::Apt => ensure_apt_packages(packages),
        PackageManager::Unknown => {
            anyhow::bail!(
                "No supported package manager found (need apk or apt-get) to install: {}",
                packages.join(", ")
            )
        }
    }
}

fn package_manager_label(pm: PackageManager) -> &'static str {
    match pm {
        PackageManager::Apk => "apk",
        PackageManager::Apt => "apt-get",
        PackageManager::Unknown => "unknown",
    }
}

fn service_manager_label(sm: ServiceManager) -> &'static str {
    match sm {
        ServiceManager::OpenRc => "OpenRC",
        ServiceManager::Systemd => "systemd",
    }
}

fn detect_platform() -> Result<SetupPlatform> {
    let service_manager = match host::detect_init_system() {
        host::InitSystem::OpenRc => ServiceManager::OpenRc,
        host::InitSystem::Systemd => ServiceManager::Systemd,
        host::InitSystem::SysV => {
            anyhow::bail!("Unsupported init system: SysV. Supported: OpenRC, systemd")
        }
    };

    let package_manager = if command_exists("apk") {
        PackageManager::Apk
    } else if command_exists("apt-get") {
        PackageManager::Apt
    } else {
        PackageManager::Unknown
    };

    match service_manager {
        ServiceManager::OpenRc => {
            if !command_exists("rc-service") || !command_exists("rc-update") {
                anyhow::bail!("OpenRC detected, but rc-service/rc-update are missing")
            }
        }
        ServiceManager::Systemd => {
            if !command_exists("systemctl") {
                anyhow::bail!("systemd detected, but systemctl is missing")
            }
        }
    }

    Ok(SetupPlatform {
        package_manager,
        service_manager,
    })
}

fn service_status_hint(service_manager: ServiceManager) -> &'static str {
    match service_manager {
        ServiceManager::OpenRc => "rc-service yunexal-panel status",
        ServiceManager::Systemd => "systemctl status yunexal-panel --no-pager",
    }
}

fn service_enable(service_manager: ServiceManager, service: &str) {
    match service_manager {
        ServiceManager::OpenRc => {
            let _ = std::process::Command::new("rc-update")
                .args(["add", service, "default"])
                .status();
        }
        ServiceManager::Systemd => {
            let _ = std::process::Command::new("systemctl")
                .args(["enable", service])
                .status();
        }
    }
}

fn service_start(service_manager: ServiceManager, service: &str) {
    match service_manager {
        ServiceManager::OpenRc => {
            let _ = std::process::Command::new("rc-service")
                .args([service, "start"])
                .status();
        }
        ServiceManager::Systemd => {
            let _ = std::process::Command::new("systemctl")
                .args(["start", service])
                .status();
        }
    }
}

fn service_stop(service_manager: ServiceManager, service: &str) {
    match service_manager {
        ServiceManager::OpenRc => {
            let _ = std::process::Command::new("rc-service")
                .args([service, "stop"])
                .status();
        }
        ServiceManager::Systemd => {
            let _ = std::process::Command::new("systemctl")
                .args(["stop", service])
                .status();
        }
    }
}

fn service_reload(service_manager: ServiceManager, service: &str) {
    match service_manager {
        ServiceManager::OpenRc => {
            let _ = std::process::Command::new("rc-service")
                .args([service, "reload"])
                .status();
        }
        ServiceManager::Systemd => {
            let _ = std::process::Command::new("systemctl")
                .args(["reload", service])
                .status();
        }
    }
}

fn service_is_active(service_manager: ServiceManager, service: &str) -> bool {
    match service_manager {
        ServiceManager::OpenRc => std::process::Command::new("rc-service")
            .args([service, "status"])
            .status()
            .map(|s| s.success())
            .unwrap_or(false),
        ServiceManager::Systemd => std::process::Command::new("systemctl")
            .args(["is-active", "--quiet", service])
            .status()
            .map(|s| s.success())
            .unwrap_or(false),
    }
}

fn add_user_to_docker_group(real_user: &str, service_manager: ServiceManager) -> bool {
    let status = match service_manager {
        ServiceManager::OpenRc => std::process::Command::new("addgroup")
            .args([real_user, "docker"])
            .status(),
        ServiceManager::Systemd => std::process::Command::new("usermod")
            .args(["-aG", "docker", real_user])
            .status()
            .or_else(|_| std::process::Command::new("adduser").args([real_user, "docker"]).status()),
    };

    status.map(|s| s.success()).unwrap_or(false)
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

const USER_UID_MIN_LEN: usize = 9;
const USER_UID_MAX_LEN: usize = 16;
const USER_NICK_MAX_LEN: usize = 24;

fn trim_to_len_chars(input: &str, max_chars: usize) -> String {
    input.trim().chars().take(max_chars).collect()
}

fn validate_user_fields(uid: &str, nickname: &str, username: &str) -> Result<()> {
    let uid_len = uid.chars().count();
    if uid_len < USER_UID_MIN_LEN || uid_len > USER_UID_MAX_LEN {
        anyhow::bail!("UID must be between {} and {} characters", USER_UID_MIN_LEN, USER_UID_MAX_LEN);
    }

    let nick_len = nickname.chars().count();
    if nick_len == 0 || nick_len > USER_NICK_MAX_LEN {
        anyhow::bail!("Nickname must be between 1 and {} characters", USER_NICK_MAX_LEN);
    }

    if username.trim().is_empty() {
        anyhow::bail!("Username cannot be empty");
    }

    Ok(())
}

fn print_users_table(users: &[db::User]) {
    println!();
    println!("\x1b[1mCurrent users:\x1b[0m");
    println!("  {:<4} {:<16} {:<24} {:<18} {:<10} {}", "ID", "UID", "Nickname", "Username", "Role", "Created");
    println!("  {}", "─".repeat(92));
    for u in users {
        println!(
            "  {:<4} {:<16} {:<24} {:<18} {:<10} {}",
            u.id,
            trim_to_len_chars(&u.uid, 16),
            trim_to_len_chars(&u.nickname, 24),
            trim_to_len_chars(&u.username, 18),
            trim_to_len_chars(&u.role, 10),
            u.created_at
        );
    }
    println!();
}

async fn prompt_role_with_default(pool: &Pool<Sqlite>, default_role: &str) -> Result<String> {
    let roles = db::list_roles(pool).await.context("Failed to list roles")?;
    let mut role_names: Vec<String> = roles.into_iter().map(|r| r.name).collect();
    role_names.sort();
    println!("Available roles: {}", role_names.join(", "));

    loop {
        let raw = prompt("Role", Some(default_role))?;
        let role = raw.trim().to_lowercase();
        if role.is_empty() {
            return Ok(default_role.to_string());
        }
        if db::role_exists(pool, &role).await? {
            return Ok(role);
        }
        eprintln!("\x1b[31m[ERROR]\x1b[0m Role '{}' does not exist.", role);
    }
}

fn map_user_db_error(e: anyhow::Error, action: &str) -> anyhow::Error {
    let msg = e.to_string().to_lowercase();
    if msg.contains("users.username") || msg.contains("idx_users_username") {
        anyhow::anyhow!("{} failed: username already exists", action)
    } else if msg.contains("users.uid") || msg.contains("idx_users_uid") {
        anyhow::anyhow!("{} failed: uid already exists", action)
    } else if msg.contains("uid must be 9-16") {
        anyhow::anyhow!("{} failed: uid must be between 9 and 16 characters", action)
    } else if msg.contains("nickname") {
        anyhow::anyhow!("{} failed: invalid nickname value", action)
    } else {
        e
    }
}

async fn setup_add_user(pool: &Pool<Sqlite>) -> Result<()> {
    let uid = prompt("UID (9-16 chars)", None)?;
    let nickname = prompt("Nickname", None)?;
    let username = prompt("Username", None)?;
    let role = prompt_role_with_default(pool, "user").await?;

    let pass = loop {
        let p = prompt_password("Password (min 8 chars)")?;
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

    let uid = uid.trim().to_string();
    let nickname = trim_to_len_chars(&nickname, USER_NICK_MAX_LEN);
    let username = username.trim().to_string();
    validate_user_fields(&uid, &nickname, &username)?;

    let hash = password::hash(&pass).context("Failed to hash password")?;
    db::create_user(pool, &uid, &nickname, &username, &hash, &role)
        .await
        .map_err(|e| map_user_db_error(e, "Create user"))?;

    ok!("User '{}' created.", username);
    Ok(())
}

async fn setup_edit_user(pool: &Pool<Sqlite>) -> Result<()> {
    let id_raw = prompt("User ID to edit", None)?;
    let id: i64 = id_raw.trim().parse().context("Invalid user ID")?;

    let user = db::find_user_by_id(pool, id)
        .await
        .context("Failed to find user")?
        .ok_or_else(|| anyhow::anyhow!("User not found"))?;

    let next_uid = {
        let raw = prompt("UID", Some(&user.uid))?;
        if raw.trim().is_empty() { user.uid.clone() } else { raw.trim().to_string() }
    };
    let next_nickname = {
        let raw = prompt("Nickname", Some(&user.nickname))?;
        if raw.trim().is_empty() { user.nickname.clone() } else { trim_to_len_chars(&raw, USER_NICK_MAX_LEN) }
    };
    let next_username = {
        let raw = prompt("Username", Some(&user.username))?;
        if raw.trim().is_empty() { user.username.clone() } else { raw.trim().to_string() }
    };

    validate_user_fields(&next_uid, &next_nickname, &next_username)?;

    db::update_user_profile(pool, id, &next_uid, &next_nickname, &next_username)
        .await
        .map_err(|e| map_user_db_error(e, "Update user"))?;

    if user.role != "root" {
        let next_role = prompt_role_with_default(pool, &user.role).await?;
        if next_role != user.role {
            db::update_user_role(pool, id, &next_role)
                .await
                .context("Failed to update user role")?;
        }
    } else {
        info!("Root role cannot be changed; keeping role='root'.");
    }

    if prompt_yn("Change password?", false)? {
        let new_password = loop {
            let p = prompt_password("New password (min 8 chars)")?;
            if p.len() < 8 {
                eprintln!("\x1b[31m[ERROR]\x1b[0m Password too short (minimum 8 characters).");
                continue;
            }
            let p2 = prompt_password("Confirm new password")?;
            if p != p2 {
                eprintln!("\x1b[31m[ERROR]\x1b[0m Passwords do not match.");
                continue;
            }
            break p;
        };
        let hash = password::hash(&new_password).context("Failed to hash password")?;
        db::update_user_password(pool, id, &hash)
            .await
            .context("Failed to update password")?;
        let _ = db::revoke_all_user_sessions(pool, id).await;
    }

    ok!("User '{}' updated.", next_username);
    Ok(())
}

async fn setup_delete_user(pool: &Pool<Sqlite>) -> Result<()> {
    let id_raw = prompt("User ID to delete", None)?;
    let id: i64 = id_raw.trim().parse().context("Invalid user ID")?;

    let user = db::find_user_by_id(pool, id)
        .await
        .context("Failed to find user")?
        .ok_or_else(|| anyhow::anyhow!("User not found"))?;

    if user.role == "root" {
        anyhow::bail!("Cannot delete the root account");
    }

    if !prompt_yn(&format!("Delete user '{}' (id={})?", user.username, user.id), false)? {
        info!("Delete cancelled.");
        return Ok(());
    }

    db::delete_user(pool, id)
        .await
        .context("Failed to delete user")?;
    ok!("User '{}' deleted.", user.username);
    Ok(())
}

async fn step_manage_users(confirm_first: bool) -> Result<()> {
    if confirm_first && !prompt_yn("Open user management now (add/edit/delete users)?", true)? {
        info!("Skipping user management.");
        return Ok(());
    }

    let pool = db::init_db().await.context("Database initialization failed")?;

    loop {
        let users = db::list_users(&pool)
            .await
            .context("Failed to load users")?;
        print_users_table(&users);
        println!("Actions: [a]dd  [e]dit  [d]elete  [q]uit");

        let action = prompt("Select action", Some("q"))?.to_lowercase();
        match action.trim() {
            "a" | "add" => {
                if let Err(e) = setup_add_user(&pool).await {
                    eprintln!("\x1b[31m[ERROR]\x1b[0m {}", e);
                }
            }
            "e" | "edit" => {
                if let Err(e) = setup_edit_user(&pool).await {
                    eprintln!("\x1b[31m[ERROR]\x1b[0m {}", e);
                }
            }
            "d" | "delete" => {
                if let Err(e) = setup_delete_user(&pool).await {
                    eprintln!("\x1b[31m[ERROR]\x1b[0m {}", e);
                }
            }
            "q" | "quit" | "" => {
                info!("Finished user management.");
                break;
            }
            _ => warn!("Unknown action. Use a/e/d/q."),
        }
    }

    Ok(())
}

fn print_main_menu() {
    println!();
    println!("\x1b[1mMain menu\x1b[0m");
    println!("  1 - install");
    println!("  2 - User Management");
    println!("  3 - uninstall panel");
    println!("  0 - quit");
}

async fn run_install_flow(
    script_dir: &Path,
    real_user: &str,
    platform: &SetupPlatform,
    opt_reset: bool,
    opt_non_interactive: bool,
) -> Result<()> {
    // ── Step 1: Reset ───────────────────────────────────────────────────────
    header!("Step 1: Reset");

    let do_reset = if opt_reset {
        true
    } else {
        prompt_yn("Wipe existing database and .env?", false)?
    };

    if do_reset {
        step_reset(script_dir, platform.service_manager).await;
    } else {
        info!("Skipping reset.");
    }

    // ── Step 2: Docker ──────────────────────────────────────────────────────
    header!("Step 2: Docker");
    step_docker(real_user, platform).await?;

    // ── Step 3: .env ────────────────────────────────────────────────────────
    header!("Step 3: Environment (.env)");
    step_env(script_dir, real_user)?;

    // ── Step 4: Admin user ──────────────────────────────────────────────────
    header!("Step 4: Admin user");
    step_admin_user(opt_non_interactive, script_dir, real_user).await?;

    // ── Step 5: Import containers ───────────────────────────────────────────
    header!("Step 5: Import Docker containers");
    step_import_containers(script_dir).await;

    // ── Step 6: Service setup ───────────────────────────────────────────────
    header!("Step 6: Service setup ({})", service_manager_label(platform.service_manager));
    step_panel_service(script_dir, real_user, platform)?;

    // ── Step 7: nginx reverse proxy ─────────────────────────────────────────
    header!("Step 7: nginx reverse proxy");
    step_nginx(script_dir, platform)?;

    // ── Summary ──────────────────────────────────────────────────────────────
    let panel_port = read_env_port(script_dir).unwrap_or_else(|| "3000".to_string());
    println!();
    println!("\x1b[1m\x1b[32m╔══════════════════════════════════════════╗\x1b[0m");
    println!("\x1b[1m\x1b[32m║            Setup complete!               ║\x1b[0m");
    println!("\x1b[1m\x1b[32m╚══════════════════════════════════════════╝\x1b[0m");
    println!();
    println!("  Panel URL  : \x1b[1mhttp://localhost:{}\x1b[0m", panel_port);
    println!("  Service    : \x1b[1m{}\x1b[0m", service_status_hint(platform.service_manager));
    println!("  Logs       : \x1b[1mtail -f /var/log/yunexal-panel.log\x1b[0m");
    println!();

    Ok(())
}

fn step_uninstall_panel(script_dir: &Path, platform: &SetupPlatform) -> Result<()> {
    warn!("This will remove panel service integration and nginx panel config.");
    if !prompt_yn("Continue uninstall?", false)? {
        info!("Uninstall cancelled.");
        return Ok(());
    }

    info!("Stopping yunexal-panel service...");
    service_stop(platform.service_manager, "yunexal-panel");

    match platform.service_manager {
        ServiceManager::OpenRc => {
            let _ = std::process::Command::new("rc-update")
                .args(["del", "yunexal-panel", "default"])
                .status();
            let init_path = PathBuf::from("/etc/init.d/yunexal-panel");
            if init_path.exists() {
                let _ = std::fs::remove_file(&init_path);
                info!("Removed {}", init_path.display());
            }
        }
        ServiceManager::Systemd => {
            let _ = std::process::Command::new("systemctl")
                .args(["disable", "--now", "yunexal-panel"])
                .status();
            let unit_path = PathBuf::from("/etc/systemd/system/yunexal-panel.service");
            if unit_path.exists() {
                let _ = std::fs::remove_file(&unit_path);
                info!("Removed {}", unit_path.display());
            }
            let _ = std::process::Command::new("systemctl")
                .arg("daemon-reload")
                .status();
        }
    }

    let launcher = script_dir.join("yunexal-panel-launcher.sh");
    if launcher.exists() {
        let _ = std::fs::remove_file(&launcher);
        info!("Removed {}", launcher.display());
    }

    match platform.service_manager {
        ServiceManager::OpenRc => {
            let conf = PathBuf::from("/etc/nginx/http.d/yunexal-panel.conf");
            if conf.exists() {
                let _ = std::fs::remove_file(&conf);
                info!("Removed {}", conf.display());
            }
        }
        ServiceManager::Systemd => {
            let conf = PathBuf::from("/etc/nginx/sites-available/yunexal-panel.conf");
            let enabled = PathBuf::from("/etc/nginx/sites-enabled/yunexal-panel.conf");
            if enabled.exists() {
                let _ = std::fs::remove_file(&enabled);
                info!("Removed {}", enabled.display());
            }
            if conf.exists() {
                let _ = std::fs::remove_file(&conf);
                info!("Removed {}", conf.display());
            }
        }
    }

    if command_exists("nginx") {
        let test = std::process::Command::new("nginx").arg("-t").output();
        if let Ok(o) = test {
            if o.status.success() {
                if service_is_active(platform.service_manager, "nginx") {
                    service_reload(platform.service_manager, "nginx");
                }
            }
        }
    }

    if prompt_yn("Remove panel data files (.env and yunexal.db*)?", false)? {
        for f in &["yunexal.db", "yunexal.db-shm", "yunexal.db-wal", ".env"] {
            let p = script_dir.join(f);
            if p.exists() {
                let _ = std::fs::remove_file(&p);
                info!("Removed {}", f);
            }
        }
    }

    ok!("Panel uninstall completed.");
    Ok(())
}

// ── Main ──────────────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let opt_reset          = args.iter().any(|a| a == "--reset");
    let opt_non_interactive = args.iter().any(|a| a == "--non-interactive");

    // --rekey --old-secret <secret>
    if let Some(rekey_idx) = args.iter().position(|a| a == "--rekey") {
        let old_secret_idx = args.iter().position(|a| a == "--old-secret")
            .and_then(|i| args.get(i + 1));
        let old_secret = match old_secret_idx {
            Some(s) => s.clone(),
            None => {
                eprintln!("\x1b[31m[ERROR]\x1b[0m --rekey requires --old-secret <previous_COOKIE_SECRET>");
                std::process::exit(1);
            }
        };
        let _ = rekey_idx;
        let script_dir = std::env::current_dir()
            .context("Failed to determine working directory")?;
        return do_rekey(&old_secret, &script_dir).await;
    }

    if args.iter().any(|a| a == "--help" || a == "-h") {
        println!("Usage: yunexal-setup [--reset] [--non-interactive] [--rekey --old-secret <key>]");
        println!();
        println!("  --reset                  Wipe DB and .env without prompting");
        println!("  --non-interactive        Read credentials from PANEL_USERNAME / PANEL_PASSWORD env vars");
        println!("  --rekey --old-secret X   Re-encrypt the database after rotating COOKIE_SECRET");
        println!();
        println!("Interactive menu:");
        println!("  1 - install");
        println!("  2 - User Management");
        println!("  3 - uninstall panel");
        println!("  0 - quit");
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

    if is_alpine_live_installer() {
        eprintln!("\x1b[31m[ERROR]\x1b[0m Running yunexal-setup from Alpine live USB is blocked.");
        eprintln!("\x1b[33m[INFO]\x1b[0m  Use installer flow from live session:");
        eprintln!("\x1b[33m[INFO]\x1b[0m    1) yunexal-install prepare --disk /dev/sdX --mode safe --root-size-gib 40");
        eprintln!("\x1b[33m[INFO]\x1b[0m    2) yunexal-setup (run installer to YUNEXAL_SYS)");
        eprintln!("\x1b[33m[INFO]\x1b[0m    3) yunexal-install finalize --disk /dev/sdX --target-root /mnt");
        eprintln!("\x1b[33m[INFO]\x1b[0m    4) reboot into installed system and run yunexal-setup there");
        std::process::exit(1);
    }

    let platform = detect_platform()
        .context("Failed to detect supported host platform (need OpenRC or systemd)")?;
    info!(
        "Detected platform: service manager={}, package manager={}",
        service_manager_label(platform.service_manager),
        package_manager_label(platform.package_manager)
    );

    let real_user = real_user();
    let script_dir = std::env::current_dir()
        .context("Failed to determine working directory")?;

    // Preserve previous CLI behavior for automation flags.
    if opt_reset || opt_non_interactive {
        run_install_flow(&script_dir, &real_user, &platform, opt_reset, opt_non_interactive).await?;
        return Ok(());
    }

    loop {
        print_main_menu();
        let choice = prompt("Choose action", Some("1"))?;
        match choice.trim() {
            "1" => run_install_flow(&script_dir, &real_user, &platform, false, false).await?,
            "2" => {
                header!("User management");
                step_manage_users(false).await?;
            }
            "3" => {
                header!("Uninstall panel");
                step_uninstall_panel(&script_dir, &platform)?;
            }
            "0" => {
                info!("Bye.");
                break;
            }
            _ => warn!("Unknown option. Use 1/2/3/0."),
        }
    }

    Ok(())
}

// ── Step implementations ──────────────────────────────────────────────────────

async fn step_reset(dir: &Path, service_manager: ServiceManager) {
    info!("Stopping yunexal-panel service (if running)…");
    service_stop(service_manager, "yunexal-panel");

    for f in &["yunexal.db", "yunexal.db-shm", "yunexal.db-wal", ".env"] {
        let p = dir.join(f);
        if p.exists() {
            let _ = std::fs::remove_file(&p);
            info!("Removed {}", f);
        }
    }
    ok!("Reset complete.");
}

async fn step_docker(real_user: &str, platform: &SetupPlatform) -> Result<()> {
    if !command_exists("docker") {
        info!(
            "Docker not found. Installing Docker packages with {}…",
            package_manager_label(platform.package_manager)
        );
        match platform.package_manager {
            PackageManager::Apk => {
                // docker-cli-compose may be unavailable in some mirrors, fallback to core package set.
                if ensure_packages(platform.package_manager, &["docker", "docker-cli-compose"]).is_err() {
                    warn!("Failed to install docker-cli-compose; retrying with core Docker package only.");
                    ensure_packages(platform.package_manager, &["docker"])?;
                }
            }
            PackageManager::Apt => {
                if ensure_packages(platform.package_manager, &["docker.io", "docker-compose-plugin"]).is_err() {
                    warn!("Failed to install docker-compose-plugin; retrying with docker.io only.");
                    ensure_packages(platform.package_manager, &["docker.io"])?;
                }
            }
            PackageManager::Unknown => {
                anyhow::bail!("Docker is missing and no supported package manager is available to install it")
            }
        }
        ok!("Docker packages installed.");
    } else {
        info!("Docker CLI detected.");
    }

    if real_user != "root" {
        if !add_user_to_docker_group(real_user, platform.service_manager) {
            warn!("Could not add '{}' to docker group automatically (continue manually if needed).", real_user);
        } else {
            ok!("User '{}' is in docker group.", real_user);
        }
    }

    service_enable(platform.service_manager, "docker");
    let running = service_is_active(platform.service_manager, "docker");

    if !running {
        info!(
            "Starting Docker daemon with {}…",
            service_manager_label(platform.service_manager)
        );
        service_start(platform.service_manager, "docker");
    }

    let docker_version = std::process::Command::new("docker")
        .args(["version", "--format", "{{.Server.Version}}"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string());

    if let Some(ref ver) = docker_version {
        ok!("Docker detected: v{}", ver);
    } else {
        warn!("Docker server version could not be detected yet.");
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
    db::seed_root_user(&pool, &username, &hash, "root").await?;
    ok!("Root user '{}' created/updated.", username);

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
            Ok(_) => {
                match yunexal_panel::docker::ensure_management_labels(&docker, cid).await {
                    Ok(true) => ok!("Imported: {} ({}) [labels normalized]", cname, &cid[..12.min(cid.len())]),
                    Ok(false) => ok!("Imported: {} ({})", cname, &cid[..12.min(cid.len())]),
                    Err(e) => {
                        warn!("Imported: {} ({}), but label migration failed: {}", cname, &cid[..12.min(cid.len())], e);
                    }
                }
            }
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

fn sh_esc_single(s: &str) -> String {
    s.replace('\\', "\\\\").replace('\'', "'\\''")
}

fn resolve_panel_binary(dir: &Path) -> String {
    ["yunexal-panel", "target/release/yunexal-panel", "target/debug/yunexal-panel"]
        .iter()
        .map(|p| dir.join(p))
        .find(|p| p.exists())
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| dir.join("target/release/yunexal-panel").to_string_lossy().to_string())
}

fn step_panel_service(dir: &Path, real_user: &str, platform: &SetupPlatform) -> Result<()> {
    match platform.service_manager {
        ServiceManager::OpenRc => step_openrc_service(dir, real_user),
        ServiceManager::Systemd => step_systemd_service(dir, real_user),
    }
}

fn step_openrc_service(dir: &Path, real_user: &str) -> Result<()> {
    let service_path = PathBuf::from("/etc/init.d/yunexal-panel");

    // Find binary
    let svc_bin = resolve_panel_binary(dir);

    let launcher_path = dir.join("yunexal-panel-launcher.sh");
    let launcher_content = format!(
        "#!/bin/sh\n\
         set -a\n\
         [ -f '{workdir}/.env' ] && . '{workdir}/.env'\n\
         set +a\n\
         exec '{bin}' \"$@\"\n",
        workdir = sh_esc_single(&dir.to_string_lossy()),
        bin = sh_esc_single(&svc_bin),
    );
    std::fs::write(&launcher_path, launcher_content)
        .context("Failed to write launcher script")?;
    let _ = std::process::Command::new("chmod")
        .args(["755", launcher_path.to_str().unwrap_or_default()])
        .status();

    let service_content = format!(
        "#!/sbin/openrc-run\n\
         name=\"yunexal-panel\"\n\
         description=\"Yunexal Panel service\"\n\
         command=\"{launcher}\"\n\
         command_user=\"{real_user}:{real_user}\"\n\
         directory=\"{workdir}\"\n\
         pidfile=\"/run/yunexal-panel.pid\"\n\
         command_background=\"yes\"\n\
         start_stop_daemon_args=\"--make-pidfile --pidfile ${{pidfile}} --stdout /var/log/yunexal-panel.log --stderr /var/log/yunexal-panel.log\"\n\
         \n\
         depend() {{\n\
             need net docker\n\
             after firewall\n\
         }}\n\
         \n\
         start_pre() {{\n\
             checkpath --file --mode 0644 --owner {real_user}:{real_user} /var/log/yunexal-panel.log\n\
             checkpath --directory --mode 0755 /run\n\
         }}\n",
        real_user = real_user,
        workdir = dir.display(),
        launcher = launcher_path.display(),
    );

    std::fs::write(&service_path, service_content)
        .context("Failed to write OpenRC service file")?;
    let _ = std::process::Command::new("chmod")
        .args(["755", service_path.to_str().unwrap_or_default()])
        .status();

    service_enable(ServiceManager::OpenRc, "yunexal-panel");
    ok!("OpenRC service installed and enabled: {}", service_path.display());

    if prompt_yn("Start yunexal-panel now?", true)? {
        if Path::new(&svc_bin).exists() {
            service_start(ServiceManager::OpenRc, "yunexal-panel");
            std::thread::sleep(std::time::Duration::from_secs(1));
            let active = service_is_active(ServiceManager::OpenRc, "yunexal-panel");
            if active {
                ok!("yunexal-panel is running.");
            } else {
                warn!("Service did not start cleanly — check: rc-service yunexal-panel status");
            }
        } else {
            warn!("Binary not found at {} — build the project first.", svc_bin);
        }
    } else {
        info!("Service not started. Run: rc-service yunexal-panel start");
    }

    Ok(())
}

fn step_systemd_service(dir: &Path, real_user: &str) -> Result<()> {
    let service_path = PathBuf::from("/etc/systemd/system/yunexal-panel.service");
    let svc_bin = resolve_panel_binary(dir);

    let service_content = format!(
        "[Unit]\n\
         Description=Yunexal Panel service\n\
         After=network-online.target docker.service\n\
         Wants=network-online.target\n\
         Requires=docker.service\n\
         \n\
         [Service]\n\
         Type=simple\n\
         User={real_user}\n\
         Group={real_user}\n\
         WorkingDirectory={workdir}\n\
         EnvironmentFile={workdir}/.env\n\
         ExecStart={bin}\n\
         Restart=on-failure\n\
         RestartSec=2\n\
         StandardOutput=append:/var/log/yunexal-panel.log\n\
         StandardError=append:/var/log/yunexal-panel.log\n\
         \n\
         [Install]\n\
         WantedBy=multi-user.target\n",
        real_user = real_user,
        workdir = dir.display(),
        bin = svc_bin,
    );

    std::fs::write(&service_path, service_content)
        .context("Failed to write systemd service file")?;

    let _ = std::process::Command::new("touch")
        .arg("/var/log/yunexal-panel.log")
        .status();
    let _ = std::process::Command::new("chown")
        .args([&format!("{}:{}", real_user, real_user), "/var/log/yunexal-panel.log"])
        .status();
    let _ = std::process::Command::new("chmod")
        .args(["644", "/var/log/yunexal-panel.log"])
        .status();

    let _ = std::process::Command::new("systemctl")
        .arg("daemon-reload")
        .status();
    service_enable(ServiceManager::Systemd, "yunexal-panel");
    ok!("systemd service installed and enabled: {}", service_path.display());

    if prompt_yn("Start yunexal-panel now?", true)? {
        if Path::new(&svc_bin).exists() {
            service_start(ServiceManager::Systemd, "yunexal-panel");
            std::thread::sleep(std::time::Duration::from_secs(1));
            let active = service_is_active(ServiceManager::Systemd, "yunexal-panel");
            if active {
                ok!("yunexal-panel is running.");
            } else {
                warn!("Service did not start cleanly — check: systemctl status yunexal-panel --no-pager");
            }
        } else {
            warn!("Binary not found at {} — build the project first.", svc_bin);
        }
    } else {
        info!("Service not started. Run: systemctl start yunexal-panel");
    }

    Ok(())
}

// ── nginx WebSocket proxy ─────────────────────────────────────────────────────

/// Builds an nginx virtual-host config with WebSocket proxy headers.
fn build_nginx_config(domain: &str, port: &str, ssl: Option<(&str, &str)>) -> String {
    // The location block — identical for HTTP and HTTPS servers.
    // proxy_http_version 1.1 + Upgrade/Connection headers are required for
    // wss:// (WebSocket over TLS) to work through nginx.
    let location = format!(
        "    server_tokens off;\n    proxy_hide_header X-Powered-By;\n\n    # WebSocket + HTTP reverse proxy (required for console)\n    location / {{\n        proxy_pass http://127.0.0.1:{port};\n        proxy_http_version 1.1;\n        proxy_set_header Upgrade $http_upgrade;\n        proxy_set_header Connection \"upgrade\";\n        proxy_set_header Host $host;\n        proxy_set_header X-Real-IP $remote_addr;\n        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;\n        proxy_set_header X-Forwarded-Proto $scheme;\n        proxy_read_timeout 3600s;\n        proxy_send_timeout 3600s;\n    }}\n",
        port = port,
    );

    if let Some((cert, key)) = ssl {
        format!(
            "server {{\n    listen 80;\n    server_name {d};\n    return 301 https://$host$request_uri;\n}}\n\nserver {{\n    listen 443 ssl;\n    server_name {d};\n\n    ssl_certificate {cert};\n    ssl_certificate_key {key};\n\n{location}}}\n",
            d = domain, cert = cert, key = key, location = location,
        )
    } else {
        format!(
            "server {{\n    listen 80;\n    server_name {d};\n\n{location}}}\n",
            d = domain, location = location,
        )
    }
}

fn step_nginx(dir: &Path, platform: &SetupPlatform) -> Result<()> {
    let mut nginx_installed = command_exists("nginx");

    if !nginx_installed {
        warn!("nginx not found.");
        if prompt_yn(
            &format!(
                "Install nginx now via {}?",
                package_manager_label(platform.package_manager)
            ),
            true,
        )? {
            ensure_packages(platform.package_manager, &["nginx"])?;
            nginx_installed = true;
        }
    }

    if !nginx_installed {
        info!("Skipping nginx configuration.");
        return Ok(());
    }

    if !prompt_yn("Configure nginx reverse proxy? (required for wss:// console WebSocket)", true)? {
        info!("Skipping nginx configuration.");
        warn!("WebSocket consoles will NOT work if nginx is not configured with WebSocket headers.");
        return Ok(());
    }

    let panel_port = read_env_port(dir).unwrap_or_else(|| "3000".to_string());

    let domain = prompt("Domain name (e.g. panel.example.com)", None)?;
    if domain.is_empty() {
        warn!("No domain entered — skipping nginx configuration.");
        return Ok(());
    }

    let cert_path = format!("/etc/letsencrypt/live/{}/fullchain.pem", domain);
    let key_path  = format!("/etc/letsencrypt/live/{}/privkey.pem",   domain);
    let has_ssl   = Path::new(&cert_path).exists();

    let config = build_nginx_config(
        &domain,
        &panel_port,
        if has_ssl { Some((cert_path.as_str(), key_path.as_str())) } else { None },
    );

    let config_path = match platform.service_manager {
        ServiceManager::OpenRc => PathBuf::from("/etc/nginx/http.d/yunexal-panel.conf"),
        ServiceManager::Systemd => PathBuf::from("/etc/nginx/sites-available/yunexal-panel.conf"),
    };

    std::fs::write(&config_path, &config).context("Failed to write nginx configuration")?;

    if platform.service_manager == ServiceManager::Systemd {
        let enabled_dir = PathBuf::from("/etc/nginx/sites-enabled");
        std::fs::create_dir_all(&enabled_dir)
            .context("Failed to create /etc/nginx/sites-enabled")?;

        let enabled_path = enabled_dir.join("yunexal-panel.conf");
        if enabled_path.exists() {
            let _ = std::fs::remove_file(&enabled_path);
        }
        symlink(&config_path, &enabled_path)
            .context("Failed to enable nginx site symlink")?;
    }

    service_enable(platform.service_manager, "nginx");

    // Test and reload
    let test = std::process::Command::new("nginx").arg("-t").output();
    match test {
        Ok(o) if o.status.success() => {
            let is_running = service_is_active(platform.service_manager, "nginx");

            if is_running {
                service_reload(platform.service_manager, "nginx");
            } else {
                service_start(platform.service_manager, "nginx");
            }

            ok!("nginx configured and reloaded: {}", config_path.display());
            if !has_ssl {
                info!("To enable HTTPS with Let's Encrypt:");
                match platform.package_manager {
                    PackageManager::Apk => {
                        info!("  apk add --no-cache certbot certbot-nginx");
                    }
                    PackageManager::Apt => {
                        info!("  apt-get install -y certbot python3-certbot-nginx");
                    }
                    PackageManager::Unknown => {
                        info!("  install certbot + nginx plugin via your distro package manager");
                    }
                }
                info!("  sudo certbot --nginx -d {}", domain);
            }
        }
        Ok(o) => {
            warn!("nginx config test failed — please fix {}:", config_path.display());
            warn!("{}", String::from_utf8_lossy(&o.stderr).trim());
        }
        Err(e) => {
            warn!("Could not verify nginx config: {}", e);
            ok!(
                "Config written to {} — reload nginx manually with {}.",
                config_path.display(),
                service_manager_label(platform.service_manager)
            );
        }
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

// ── Re-encryption ─────────────────────────────────────────────────────────────

async fn do_rekey(old_secret: &str, dir: &Path) -> Result<()> {
    dotenvy::from_path(dir.join(".env")).ok();
    let new_secret = std::env::var("COOKIE_SECRET")
        .context("COOKIE_SECRET not found in .env — run yunexal-setup to configure it first")?;

    if old_secret == new_secret {
        anyhow::bail!("Old and new secrets are identical — nothing to do.");
    }

    let old_key = crypto::derive_db_key(old_secret);
    let new_key = crypto::derive_db_key(&new_secret);

    let pool = db::init_db().await.context("Database init failed")?;

    // Verify old key is correct before touching anything.
    db::verify_or_init_canary(&pool, &old_key)
        .await
        .context("Old secret does not match the stored canary — check --old-secret value")?;

    info!("Old secret verified. Re-encrypting columns...");

    // Re-encrypt image_env_overrides.env
    let img_rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT image_id, env FROM image_env_overrides WHERE env != '' AND env != 'enc:v1:'"
    )
    .fetch_all(&pool)
    .await
    .context("Failed to read image_env_overrides")?;
    let img_count = img_rows.len();
    for (image_id, raw) in img_rows {
        let plain = crypto::decrypt_if_encrypted(&raw, &old_key)?;
        let enc = crypto::encrypt(&plain, &new_key);
        sqlx::query("UPDATE image_env_overrides SET env = ? WHERE image_id = ?")
            .bind(&enc).bind(&image_id)
            .execute(&pool).await?;
    }
    info!("Re-encrypted {} image env rows.", img_count);

    // Re-encrypt service_api_key
    let api_raw: Option<String> = sqlx::query_scalar(
        "SELECT value FROM panel_settings WHERE key = 'service_api_key'"
    )
    .fetch_optional(&pool)
    .await
    .context("Failed to read service_api_key")?;
    if let Some(raw) = api_raw {
        if !raw.trim().is_empty() {
            let plain = crypto::decrypt_if_encrypted(&raw, &old_key)?;
            let enc = crypto::encrypt(&plain, &new_key);
            sqlx::query("UPDATE panel_settings SET value = ? WHERE key = 'service_api_key'")
                .bind(&enc)
                .execute(&pool).await?;
            info!("Re-encrypted service_api_key.");
        }
    }

    // Re-encrypt user_sessions.ip
    let sess_rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT session_id, ip FROM user_sessions WHERE ip != ''"
    )
    .fetch_all(&pool)
    .await
    .context("Failed to read user_sessions")?;
    let sess_count = sess_rows.len();
    for (session_id, raw) in sess_rows {
        let plain = crypto::decrypt_if_encrypted(&raw, &old_key)?;
        let enc = crypto::encrypt(&plain, &new_key);
        sqlx::query("UPDATE user_sessions SET ip = ? WHERE session_id = ?")
            .bind(&enc).bind(&session_id)
            .execute(&pool).await?;
    }
    info!("Re-encrypted {} session IP rows.", sess_count);

    // Update canary to new key.
    let new_canary = crypto::encrypt("yunexal-canary-v1", &new_key);
    sqlx::query("INSERT OR REPLACE INTO key_meta (k, v) VALUES ('canary', ?)")
        .bind(&new_canary)
        .execute(&pool).await
        .context("Failed to update canary")?;

    println!();
    println!("\x1b[32m[OK]\x1b[0m    Re-encryption complete.");
    println!("\x1b[32m[OK]\x1b[0m    {img_count} image env rows, {sess_count} session IP rows, service_api_key — all re-encrypted with new key.");
    Ok(())
}
