use axum::{
    extract::{Form, Multipart, Path, Query, State},
    http::StatusCode,
    response::{Html, IntoResponse, Redirect},
    Json,
};
use axum_extra::extract::cookie::PrivateCookieJar;
use crate::{auth, db, docker};
use crate::state::AppState;
use super::templates::{ArchiveForm, CopyFileForm, CreateFileForm, DeleteFileQuery, ExtractForm, FileContentQuery, FileEditTemplate, FileListQuery, FileUploadQuery, RenameFileForm, SaveFileForm};

fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;")
     .replace('<', "&lt;")
     .replace('>', "&gt;")
     .replace('"', "&quot;")
     .replace('\'', "&#x27;")
}

fn format_size(bytes: u64) -> String {
    if bytes >= 1_073_741_824 {
        format!("{:.1} GB", bytes as f64 / 1_073_741_824.0)
    } else if bytes >= 1_048_576 {
        format!("{:.1} MB", bytes as f64 / 1_048_576.0)
    } else if bytes >= 1_024 {
        format!("{:.1} KB", bytes as f64 / 1_024.0)
    } else {
        format!("{} B", bytes)
    }
}

/// Returns (bootstrap-icon-class, color-class) for a filename based on its extension.
fn file_icon(name: &str) -> (&'static str, &'static str) {
    let ext = name.rsplit('.').next().unwrap_or("").to_lowercase();
    match ext.as_str() {
        "json"|"js"|"ts"|"java"|"py"|"rs"|"go"|"c"|"cpp"|"h"|"lua"|"rb"|"php"|"sql"
            => ("bi-file-earmark-code", "fb-icon-code"),
        "yaml"|"yml"|"toml"|"properties"|"conf"|"cfg"|"ini"|"env"|"xml"|"html"|"htm"
            => ("bi-file-earmark-ruled", "fb-icon-config"),
        "jar"|"zip"|"tar"|"gz"|"bz2"|"xz"|"7z"|"rar"|"war"
            => ("bi-file-earmark-zip", "fb-icon-archive"),
        "log"|"txt"|"md"|"rst"|"nfo"
            => ("bi-file-earmark-text", "fb-icon-text"),
        "png"|"jpg"|"jpeg"|"gif"|"ico"|"svg"|"webp"
            => ("bi-file-earmark-image", "fb-icon-config"),
        _ => ("bi-file-earmark", "fb-icon-text"),
    }
}

fn is_extractable(name: &str) -> bool {
    let n = name.to_lowercase();
    n.ends_with(".tar.gz") || n.ends_with(".tgz")
    || n.ends_with(".tar.bz2") || n.ends_with(".tbz2")
    || n.ends_with(".tar.xz") || n.ends_with(".txz")
    || n.ends_with(".tar") || n.ends_with(".zip") || n.ends_with(".jar")
}

