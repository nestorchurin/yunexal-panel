use askama::Template;
use axum::response::Html;
use serde::Deserialize;
use crate::docker::ContainerInfo;

/// Render an Askama template into an HTML response.
pub fn render<T: Template>(t: T) -> Html<String> {
    Html(t.render().unwrap_or_else(|e| format!("<p>Template error: {e}</p>")))
}

/// Display-safe user record (no password hash).
#[derive(Debug, Clone)]
pub struct UserInfo {
    pub id: i64,
    pub uid: String,
    pub nickname: String,
    pub username: String,
    pub role: String,
    pub created_at: String,
}

// ── Page templates ────────────────────────────────────────────────────────────

#[derive(Template)]
#[template(path = "index.html")]
pub struct IndexTemplate {
    pub containers: Vec<ContainerInfo>,
    pub is_admin: bool,
    pub auth_username: String,
    pub auth_owner_label: String,
    pub nonce: String,
}

#[derive(Template)]
#[template(path = "server_list.html")]
pub struct ServerListTemplate {
    pub containers: Vec<ContainerInfo>,
    pub is_admin: bool,
}

#[derive(Template)]
#[template(path = "server_card.html")]
pub struct ServerCardTemplate {
    pub container: ContainerInfo,
    pub is_admin: bool,
}

#[derive(Template)]
#[template(path = "new_server.html")]
pub struct NewServerTemplate {
    pub error: Option<String>,
    pub fix_cmd: Option<String>,
    pub users: Vec<UserInfo>,
    pub nonce: String,
    pub default_quota_gb: String,
}

#[derive(Template)]
#[template(path = "login.html")]
pub struct LoginTemplate {
    pub error: Option<String>,
}

#[derive(Template)]
#[template(path = "console.html")]
pub struct ConsoleTemplate {
    pub id: i64,
    pub container: ContainerInfo,
    pub can_power: bool,
    pub can_members: bool,
    pub active_tab: &'static str,
    pub nonce: String,
}

#[derive(Template)]
#[template(path = "server_users.html")]
pub struct ServerUsersTemplate {
    pub id: i64,
    pub container: ContainerInfo,
    pub can_members: bool,
    pub can_members_write: bool,
    pub active_tab: &'static str,
    pub nonce: String,
}

#[derive(Template)]
#[template(path = "files.html")]
pub struct FilesTemplate {
    pub id: i64,
    pub container: ContainerInfo,
    pub can_members: bool,
    pub active_tab: &'static str,
    pub nonce: String,
    pub sftp_enabled: bool,
    pub sftp_port: String,
    pub sftp_username_hint: String,
}

#[derive(Template)]
#[template(path = "edit.html")]
pub struct FileEditTemplate {
    pub id: i64,
    pub container: ContainerInfo,
    pub can_members: bool,
    pub path: String,
    pub filename: String,
    pub content: String,
    pub ace_mode: String,
    pub active_tab: &'static str,
    pub nonce: String,
}

#[derive(Template)]
#[template(path = "settings.html")]
pub struct SettingsTemplate {

    pub id: i64,
    pub container: ContainerInfo,
    pub is_admin: bool,
    pub can_edit_keys: bool,
    pub can_members: bool,
    pub active_tab: &'static str,
    pub nonce: String,
    pub env: String,
    pub env_permissions: String,
    pub sftp_enabled: bool,
    pub sftp_port: String,
    pub sftp_username_hint: String,
}

#[derive(Debug, Clone)]
pub struct PortRow {
    pub host_port: u16,
    pub container_port: u16,
    pub tag: String,
    pub enabled: bool,
    pub ufw_blocked: bool,
}

#[derive(Template)]
#[template(path = "networking.html")]
pub struct NetworkingTemplate {
    pub id: i64,
    pub container: ContainerInfo,
    /// Current bandwidth limit in Mbit/s, or None for unlimited.
    pub bandwidth_mbit: Option<u32>,
    pub is_admin: bool,
    pub can_members: bool,
    pub ports: Vec<PortRow>,
    pub active_tab: &'static str,
    pub nonce: String,
    pub ufw_enabled: bool,
    pub bandwidth_enabled: bool,
}

