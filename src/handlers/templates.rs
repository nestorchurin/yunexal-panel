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
    pub users: Vec<UserInfo>,
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
}

#[derive(Template)]
#[template(path = "files.html")]
pub struct FilesTemplate {
    pub id: i64,
    pub container: ContainerInfo,
}

#[derive(Template)]
#[template(path = "edit.html")]
pub struct FileEditTemplate {
    pub id: i64,
    pub container: ContainerInfo,
    pub path: String,
    pub filename: String,
    pub content: String,
    pub ace_mode: String,
}

#[derive(Template)]
#[template(path = "settings.html")]
pub struct SettingsTemplate {

    pub id: i64,
    pub container: ContainerInfo,
    pub is_admin: bool,
}

#[derive(Debug, Clone)]
pub struct PortRow {
    pub host_port: u16,
    pub container_port: u16,
    pub tag: String,
    pub enabled: bool,
}

#[derive(Template)]
#[template(path = "networking.html")]
pub struct NetworkingTemplate {
    pub id: i64,
    pub container: ContainerInfo,
    /// Current bandwidth limit in Mbit/s, or None for unlimited.
    pub bandwidth_mbit: Option<u32>,
    pub is_admin: bool,
    pub ports: Vec<PortRow>,
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
    pub panel_memory_mb: String,
    pub panel_version: String,
    pub users: Vec<UserInfo>,
    pub users_count: usize,
    pub tab: String,
}

#[derive(Template)]
#[template(path = "admin_edit.html")]
pub struct AdminEditTemplate {
    pub id: i64,
    pub container: ContainerInfo,
    pub edit: ContainerEditInfo,
    pub users: Vec<UserInfo>,
    pub error: Option<String>,
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
    pub owner_id: i64,
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
    // ── DNS / SRV auto-record ────────────────────────────────────────────────
    /// "1" to create an SRV record after container is created.
    #[serde(default)]
    pub dns_srv_enabled: String,
    #[serde(default)]
    pub dns_provider_id: String,
    #[serde(default)]
    pub dns_zone_id: String,
    #[serde(default)]
    pub dns_zone_name: String,
    /// Full SRV name, e.g. `_minecraft._tcp`
    #[serde(default)]
    pub dns_srv_name: String,
    #[serde(default)]
    pub dns_srv_port: String,
    /// SRV target hostname (leave empty to use zone name)
    #[serde(default)]
    pub dns_srv_target: String,
    #[serde(default)]
    pub dns_srv_priority: String,
    #[serde(default)]
    pub dns_srv_weight: String,
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
pub struct FileUploadQuery {
    #[serde(default)]
    pub path: String,
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
    pub username: String,
    pub password: String,
    pub role: String,
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
    pub ports: String,
    pub env: String,
}