/// Builds a breadcrumb bar HTML for the given path.
fn build_breadcrumb(db_id: i64, path: &str) -> String {
    let mut h = String::from(r#"<div class="fb-pathbar">"#);
    h.push_str(&format!(
        r##"<a class="fb-bc-root" hx-get="/api/servers/{}/files/list?path=/" hx-target="#file-browser" hx-swap="outerHTML"><i class="bi bi-house-fill"></i> root</a>"##,
        db_id
    ));
    if path != "/" {
        let segs: Vec<&str> = path.trim_start_matches('/').split('/').filter(|s| !s.is_empty()).collect();
        let mut acc = String::new();
        for (i, seg) in segs.iter().enumerate() {
            acc.push('/');
            acc.push_str(seg);
            h.push_str(r#"<span class="fb-bc-sep">›</span>"#);
            if i == segs.len() - 1 {
                h.push_str(&format!(r#"<span class="fb-bc-seg current">{}</span>"#, escape_html(seg)));
            } else {
                h.push_str(&format!(
                    r##"<a class="fb-bc-seg" hx-get="/api/servers/{}/files/list?path={}" hx-target="#file-browser" hx-swap="outerHTML">{}</a>"##,
                    db_id, escape_html(&acc), escape_html(seg)
                ));
            }
        }
    }
    h.push_str("</div>");
    h
}

pub async fn list_files_api(
    State(state): State<AppState>,
    jar: PrivateCookieJar,
    Path(db_id): Path<i64>,
    Query(query): Query<FileListQuery>,
) -> impl IntoResponse {
    if !auth::can_access_server(&state, &jar, db_id).await {
        return Html(String::from(r#"<div id="file-browser"><p style="color:var(--err);padding:1rem;">Access denied</p></div>"#)).into_response();
    }
    let docker_id = match db::get_container_id_by_server_id(&state.db, db_id).await.ok().flatten() {
        Some(cid) => cid,
        None => return Html(String::from(r#"<div id="file-browser"><p style="color:var(--err);padding:1rem;">Server not found</p></div>"#)).into_response(),
    };
    let path = query.path.unwrap_or_else(|| "/".to_string());
    let volume_dir = docker::get_volume_dir(&state.docker, &docker_id)
        .await
        .unwrap_or_else(|_| docker_id.clone());

    // ── Path traversal guard ──
    let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    let volume_path = cwd.join("volumes").join(&volume_dir);
    if resolve_path(&volume_path, &path).is_none() {
        return Html(String::from(r#"<div id="file-browser"><p style="color:var(--err);padding:1rem;">Access denied</p></div>"#)).into_response();
    }

    let safe_path_attr = escape_html(&path);
    let mut html = format!(
        r#"<div id="file-browser" data-path="{}" hx-trigger="file-created from:body" hx-get="/api/servers/{}/files/list?path={}" hx-swap="outerHTML">"#,
        safe_path_attr, db_id, safe_path_attr
    );

    html.push_str(&build_breadcrumb(db_id, &path));
    html.push_str(r#"<div class="fb-sel-bar" id="fb-sel-bar"><label class="fb-sel-all"><span class="fb-cb-wrap"><input type="checkbox" class="fb-cb" id="fb-cb-all"><span class="fb-cb-box"></span></span>All</label><span class="fb-sel-count" id="fb-sel-count"></span><div class="fb-sel-actions"><button type="button" class="fb-sel-btn fb-sel-btn-copy" id="fb-btn-copy"><i class="bi bi-files"></i> Copy</button><button type="button" class="fb-sel-btn fb-sel-btn-cut" id="fb-btn-cut"><i class="bi bi-scissors"></i> Cut</button><div class="fb-sel-divider"></div><button type="button" class="fb-sel-btn fb-sel-btn-archive" id="fb-btn-archive"><i class="bi bi-file-earmark-zip"></i> Archive</button><div class="fb-sel-divider"></div><button type="button" class="fb-sel-btn fb-sel-btn-delete" id="fb-btn-delete"><i class="bi bi-trash3"></i> Delete</button></div></div>"#);
    html.push_str(r#"<div class="fb-list">"#);

    match docker::list_files(&state.docker, &volume_dir, &path).await {
        Ok(files) => {
            // Back (..) row
            if path != "/" {
                let parent = std::path::Path::new(&path)
                    .parent()
                    .unwrap_or(std::path::Path::new("/"))
                    .to_str()
                    .unwrap_or("/");
                html.push_str(&format!(
                    r##"<a class="fb-row fb-row-back" hx-get="/api/servers/{}/files/list?path={}" hx-target="#file-browser" hx-swap="outerHTML"><div class="fb-icon fb-icon-back"><i class="bi bi-arrow-left"></i></div><div class="fb-name">..</div><div class="fb-meta">back</div></a><div class="fb-divider"></div>"##,
                    db_id, escape_html(parent)
                ));
            }

            if files.is_empty() {
                html.push_str(r#"<div class="fb-empty"><i class="bi bi-folder2-open"></i><div>This folder is empty</div></div>"#);
            }

            for (file, fsize) in &files {
                let is_dir = file.ends_with('/');
                let clean_name = file.trim_end_matches('/');
                let full_path = if path == "/" {
                    format!("/{}", clean_name)
                } else {
                    format!("{}/{}", path.trim_end_matches('/'), clean_name)
                };
                let safe_name = escape_html(clean_name);
                let safe_full = escape_html(&full_path);

                if is_dir {
                    html.push_str(&format!(
                        r##"<a class="fb-row fb-row-dir" data-path="{}" data-type="dir" hx-get="/api/servers/{}/files/list?path={}" hx-target="#file-browser" hx-swap="outerHTML"><label class="fb-cb-wrap" onclick="event.stopPropagation()"><input type="checkbox" class="fb-cb" value="{}"><span class="fb-cb-box"></span></label><div class="fb-icon fb-icon-dir"><i class="bi bi-folder-fill"></i></div><div class="fb-name">{}</div><div class="fb-meta">folder</div></a>"##,
                        safe_full, db_id, safe_full, safe_full, safe_name
                    ));
                } else {
                    let (icon, color) = file_icon(clean_name);
                    let archive_attr = if is_extractable(clean_name) { " data-archive=\"true\"" } else { "" };
                    let meta_label = match fsize {
                        Some(b) => format_size(*b),
                        None => {
                            let raw_ext = clean_name.rsplit('.').next().unwrap_or(clean_name);
                            if raw_ext != clean_name && raw_ext.len() <= 8 {
                                format!(".{}", escape_html(raw_ext))
                            } else {
                                "file".to_string()
                            }
                        }
                    };
                    html.push_str(&format!(
                        r#"<a class="fb-row fb-row-file" data-path="{}" data-type="file"{}  href="/servers/{}/files/edit?path={}"><label class="fb-cb-wrap" onclick="event.stopPropagation()"><input type="checkbox" class="fb-cb" value="{}"><span class="fb-cb-box"></span></label><div class="fb-icon {}"><i class="bi {}"></i></div><div class="fb-name">{}</div><div class="fb-meta">{}</div></a>"#,
                        safe_full, archive_attr, db_id, urlencoding::encode(&full_path),
                        safe_full, color, icon, safe_name, meta_label
                    ));
                }
            }
        }
        Err(e) => {
            html.push_str(&format!(
                r#"<div class="fb-empty"><i class="bi bi-exclamation-triangle" style="color:var(--err)"></i><div style="color:var(--err)">{}</div></div>"#,
                escape_html(&e.to_string())
            ));
        }
    }

    html.push_str("</div></div>"); // close fb-list + file-browser
    Html(html).into_response()
}

pub async fn edit_file_page(
    State(state): State<AppState>,
    jar: PrivateCookieJar,
    Path(db_id): Path<i64>,
    Query(query): Query<FileContentQuery>,
) -> impl IntoResponse {
    if !auth::can_access_server(&state, &jar, db_id).await {
        return Redirect::to("/").into_response();
    }
    let (docker_id, db_name) = match db::get_server_info_by_db_id(&state.db, db_id).await.ok().flatten() {
        Some(row) => row,
        None => return Redirect::to("/").into_response(),
    };
    let volume_dir = docker::get_volume_dir(&state.docker, &docker_id)
        .await
        .unwrap_or_else(|_| docker_id.clone());
    let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    let volume_path = cwd.join("volumes").join(&volume_dir);

    let file_path = match resolve_path(&volume_path, &query.path) {
        Some(p) => p,
        None => return Redirect::to(&format!("/servers/{}/files", db_id)).into_response(),
    };

    let filename = file_path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_string();
    if filename.starts_with('.') || filename.ends_with(".example") || filename.ends_with(".test") {
        return Redirect::to(&format!("/servers/{}/files", db_id)).into_response();
    }

    let container = match crate::docker::get_container(&state.docker, &docker_id).await {
        Ok(mut c) => { c.db_id = db_id; c.name = db_name; c }
        Err(_) => return Redirect::to("/").into_response(),
    };

    let content = tokio::fs::read_to_string(&file_path).await.unwrap_or_default();

    let ace_mode = if filename.ends_with(".json") {
        "ace/mode/json"
    } else if filename.ends_with(".yaml") || filename.ends_with(".yml") {
        "ace/mode/yaml"
    } else if filename.ends_with(".xml") {
        "ace/mode/xml"
    } else if filename.ends_with(".properties") {
        "ace/mode/properties"
    } else {
        "ace/mode/text"
    }
    .to_string();

    super::templates::render(FileEditTemplate {
        id: db_id,
        container,
        path: query.path,
        filename,
        content,
        ace_mode,
        active_tab: "files",
    })
    .into_response()
}

pub async fn save_file_content(
    State(state): State<AppState>,
    jar: PrivateCookieJar,
    Path(db_id): Path<i64>,
    Form(form): Form<SaveFileForm>,
) -> impl IntoResponse {
    if !auth::can_access_server(&state, &jar, db_id).await {
        return (StatusCode::FORBIDDEN, "Access denied").into_response();
    }
    let docker_id = match db::get_container_id_by_server_id(&state.db, db_id).await.ok().flatten() {
        Some(cid) => cid,
        None => return (StatusCode::NOT_FOUND, "Server not found").into_response(),
    };
    let volume_dir = docker::get_volume_dir(&state.docker, &docker_id)
        .await
        .unwrap_or_else(|_| docker_id.clone());
    let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    let volume_path = cwd.join("volumes").join(&volume_dir);

    let file_path = match resolve_path(&volume_path, &form.path) {
        Some(p) => p,
        None => return (StatusCode::FORBIDDEN, "Access Denied").into_response(),
    };
    let rel_path = file_path.strip_prefix(&volume_path)
        .map(|p| p.display().to_string())
        .unwrap_or_default();

    // Try direct write first.
    match tokio::fs::write(&file_path, form.content.as_bytes()).await {
        Ok(_) => return (StatusCode::OK, "ok").into_response(),
        Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
            // Files are root-owned (created by Docker). Fall through to docker write.
        }
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to save: {}", e)).into_response(),
    }

    // Fallback: write via a temporary Alpine container (bypasses root ownership).
    // This is the same pattern used for bandwidth limiting.
    let mount_arg = format!("{}:/mnt:rw", volume_path.display());
    let sh_cmd = format!("cat > '/mnt/{}'", rel_path.replace('\'', "'\\''"));
    let mut child = match tokio::process::Command::new("docker")
        .args(["run", "--rm", "-i", "-v", &mount_arg, "alpine", "sh", "-c", &sh_cmd])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to spawn docker: {}", e)).into_response(),
    };

    if let Some(mut stdin) = child.stdin.take() {
        use tokio::io::AsyncWriteExt;
        let _ = stdin.write_all(form.content.as_bytes()).await;
    }

    match child.wait_with_output().await {
        Ok(out) if out.status.success() => (StatusCode::OK, "ok").into_response(),
        Ok(out) => (StatusCode::INTERNAL_SERVER_ERROR,
            format!("Docker write failed: {}", String::from_utf8_lossy(&out.stderr))).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed: {}", e)).into_response(),
    }
}

pub async fn create_new_file(
    State(state): State<AppState>,
    jar: PrivateCookieJar,
    Path(db_id): Path<i64>,
    Form(form): Form<CreateFileForm>,
) -> impl IntoResponse {
    if !auth::can_access_server(&state, &jar, db_id).await {
        return "Access denied".into_response();
    }
    let docker_id = match db::get_container_id_by_server_id(&state.db, db_id).await.ok().flatten() {
        Some(cid) => cid,
        None => return "Server not found".into_response(),
    };
    let volume_dir = docker::get_volume_dir(&state.docker, &docker_id)
        .await
        .unwrap_or_else(|_| docker_id.clone());
    let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    let volume_path = cwd.join("volumes").join(&volume_dir);

    let name = form.name.trim();
    if name.is_empty() || name.contains('/') || name.contains('\\') || name.starts_with('.') {
        return "Invalid file name (cannot start with dot)".into_response();
    }

    // Resolve the target directory from form.path
    let dir_path = {
        let p = form.path.trim();
        if p.is_empty() || p == "/" {
            volume_path.clone()
        } else {
            match resolve_path(&volume_path, p) {
                Some(p) => p,
                None => return "Access denied".into_response(),
            }
        }
    };
    // Create directory via Docker if needed (root-owned volume)
    let rel_dir = dir_path.strip_prefix(&volume_path)
        .map(|p| p.display().to_string())
        .unwrap_or_default();
    if !dir_path.exists() {
        let cmd = if rel_dir.is_empty() {
            "true".to_string()
        } else {
            format!("mkdir -p '/mnt/{}'", sh_esc(&rel_dir))
        };
        if let Err(e) = docker_volume_cmd(&volume_path, &cmd).await {
            return format!("Failed to create directory: {e}").into_response();
        }
    }

    let file_path = dir_path.join(name);
    if file_path.exists() {
        return "File already exists".into_response();
    }

    let rel_file = file_path.strip_prefix(&volume_path)
        .map(|p| p.display().to_string())
        .unwrap_or_default();
    let create_result = tokio::fs::write(&file_path, "").await;
    if let Err(e) = create_result {
        if e.kind() == std::io::ErrorKind::PermissionDenied {
            let cmd = format!("touch '/mnt/{}'", sh_esc(&rel_file));
            if let Err(e2) = docker_volume_cmd(&volume_path, &cmd).await {
                return format!("Failed to create file: {e2}").into_response();
            }
        } else {
            return format!("Failed to create file: {e}").into_response();
        }
    }

    [(
        axum::http::header::HeaderName::from_static("hx-trigger"),
        "file-created",
    )]
    .into_response()
}

// ── Run a shell command inside Alpine with the volume mounted ───────────────
/// Runs `sh -c {cmd}` inside `docker run --rm alpine` with the volume mounted at /mnt.
/// Returns Ok(()) on success, Err(message) on failure.
async fn docker_volume_cmd(volume_path: &std::path::Path, cmd: &str) -> Result<(), String> {
    let mount_arg = format!("{}:/mnt:rw", volume_path.display());
    let out = tokio::process::Command::new("docker")
        .args(["run", "--rm", "-v", &mount_arg, "alpine", "sh", "-c", cmd])
        .output()
        .await
        .map_err(|e| format!("docker spawn failed: {e}"))?;
    if out.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

/// Shell-escape a path segment for use inside single-quoted strings.
fn sh_esc(s: &str) -> String {
    s.replace('\\', "\\\\").replace('\'', "'\\''")
}

// ── JSON file listing ──────────────────────────────────────────────────────────

#[derive(serde::Serialize)]
pub struct FileJsonEntry {
    name: String,
    path: String,
    is_dir: bool,
    meta: String,
    icon: String,
    color: String,
    is_archive: bool,
}

pub async fn list_files_json(
    State(state): State<AppState>,
    jar: PrivateCookieJar,
    Path(db_id): Path<i64>,
    Query(query): Query<FileListQuery>,
) -> impl IntoResponse {
    if !auth::can_access_server(&state, &jar, db_id).await {
        return (StatusCode::FORBIDDEN, Json(Vec::<FileJsonEntry>::new())).into_response();
    }
    let docker_id = match db::get_container_id_by_server_id(&state.db, db_id).await.ok().flatten() {
        Some(cid) => cid,
        None => return (StatusCode::NOT_FOUND, Json(Vec::<FileJsonEntry>::new())).into_response(),
    };
    let path = query.path.unwrap_or_else(|| "/".to_string());
    let volume_dir = docker::get_volume_dir(&state.docker, &docker_id)
        .await
        .unwrap_or_else(|_| docker_id.clone());

    let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    let volume_path = cwd.join("volumes").join(&volume_dir);
    if resolve_path(&volume_path, &path).is_none() {
        return (StatusCode::FORBIDDEN, Json(Vec::<FileJsonEntry>::new())).into_response();
    }

    match docker::list_files(&state.docker, &volume_dir, &path).await {
        Ok(files) => {
            let entries: Vec<FileJsonEntry> = files.iter().map(|(file, fsize)| {
                let is_dir = file.ends_with('/');
                let clean_name = file.trim_end_matches('/').to_string();
                let full_path = if path == "/" {
                    format!("/{}", clean_name)
                } else {
                    format!("{}/{}", path.trim_end_matches('/'), clean_name)
                };
                if is_dir {
                    FileJsonEntry {
                        name: clean_name,
                        path: full_path,
                        is_dir: true,
                        meta: "folder".to_string(),
                        icon: "bi-folder-fill".to_string(),
                        color: "fb-icon-dir".to_string(),
                        is_archive: false,
                    }
                } else {
                    let (icon, color) = file_icon(&clean_name);
                    let meta = match fsize {
                        Some(b) => format_size(*b),
                        None => {
                            let raw_ext = clean_name.rsplit('.').next().unwrap_or(&clean_name);
                            if raw_ext != clean_name.as_str() && raw_ext.len() <= 8 {
                                format!(".{}", raw_ext)
                            } else {
                                "file".to_string()
                            }
                        }
                    };
                    FileJsonEntry {
                        is_archive: is_extractable(&clean_name),
                        name: clean_name,
                        path: full_path,
                        is_dir: false,
                        meta,
                        icon: icon.to_string(),
                        color: color.to_string(),
                    }
                }
            }).collect();
            Json(entries).into_response()
        }
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, Json(Vec::<FileJsonEntry>::new())).into_response(),
    }
}

// ── Helper to resolve and guard a volume-relative path ───────────────────────
/// Joins `rel` onto `volume_path`, normalizes away any `..`/`.` components, and
/// verifies the result is still inside `volume_path`.  Returns `None` on traversal.
fn resolve_path(
    volume_path: &std::path::Path,
    rel: &str,
) -> Option<std::path::PathBuf> {
    let rel = rel.trim_start_matches('/');
    if rel.is_empty() {
        return Some(volume_path.to_path_buf());
    }
    let joined = volume_path.join(rel);
    // Normalize: resolve `.` and `..` without touching the filesystem.
    let mut normalized = std::path::PathBuf::new();
    for component in joined.components() {
        match component {
            std::path::Component::ParentDir => { normalized.pop(); },
            std::path::Component::CurDir    => {},
            c => normalized.push(c),
        }
    }
    if normalized.starts_with(volume_path) { Some(normalized) } else { None }
}

// ── DELETE a file or directory ────────────────────────────────────────────────
pub async fn delete_file(
    State(state): State<AppState>,
    jar: PrivateCookieJar,
    Path(db_id): Path<i64>,
    Query(query): Query<DeleteFileQuery>,
) -> impl IntoResponse {
    if !auth::can_access_server(&state, &jar, db_id).await {
        return (StatusCode::FORBIDDEN, "Access denied").into_response();
    }
    let docker_id = match db::get_container_id_by_server_id(&state.db, db_id).await.ok().flatten() {
        Some(cid) => cid,
        None => return (StatusCode::NOT_FOUND, "Server not found").into_response(),
    };
    let volume_dir = docker::get_volume_dir(&state.docker, &docker_id).await
        .unwrap_or_else(|_| docker_id.clone());
    let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    let volume_path = cwd.join("volumes").join(&volume_dir);

    let target = match resolve_path(&volume_path, &query.path) {
        Some(p) => p,
        None => return (StatusCode::FORBIDDEN, "Access denied").into_response(),
    };
    if !target.exists() {
        return (StatusCode::NOT_FOUND, "Not found").into_response();
    }

    let result = if target.is_dir() {
        tokio::fs::remove_dir_all(&target).await
    } else {
        tokio::fs::remove_file(&target).await
    };

    match result {
        Ok(_) => (
            StatusCode::OK,
            axum::http::HeaderMap::from_iter([(
                axum::http::header::HeaderName::from_static("hx-trigger"),
                axum::http::HeaderValue::from_static("file-created"),
            )]),
        ).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

// ── RENAME a file or directory ────────────────────────────────────────────────
pub async fn rename_file(
    State(state): State<AppState>,
    jar: PrivateCookieJar,
    Path(db_id): Path<i64>,
    Form(form): Form<RenameFileForm>,
) -> impl IntoResponse {
    if !auth::can_access_server(&state, &jar, db_id).await {
        return (StatusCode::FORBIDDEN, "Access denied").into_response();
    }
    let docker_id = match db::get_container_id_by_server_id(&state.db, db_id).await.ok().flatten() {
        Some(cid) => cid,
        None => return (StatusCode::NOT_FOUND, "Server not found").into_response(),
    };
    let volume_dir = docker::get_volume_dir(&state.docker, &docker_id).await
        .unwrap_or_else(|_| docker_id.clone());
    let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    let volume_path = cwd.join("volumes").join(&volume_dir);

    let src = match resolve_path(&volume_path, &form.path) {
        Some(p) => p,
        None => return (StatusCode::FORBIDDEN, "Access denied").into_response(),
    };
    let new_name = form.new_name.trim();
    if new_name.is_empty() || new_name.contains('/') || new_name.contains('\\') {
        return (StatusCode::BAD_REQUEST, "Invalid name").into_response();
    }
    let dst = match src.parent() {
        Some(parent) => parent.join(new_name),
        None => return (StatusCode::BAD_REQUEST, "Cannot rename root").into_response(),
    };
    if !dst.starts_with(&volume_path) {
        return (StatusCode::FORBIDDEN, "Access denied").into_response();
    }
    if dst.exists() {
        return (StatusCode::CONFLICT, "Name already exists").into_response();
    }
    match tokio::fs::rename(&src, &dst).await {
        Ok(_) => (
            StatusCode::OK,
            axum::http::HeaderMap::from_iter([(
                axum::http::header::HeaderName::from_static("hx-trigger"),
                axum::http::HeaderValue::from_static("file-created"),
            )]),
        ).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

// ── COPY a file or directory to a destination directory ──────────────────────
pub async fn copy_file(
    State(state): State<AppState>,
    jar: PrivateCookieJar,
    Path(db_id): Path<i64>,
    Form(form): Form<CopyFileForm>,
) -> impl IntoResponse {
    if !auth::can_access_server(&state, &jar, db_id).await {
        return (StatusCode::FORBIDDEN, "Access denied").into_response();
    }
    let docker_id = match db::get_container_id_by_server_id(&state.db, db_id).await.ok().flatten() {
        Some(cid) => cid,
        None => return (StatusCode::NOT_FOUND, "Server not found").into_response(),
    };
    let volume_dir = docker::get_volume_dir(&state.docker, &docker_id).await
        .unwrap_or_else(|_| docker_id.clone());
    let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    let volume_path = cwd.join("volumes").join(&volume_dir);

    let src = match resolve_path(&volume_path, &form.src) {
        Some(p) => p,
        None => return (StatusCode::FORBIDDEN, "Access denied (src)").into_response(),
    };
    let dst_dir = match resolve_path(&volume_path, &form.dst_dir) {
        Some(p) => p,
        None => return (StatusCode::FORBIDDEN, "Access denied (dst)").into_response(),
    };
    if !src.exists() {
        return (StatusCode::NOT_FOUND, "Source not found").into_response();
    }
    let fname = match src.file_name().and_then(|s| s.to_str()) {
        Some(n) => n.to_string(),
        None => return (StatusCode::BAD_REQUEST, "Bad source name").into_response(),
    };

    // Avoid overwriting: if target exists, append "_copy"
    let mut dst = dst_dir.join(&fname);
    if dst.exists() {
        let stem = src.file_stem().and_then(|s| s.to_str()).unwrap_or(&fname);
        let ext = src.extension().and_then(|s| s.to_str()).map(|e| format!(".{}", e)).unwrap_or_default();
        dst = dst_dir.join(format!("{}_copy{}", stem, ext));
    }
    if !dst.starts_with(&volume_path) {
        return (StatusCode::FORBIDDEN, "Access denied (dst path)").into_response();
    }

    let copy_result: Result<(), String> = if src.is_dir() {
        // Always use Docker for directory copy (cp -r)
        let rel_src = src.strip_prefix(&volume_path)
            .map(|p| p.display().to_string()).unwrap_or_default();
        let rel_dst = dst.strip_prefix(&volume_path)
            .map(|p| p.display().to_string()).unwrap_or_default();
        let cmd = format!("cp -r '/mnt/{}' '/mnt/{}'", sh_esc(&rel_src), sh_esc(&rel_dst));
        docker_volume_cmd(&volume_path, &cmd).await
    } else {
        match tokio::fs::copy(&src, &dst).await {
            Ok(_) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
                let rel_src = src.strip_prefix(&volume_path)
                    .map(|p| p.display().to_string()).unwrap_or_default();
                let rel_dst = dst.strip_prefix(&volume_path)
                    .map(|p| p.display().to_string()).unwrap_or_default();
                let cmd = format!("cp '/mnt/{}' '/mnt/{}'", sh_esc(&rel_src), sh_esc(&rel_dst));
                docker_volume_cmd(&volume_path, &cmd).await
            }
            Err(e) => Err(e.to_string()),
        }
    };
    if let Err(e) = copy_result {
        return (StatusCode::INTERNAL_SERVER_ERROR, e).into_response();
    }

    (
        StatusCode::OK,
        axum::http::HeaderMap::from_iter([(
            axum::http::header::HeaderName::from_static("hx-trigger"),
            axum::http::HeaderValue::from_static("file-created"),
        )]),
    ).into_response()
}

// ── UPLOAD files via multipart form ──────────────────────────────────────────
pub async fn upload_files(
    State(state): State<AppState>,
    jar: PrivateCookieJar,
    Path(db_id): Path<i64>,
    Query(query): Query<FileUploadQuery>,
    mut multipart: Multipart,
) -> impl IntoResponse {
    if !auth::can_access_server(&state, &jar, db_id).await {
        return (StatusCode::FORBIDDEN, "Access denied").into_response();
    }
    let docker_id = match db::get_container_id_by_server_id(&state.db, db_id).await.ok().flatten() {
        Some(cid) => cid,
        None => return (StatusCode::NOT_FOUND, "Server not found").into_response(),
    };
    let volume_dir = docker::get_volume_dir(&state.docker, &docker_id).await
        .unwrap_or_else(|_| docker_id.clone());
    let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    let volume_path = cwd.join("volumes").join(&volume_dir);

    let dest_dir = match resolve_path(&volume_path, &query.path) {
        Some(p) => p,
        None => return (StatusCode::FORBIDDEN, "Access denied").into_response(),
    };
    if let Err(e) = tokio::fs::create_dir_all(&dest_dir).await {
        return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response();
    }

    let mut saved = 0u32;
    let mut last_err = String::new();

    loop {
        let field_result = multipart.next_field().await;
        let mut field = match field_result {
            Ok(Some(f)) => f,
            Ok(None) => break,
            Err(e) => { last_err = format!("multipart read error: {e}"); break; }
        };

        let filename = field.file_name()
            .map(|s| s.to_string())
            .or_else(|| field.name().map(|s| s.to_string()))
            .unwrap_or_else(|| "upload".to_string());
        if filename.is_empty() { last_err = "empty filename".to_string(); continue; }

        // Sanitise — strip directory components
        let fname = std::path::Path::new(&filename)
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("upload")
            .to_string();

        let file_path = dest_dir.join(&fname);
        if !file_path.starts_with(&volume_path) {
            last_err = format!("path traversal blocked: {fname}");
            continue;
        }

        // Always stream to a temp file first (volume dirs are root-owned)
        let tmp_path = std::env::temp_dir().join(format!("yxupload_{}", fname));
        let mut f = match tokio::fs::File::create(&tmp_path).await {
            Ok(f) => f,
            Err(e) => { last_err = format!("tmp create failed: {e}"); continue; }
        };

        // Stream chunks to temp file
        let mut stream_ok = true;
        loop {
            match field.chunk().await {
                Ok(Some(chunk)) => {
                    use tokio::io::AsyncWriteExt;
                    if let Err(e) = f.write_all(&chunk).await {
                        last_err = format!("write error for {fname}: {e}");
                        stream_ok = false;
                        break;
                    }
                }
                Ok(None) => {
                    use tokio::io::AsyncWriteExt;
                    let _ = f.flush().await;
                    break;
                }
                Err(e) => {
                    last_err = format!("chunk read error for {fname}: {e}");
                    stream_ok = false;
                    break;
                }
            }
        }
        if !stream_ok {
            let _ = tokio::fs::remove_file(&tmp_path).await;
            continue;
        }
        drop(f); // close the file before moving

        // Try direct rename (same FS), then direct copy, then docker fallback
        let tmp_str = tmp_path.display().to_string();

        let installed = if tokio::fs::rename(&tmp_path, &file_path).await.is_ok() {
            true
        } else if tokio::fs::copy(&tmp_path, &file_path).await.is_ok() {
            let _ = tokio::fs::remove_file(&tmp_path).await;
            true
        } else {
            // Fallback: docker run with both volumes mounted, copy inside container
            let mount_vol = format!("{}:/mnt:rw", volume_path.display());
            let mount_tmp = format!("{}:/srcfile:ro", tmp_str);
            let rel_dst = file_path.strip_prefix(&volume_path)
                .map(|p| p.display().to_string())
                .unwrap_or_else(|_| fname.clone());
            let out = tokio::process::Command::new("docker")
                .args(["run", "--rm",
                    "-v", &mount_tmp,
                    "-v", &mount_vol,
                    "alpine",
                    "cp", "/srcfile", &format!("/mnt/{}", rel_dst)])
                .output().await;
            let _ = tokio::fs::remove_file(&tmp_path).await;
            match out {
                Ok(o) if o.status.success() => true,
                Ok(o) => {
                    last_err = format!("docker cp failed for {fname}: {}",
                        String::from_utf8_lossy(&o.stderr));
                    false
                }
                Err(e) => { last_err = format!("docker spawn failed: {e}"); false }
            }
        };
        if installed { saved += 1; }
    }

    if saved == 0 {
        let msg = if last_err.is_empty() {
            "No files received (no multipart fields found)".to_string()
        } else {
            format!("Upload failed: {last_err}")
        };
        return (StatusCode::BAD_REQUEST, msg).into_response();
    }

    (
        StatusCode::OK,
        axum::http::HeaderMap::from_iter([(
            axum::http::header::HeaderName::from_static("hx-trigger"),
            axum::http::HeaderValue::from_static("file-created"),
        )]),
    ).into_response()
}

// ── EXTRACT an archive file in-place ─────────────────────────────────────────
pub async fn extract_archive(
    State(state): State<AppState>,
    jar: PrivateCookieJar,
    Path(db_id): Path<i64>,
    Form(form): Form<ExtractForm>,
) -> impl IntoResponse {
    if !auth::can_access_server(&state, &jar, db_id).await {
        return (StatusCode::FORBIDDEN, "Access denied").into_response();
    }
    let docker_id = match db::get_container_id_by_server_id(&state.db, db_id).await.ok().flatten() {
        Some(cid) => cid,
        None => return (StatusCode::NOT_FOUND, "Server not found").into_response(),
    };
    let volume_dir = docker::get_volume_dir(&state.docker, &docker_id).await
        .unwrap_or_else(|_| docker_id.clone());
    let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    let volume_path = cwd.join("volumes").join(&volume_dir);

    let archive_path = match resolve_path(&volume_path, &form.path) {
        Some(p) => p,
        None => return (StatusCode::FORBIDDEN, "Access denied").into_response(),
    };
    if !archive_path.exists() {
        return (StatusCode::NOT_FOUND, "Archive not found").into_response();
    }

    let arch_name = match archive_path.file_name().and_then(|s| s.to_str()) {
        Some(n) => n.to_string(),
        None => return (StatusCode::BAD_REQUEST, "Invalid path").into_response(),
    };
    let arch_dir = match archive_path.parent() {
        Some(p) => p.to_path_buf(),
        None => return (StatusCode::BAD_REQUEST, "Invalid path").into_response(),
    };
    let rel_dir = arch_dir.strip_prefix(&volume_path)
        .map(|p| p.display().to_string())
        .unwrap_or_default();
    let cd_part = if rel_dir.is_empty() {
        "cd /mnt".to_string()
    } else {
        format!("cd '/mnt/{}'", sh_esc(&rel_dir))
    };

    let name_lower = arch_name.to_lowercase();
    let sh_cmd = if name_lower.ends_with(".tar.gz") || name_lower.ends_with(".tgz") {
        format!("{} && tar xzf '{}'", cd_part, sh_esc(&arch_name))
    } else if name_lower.ends_with(".tar.bz2") || name_lower.ends_with(".tbz2") {
        format!("{} && tar xjf '{}'", cd_part, sh_esc(&arch_name))
    } else if name_lower.ends_with(".tar.xz") || name_lower.ends_with(".txz") {
        format!("{} && tar xJf '{}'", cd_part, sh_esc(&arch_name))
    } else if name_lower.ends_with(".tar") {
        format!("{} && tar xf '{}'", cd_part, sh_esc(&arch_name))
    } else if name_lower.ends_with(".zip") || name_lower.ends_with(".jar") {
        format!("{} && unzip -o '{}'", cd_part, sh_esc(&arch_name))
    } else {
        return (StatusCode::BAD_REQUEST, "Unsupported archive format").into_response();
    };

    match docker_volume_cmd(&volume_path, &sh_cmd).await {
        Ok(_) => (
            StatusCode::OK,
            axum::http::HeaderMap::from_iter([(
                axum::http::header::HeaderName::from_static("hx-trigger"),
                axum::http::HeaderValue::from_static("file-created"),
            )]),
        ).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, format!("Extract failed: {e}")).into_response(),
    }
}

// ── CREATE a tar.gz archive from selected items ───────────────────────────────
pub async fn create_archive(
    State(state): State<AppState>,
    jar: PrivateCookieJar,
    Path(db_id): Path<i64>,
    Form(form): Form<ArchiveForm>,
) -> impl IntoResponse {
    if !auth::can_access_server(&state, &jar, db_id).await {
        return (StatusCode::FORBIDDEN, "Access denied").into_response();
    }
    let docker_id = match db::get_container_id_by_server_id(&state.db, db_id).await.ok().flatten() {
        Some(cid) => cid,
        None => return (StatusCode::NOT_FOUND, "Server not found").into_response(),
    };
    let volume_dir = docker::get_volume_dir(&state.docker, &docker_id).await
        .unwrap_or_else(|_| docker_id.clone());
    let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    let volume_path = cwd.join("volumes").join(&volume_dir);

    let base_dir = match resolve_path(&volume_path, &form.dir) {
        Some(p) => p,
        None => return (StatusCode::FORBIDDEN, "Access denied").into_response(),
    };

    // Sanitize archive name
    let raw_name = form.name.trim();
    if raw_name.is_empty() || raw_name.contains('/') || raw_name.contains('\\') || raw_name.starts_with('.') {
        return (StatusCode::BAD_REQUEST, "Invalid archive name").into_response();
    }
    let archive_name = if raw_name.ends_with(".tar.gz") {
        raw_name.to_string()
    } else {
        format!("{}.tar.gz", raw_name)
    };

    // Parse + validate paths (newline-separated full volume paths)
    let names: Vec<String> = form.paths
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() { return None; }
            let item = resolve_path(&volume_path, line)?;
            if item.parent()? != base_dir { return None; }
            let name = item.file_name()?.to_str()?.to_string();
            if name.is_empty() || name.contains('/') { return None; }
            Some(name)
        })
        .collect();

    if names.is_empty() {
        return (StatusCode::BAD_REQUEST, "No valid items selected").into_response();
    }

    let rel_dir = base_dir.strip_prefix(&volume_path)
        .map(|p| p.display().to_string())
        .unwrap_or_default();
    let cd_part = if rel_dir.is_empty() {
        "cd /mnt".to_string()
    } else {
        format!("cd '/mnt/{}'", sh_esc(&rel_dir))
    };
    let targets: String = names.iter()
        .map(|n| format!("'{}'", sh_esc(n)))
        .collect::<Vec<_>>()
        .join(" ");
    let sh_cmd = format!("{} && tar czf '{}' {}", cd_part, sh_esc(&archive_name), targets);

    match docker_volume_cmd(&volume_path, &sh_cmd).await {
        Ok(_) => (
            StatusCode::OK,
            axum::http::HeaderMap::from_iter([(
                axum::http::header::HeaderName::from_static("hx-trigger"),
                axum::http::HeaderValue::from_static("file-created"),
            )]),
        ).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, format!("Archive failed: {e}")).into_response(),
    }
}

// ── BULK DELETE multiple paths ────────────────────────────────────────────────
pub async fn bulk_delete(
    State(state): State<AppState>,
    jar: PrivateCookieJar,
    Path(db_id): Path<i64>,
    Form(form): Form<super::templates::BulkPathsForm>,
) -> impl IntoResponse {
    if !auth::can_access_server(&state, &jar, db_id).await {
        return (StatusCode::FORBIDDEN, "Access denied").into_response();
    }
    let docker_id = match db::get_container_id_by_server_id(&state.db, db_id).await.ok().flatten() {
        Some(cid) => cid,
        None => return (StatusCode::NOT_FOUND, "Server not found").into_response(),
    };
    let volume_dir = docker::get_volume_dir(&state.docker, &docker_id).await
        .unwrap_or_else(|_| docker_id.clone());
    let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    let volume_path = cwd.join("volumes").join(&volume_dir);

    let mut errors: Vec<String> = Vec::new();
    for line in form.paths.lines() {
        let p = line.trim();
        if p.is_empty() { continue; }
        let target = match resolve_path(&volume_path, p) {
            Some(t) => t,
            None => { errors.push(format!("Access denied: {}", p)); continue; }
        };
        let res = if target.is_dir() {
            tokio::fs::remove_dir_all(&target).await
        } else {
            tokio::fs::remove_file(&target).await
        };
        if let Err(e) = res { errors.push(format!("{}: {}", p, e)); }
    }
    if !errors.is_empty() {
        return (StatusCode::INTERNAL_SERVER_ERROR, errors.join("\n")).into_response();
    }
    (
        StatusCode::OK,
        axum::http::HeaderMap::from_iter([(
            axum::http::header::HeaderName::from_static("hx-trigger"),
            axum::http::HeaderValue::from_static("file-created"),
        )]),
    ).into_response()
}

// ── MOVE a file/directory to another directory ────────────────────────────────
pub async fn move_file(
    State(state): State<AppState>,
    jar: PrivateCookieJar,
    Path(db_id): Path<i64>,
    Form(form): Form<super::templates::CopyFileForm>,
) -> impl IntoResponse {
    if !auth::can_access_server(&state, &jar, db_id).await {
        return (StatusCode::FORBIDDEN, "Access denied").into_response();
    }
    let docker_id = match db::get_container_id_by_server_id(&state.db, db_id).await.ok().flatten() {
        Some(cid) => cid,
        None => return (StatusCode::NOT_FOUND, "Server not found").into_response(),
    };
    let volume_dir = docker::get_volume_dir(&state.docker, &docker_id).await
        .unwrap_or_else(|_| docker_id.clone());
    let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    let volume_path = cwd.join("volumes").join(&volume_dir);

    let src = match resolve_path(&volume_path, &form.src) {
        Some(p) => p,
        None => return (StatusCode::FORBIDDEN, "Access denied (src)").into_response(),
    };
    let dst_dir = match resolve_path(&volume_path, &form.dst_dir) {
        Some(p) => p,
        None => return (StatusCode::FORBIDDEN, "Access denied (dst)").into_response(),
    };
    if !src.exists() {
        return (StatusCode::NOT_FOUND, "Source not found").into_response();
    }
    let fname = match src.file_name().and_then(|s| s.to_str()) {
        Some(n) => n.to_string(),
        None => return (StatusCode::BAD_REQUEST, "Bad source name").into_response(),
    };
    let dst = dst_dir.join(&fname);
    if !dst.starts_with(&volume_path) {
        return (StatusCode::FORBIDDEN, "Access denied (dst path)").into_response();
    }
    if dst.exists() {
        return (StatusCode::CONFLICT, "Destination already exists").into_response();
    }
    // Try cheap rename; fall back to Alpine mv for cross-device
    match tokio::fs::rename(&src, &dst).await {
        Ok(_) => {}
        Err(e) => {
            let rel_src = src.strip_prefix(&volume_path)
                .map(|p| p.display().to_string()).unwrap_or_default();
            let rel_dst = dst.strip_prefix(&volume_path)
                .map(|p| p.display().to_string()).unwrap_or_default();
            let cmd = format!("mv '/mnt/{}' '/mnt/{}'", sh_esc(&rel_src), sh_esc(&rel_dst));
            if let Err(e2) = docker_volume_cmd(&volume_path, &cmd).await {
                return (StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Move failed: {} / {}", e, e2)).into_response();
            }
        }
    }
    (
        StatusCode::OK,
        axum::http::HeaderMap::from_iter([(
            axum::http::header::HeaderName::from_static("hx-trigger"),
            axum::http::HeaderValue::from_static("file-created"),
        )]),
    ).into_response()
}