#[derive(Template)]
#[template(path = "server_audit.html")]
pub struct ServerAuditTemplate {
    pub id: i64,
    pub container: ContainerInfo,
    pub can_members: bool,
    pub active_tab: &'static str,
    pub nonce: String,
}

#[derive(Template)]
#[template(path = "schedules.html")]
pub struct SchedulesTemplate {
    pub id: i64,
    pub container: ContainerInfo,
    pub can_members: bool,
    pub can_write: bool,
    pub active_tab: &'static str,
    pub nonce: String,
}

#[derive(Template)]
#[template(path = "backups.html")]
pub struct BackupsTemplate {
    pub id: i64,
    pub container: ContainerInfo,
    pub can_members: bool,
    pub can_write: bool,
    pub is_admin: bool,
    pub max_backups: i64,
    pub active_tab: &'static str,
    pub nonce: String,
}

#[derive(Template)]
#[template(path = "admin.html")]
pub struct AdminTemplate {
    pub containers: Vec<ContainerInfo>,
    pub total_containers: usize,
    pub running_containers: usize,
    pub stopped_containers: usize,
    pub docker_version: String,
    pub docker_api_version: String,
    pub docker_os: String,
    pub docker_arch: String,
    pub docker_mem_gb: String,
    pub docker_cpus: i64,
    pub docker_storage_driver: String,
    pub listen_addr: String,
    pub auth_username: String,
    pub auth_role: String,
    pub auth_role_color: String,
    pub auth_role_badge_bg: String,
    pub auth_role_badge_border: String,
    pub root_role_color: String,
    pub panel_memory_mb: String,
    pub panel_version: String,
    pub users: Vec<UserInfo>,
    pub users_count: usize,
    pub tab: String,
    // Host system stats
    pub kernel_version: String,
    pub host_uptime: String,
    pub host_load_avg: String,
    pub host_ram_used_gb: String,
    pub host_ram_total_gb: String,
    pub host_swap_used_gb: String,
    pub host_swap_total_gb: String,
    // ZRAM (empty strings = not active)
    pub zram_active: bool,
    pub zram_devices: usize,
    pub zram_disk_mb: String,
    pub zram_orig_mb: String,
    pub zram_compr_mb: String,
    pub zram_ratio: String,
    pub zram_algorithm: String,
    pub nonce: String,
    pub settings_ufw_enabled: bool,
    pub settings_bandwidth_enabled: bool,
    pub docker_default_quota: String,
    pub container_storage_path: String,
    pub settings_storage_unsafe_override: bool,
    pub panel_accent: String,
    pub panel_name: String,
    pub settings_sftp_enabled: bool,
    pub settings_sftp_port: String,
    pub notifications_enabled: bool,
    pub notifications_webhook_url: String,
    pub notifications_email_to: String,
    pub notifications_on_server_down: bool,
    pub notifications_on_disk_high: bool,
    pub notifications_on_user_create: bool,
}

#[derive(Template)]
#[template(path = "admin_edit.html")]
pub struct AdminEditTemplate {
    pub id: i64,
    pub container: ContainerInfo,
    pub edit: ContainerEditInfo,
    pub current_storage_source: String,
    pub current_storage_base: String,
    pub users: Vec<UserInfo>,
    pub error: Option<String>,
    pub nonce: String,
}

/// Container config extracted from Docker inspect for the edit form.
#[derive(Debug, Clone)]
pub struct ContainerEditInfo {
    pub image: String,
    /// Newline-joined "KEY=VALUE" environment variable lines.
    pub env: String,
    /// Newline-joined "host:container/proto" port lines.
    pub ports: String,
    /// CPU limit as string (empty = unlimited).
    pub cpu: String,
    /// Memory limit in MB as string (empty = unlimited).
    pub memory_mb: String,
    /// Disk limit as string (e.g. "15gb", empty = unlimited).
    pub disk_limit: String,
    /// Bandwidth limit in Mbit/s (empty = unlimited).
    pub bandwidth_mbit: String,
    pub owner_id: i64,
    /// Max schedule run history entries kept per server.
    pub max_schedule_runs: String,
    /// Max backups stored per server.
    pub max_backups: String,
}

// ── Form / Query structs ──────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct CreateServerForm {
    #[allow(dead_code)]
    pub name: String,
    pub image: String,
    pub config: String,
    /// Bandwidth limit in Mbit/s set at creation time. Empty string = unlimited.
    #[serde(default)]
    pub bandwidth_mbit: String,
    /// Owner user id selected in the form. 0 = assigned server-side (self).
    #[serde(default)]
    pub owner_id: i64,
    /// Custom storage path for this container's volume. Empty = use panel default.
    #[serde(default)]
    pub container_storage_path: String,
    /// Max schedule run history kept per server.
    #[serde(default)]
    pub max_schedule_runs: i64,
}

#[derive(Deserialize)]
pub struct FileContentQuery {
    pub path: String,
}

#[derive(Deserialize)]
pub struct FileListQuery {
    pub path: Option<String>,
}

#[derive(Deserialize)]
pub struct SaveFileForm {
    pub path: String,
    pub content: String,
}

#[derive(Deserialize)]
pub struct CreateFileForm {
    pub name: String,
    /// Current directory path — set by the file browser JS
    #[serde(default)]
    pub path: String,
    /// "file" (default) or "folder"
    #[serde(default)]
    pub entry_type: String,
}

#[derive(Deserialize)]
pub struct DeleteFileQuery {
    pub path: String,
}

#[derive(Deserialize)]
pub struct RenameFileForm {
    pub path: String,
    pub new_name: String,
}

#[derive(Deserialize)]
pub struct CopyFileForm {
    /// Source absolute-from-volume-root path, e.g. /plugins/foo.jar
    pub src: String,
    /// Destination directory, e.g. /plugins/backup
    pub dst_dir: String,
}

#[derive(Deserialize)]
pub struct BulkPathsForm {
    /// Newline-separated list of volume-relative paths to operate on.
    pub paths: String,
}

#[derive(Deserialize)]
pub struct FileUploadQuery {
    #[serde(default)]
    pub path: String,
}

#[derive(Deserialize)]
pub struct FileChunkUploadQuery {
    #[serde(default)]
    pub path: String,
    pub filename: String,
    pub upload_id: String,
    pub chunk_index: u32,
    pub total_chunks: u32,
}

#[derive(Deserialize)]
pub struct FileChunkCompleteQuery {
    #[serde(default)]
    pub path: String,
    pub filename: String,
    pub upload_id: String,
    pub total_chunks: u32,
}

#[derive(Deserialize)]
pub struct RenameServerForm {
    pub name: String,
}

#[derive(Deserialize)]
pub struct LoginForm {
    pub username: String,
    pub password: String,
}

#[derive(Deserialize)]
pub struct ChangePwForm {
    pub current: String,
    pub new_password: String,
}

#[derive(Deserialize)]
pub struct CreateUserForm {
    pub uid: String,
    pub nickname: String,
    pub username: String,
    pub password: String,
    pub role: String,
}

#[derive(Deserialize)]
pub struct SetUserRoleForm {
    pub role: String,
}

#[derive(Deserialize)]
pub struct CreateRoleForm {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub color: String,
}

#[derive(Deserialize)]
pub struct SetRolePermissionsForm {
    #[serde(default)]
    pub permissions: std::collections::HashMap<String, String>,
    #[serde(default)]
    pub color: String,
}

#[derive(Deserialize)]
pub struct AdminSetPasswordForm {
    pub new_password: String,
}

#[derive(Deserialize)]
pub struct EditContainerForm {
    pub name: String,
    pub image: String,
    pub owner_id: i64,
    pub memory_mb: i64,
    pub cpu: f64,
    #[serde(default)]
    pub disk_limit: String,
    #[serde(default)]
    pub bandwidth_mbit: String,
    pub ports: String,
    pub env: String,
    #[serde(default)]
    pub max_schedule_runs: i64,
    #[serde(default)]
    pub max_backups: i64,
}

#[derive(Deserialize)]
pub struct ExtractForm {
    pub path: String,
    #[serde(default)]
    pub destination: String,
}

#[derive(Deserialize)]
pub struct ArchiveForm {
    pub dir: String,
    pub name: String,
    pub paths: String,
}

#[derive(Deserialize)]
pub struct DownloadBulkForm {
    pub dir: String,
    pub paths: String,
}
