use axum::{
    body::Body,
    extract::{Extension, Multipart, Path, State},
    http::{header, StatusCode},
    response::IntoResponse,
    Json,
};
use axum_extra::extract::cookie::PrivateCookieJar;
use serde::{Deserialize, Serialize};
use tokio_util::io::ReaderStream;
use crate::{auth, db, docker};
use crate::state::AppState;
use super::CspNonce;
use super::templates::{render, BackupsTemplate};

pub async fn backups_page(
    State(state): State<AppState>,
    jar: PrivateCookieJar,
    Path(db_id): Path<i64>,
    Extension(CspNonce(nonce)): Extension<CspNonce>,
) -> impl IntoResponse {
    if !auth::can_access_server_permission(&state, &jar, db_id, "files", false).await {
        return (StatusCode::FORBIDDEN, "Access denied").into_response();
    }
    let can_write = auth::can_access_server_permission(&state, &jar, db_id, "files", true).await;
    let can_members = auth::can_access_server_permission(&state, &jar, db_id, "members", false).await;
    let is_admin = auth::is_admin_session(&state, &jar).await;
    let max_backups = db::get_server_max_backups(&state.db, db_id).await;
    let (docker_id, db_name) = match db::get_server_info_by_db_id(&state.db, db_id).await {
        Ok(Some(v)) => v,
        _ => return (StatusCode::NOT_FOUND, "Server not found").into_response(),
    };
    match docker::get_container(&state.docker, &docker_id).await {
        Ok(mut c) => {
            c.db_id = db_id;
            c.name = db_name;
            render(BackupsTemplate {
                id: db_id,
                container: c,
                can_members,
                can_write,
                is_admin,
                max_backups,
                active_tab: "backups",
                nonce,
            }).into_response()
        }
        Err(e) => format!("Error: {e}").into_response(),
    }
}

fn is_safe_filename(name: &str) -> bool {
    !name.is_empty()
        && !name.contains('/')
        && !name.contains('\\')
        && !name.contains("..")
        && name.ends_with(".tar.gz")
        && name.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_' || c == '.')
}

#[derive(Serialize)]
struct BackupFileInfo {
    name: String,
    size_bytes: u64,
    created_at: String,
}

pub async fn api_list_backups(
    State(state): State<AppState>,
    jar: PrivateCookieJar,
    Path(db_id): Path<i64>,
) -> impl IntoResponse {
    if !auth::can_access_server_permission(&state, &jar, db_id, "files", false).await {
        return (StatusCode::FORBIDDEN, Json(serde_json::json!({"error":"Access denied"}))).into_response();
    }
    let docker_id = match db::get_container_id_by_server_id(&state.db, db_id).await.ok().flatten() {
        Some(cid) => cid,
        None => return (StatusCode::NOT_FOUND, Json(serde_json::json!({"error":"Server not found"}))).into_response(),
    };
    let volume_dir = match docker::get_volume_dir(&state.docker, &docker_id).await {
        Ok(v) => v,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e.to_string()}))).into_response(),
    };
    let backups_path = docker::backup_dir_for_volume(&docker::volume_dir_to_path(&volume_dir));

    let mut files: Vec<BackupFileInfo> = Vec::new();
    if let Ok(mut rd) = tokio::fs::read_dir(&backups_path).await {
        while let Ok(Some(entry)) = rd.next_entry().await {
            let name = entry.file_name().to_string_lossy().into_owned();
            if !name.ends_with(".tar.gz") {
                continue;
            }
            if let Ok(meta) = entry.metadata().await {
                use chrono::TimeZone as _;
                let created_at = meta.modified()
                    .ok()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| {
                        chrono::Utc.timestamp_opt(d.as_secs() as i64, 0)
                            .single()
                            .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
                            .unwrap_or_else(|| String::from("—"))
                    })
                    .unwrap_or_else(|| String::from("—"));
                files.push(BackupFileInfo { name, size_bytes: meta.len(), created_at });
            }
        }
    }
    files.sort_by(|a, b| b.name.cmp(&a.name));
    let max_backups = db::get_server_max_backups(&state.db, db_id).await;
    Json(serde_json::json!({
        "backups": files,
        "max_backups": max_backups,
    })).into_response()
}

#[derive(Deserialize)]
pub struct CreateBackupBody {
    #[serde(default)]
    pub prefix: String,
}

pub async fn api_create_backup(
    State(state): State<AppState>,
    jar: PrivateCookieJar,
    Path(db_id): Path<i64>,
    Json(body): Json<CreateBackupBody>,
) -> impl IntoResponse {
    if !auth::can_access_server_permission(&state, &jar, db_id, "files", true).await {
        return (StatusCode::FORBIDDEN, Json(serde_json::json!({"error":"Access denied"}))).into_response();
    }
    let docker_id = match db::get_container_id_by_server_id(&state.db, db_id).await.ok().flatten() {
        Some(cid) => cid,
        None => return (StatusCode::NOT_FOUND, Json(serde_json::json!({"error":"Server not found"}))).into_response(),
    };

    // Enforce backup limit
    let max_backups = db::get_server_max_backups(&state.db, db_id).await;
    let volume_dir = match docker::get_volume_dir(&state.docker, &docker_id).await {
        Ok(v) => v,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e.to_string()}))).into_response(),
    };
    let backups_path = docker::backup_dir_for_volume(&docker::volume_dir_to_path(&volume_dir));
    let current_count = count_backups(&backups_path).await;
    if current_count >= max_backups as usize {
        return (StatusCode::UNPROCESSABLE_ENTITY, Json(serde_json::json!({
            "error": format!("Backup limit reached ({}/{}). Delete a backup first.", current_count, max_backups)
        }))).into_response();
    }

    let prefix = if body.prefix.trim().is_empty() { "backup".to_string() } else { body.prefix.trim().to_string() };
    let payload = serde_json::json!({"prefix": prefix}).to_string();

    match crate::scheduler::run_backup(&state.docker, &docker_id, &payload).await {
        Ok(msg) => Json(serde_json::json!({"ok": true, "message": msg})).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e.to_string()}))).into_response(),
    }
}

async fn count_backups(backups_path: &std::path::Path) -> usize {
    let mut count = 0usize;
    if let Ok(mut rd) = tokio::fs::read_dir(backups_path).await {
        while let Ok(Some(entry)) = rd.next_entry().await {
            if entry.file_name().to_string_lossy().ends_with(".tar.gz") {
                count += 1;
            }
        }
    }
    count
}

pub async fn api_delete_backup(
    State(state): State<AppState>,
    jar: PrivateCookieJar,
    Path((db_id, filename)): Path<(i64, String)>,
) -> impl IntoResponse {
    if !auth::can_access_server_permission(&state, &jar, db_id, "files", true).await {
        return (StatusCode::FORBIDDEN, Json(serde_json::json!({"error":"Access denied"}))).into_response();
    }
    if !is_safe_filename(&filename) {
        return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error":"Invalid filename"}))).into_response();
    }
    let docker_id = match db::get_container_id_by_server_id(&state.db, db_id).await.ok().flatten() {
        Some(cid) => cid,
        None => return (StatusCode::NOT_FOUND, Json(serde_json::json!({"error":"Server not found"}))).into_response(),
    };
    let volume_dir = match docker::get_volume_dir(&state.docker, &docker_id).await {
        Ok(v) => v,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e.to_string()}))).into_response(),
    };
    let path = docker::backup_dir_for_volume(&docker::volume_dir_to_path(&volume_dir)).join(&filename);
    match tokio::fs::remove_file(&path).await {
        Ok(_) => Json(serde_json::json!({"ok": true})).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e.to_string()}))).into_response(),
    }
}

pub async fn api_download_backup(
    State(state): State<AppState>,
    jar: PrivateCookieJar,
    Path((db_id, filename)): Path<(i64, String)>,
) -> impl IntoResponse {
    if !auth::can_access_server_permission(&state, &jar, db_id, "files", false).await {
        return (StatusCode::FORBIDDEN, "Access denied").into_response();
    }
    if !auth::is_admin_session(&state, &jar).await {
        return (StatusCode::FORBIDDEN, "Admin only").into_response();
    }
    if !is_safe_filename(&filename) {
        return (StatusCode::BAD_REQUEST, "Invalid filename").into_response();
    }
    let docker_id = match db::get_container_id_by_server_id(&state.db, db_id).await.ok().flatten() {
        Some(cid) => cid,
        None => return (StatusCode::NOT_FOUND, "Server not found").into_response(),
    };
    let volume_dir = match docker::get_volume_dir(&state.docker, &docker_id).await {
        Ok(v) => v,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, format!("Error: {e}")).into_response(),
    };
    let path = docker::backup_dir_for_volume(&docker::volume_dir_to_path(&volume_dir)).join(&filename);
    let meta = match tokio::fs::metadata(&path).await {
        Ok(m) if m.is_file() => m,
        Ok(_) => return (StatusCode::BAD_REQUEST, "Not a file").into_response(),
        Err(_) => return (StatusCode::NOT_FOUND, "File not found").into_response(),
    };
    let file_size = meta.len();
    let file = match tokio::fs::File::open(&path).await {
        Ok(f) => f,
        Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR, "Cannot open file").into_response(),
    };
    let encoded_name = urlencoding::encode(&filename).into_owned();
    let disposition = format!(
        "attachment; filename=\"{}\"; filename*=UTF-8''{}",
        filename.replace('"', "\\\""),
        encoded_name
    );
    let stream = ReaderStream::new(file);
    let body = Body::from_stream(stream);
    axum::response::Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/gzip")
        .header(header::CONTENT_DISPOSITION, &disposition)
        .header(header::CONTENT_LENGTH, file_size.to_string())
        .body(body)
        .unwrap()
        .into_response()
}

pub async fn api_restore_backup(
    State(state): State<AppState>,
    jar: PrivateCookieJar,
    Path(db_id): Path<i64>,
    mut multipart: Multipart,
) -> impl IntoResponse {
    if !auth::can_access_server_permission(&state, &jar, db_id, "files", true).await {
        return (StatusCode::FORBIDDEN, Json(serde_json::json!({"error":"Access denied"}))).into_response();
    }
    let docker_id = match db::get_container_id_by_server_id(&state.db, db_id).await.ok().flatten() {
        Some(cid) => cid,
        None => return (StatusCode::NOT_FOUND, Json(serde_json::json!({"error":"Server not found"}))).into_response(),
    };
    let volume_dir = match docker::get_volume_dir(&state.docker, &docker_id).await {
        Ok(v) => v,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e.to_string()}))).into_response(),
    };
    let volume_path = docker::volume_dir_to_path(&volume_dir);

    // Stream uploaded file to a temp path
    let tmp_path = std::path::PathBuf::from(format!("/tmp/yxrestore_{}.tar.gz", uuid_hex()));
    let mut found = false;

    while let Ok(Some(mut field)) = multipart.next_field().await {
        let fname = field.file_name().map(|s| s.to_string()).unwrap_or_default();
        if !fname.ends_with(".tar.gz") {
            continue;
        }
        found = true;
        use tokio::io::AsyncWriteExt as _;
        let mut f = match tokio::fs::File::create(&tmp_path).await {
            Ok(f) => f,
            Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e.to_string()}))).into_response(),
        };
        while let Ok(Some(chunk)) = field.chunk().await {
            if f.write_all(&chunk).await.is_err() {
                let _ = tokio::fs::remove_file(&tmp_path).await;
                return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error":"Write error"}))).into_response();
            }
        }
        break;
    }

    if !found {
        return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error":"No .tar.gz file uploaded"}))).into_response();
    }

    let tmp_path2 = tmp_path.clone();
    let volume_path2 = volume_path.clone();
    let result = tokio::task::spawn_blocking(move || {
        restore_to_volume(&volume_path2, &tmp_path2)
    }).await;

    let _ = tokio::fs::remove_file(&tmp_path).await;

    match result {
        Ok(Ok(())) => Json(serde_json::json!({"ok": true})).into_response(),
        Ok(Err(e)) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e}))).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e.to_string()}))).into_response(),
    }
}

fn uuid_hex() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let t = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    format!("{:x}", t)
}

fn restore_to_volume(volume_path: &std::path::Path, archive: &std::path::Path) -> Result<(), String> {
    // Try direct extraction first (panel runs as root or volume is accessible)
    if let Ok(()) = try_direct_extract(volume_path, archive) {
        return Ok(());
    }
    // Fallback: use docker run alpine with volume mounts (handles root-owned volumes)
    docker_extract(volume_path, archive)
}

fn try_direct_extract(volume_path: &std::path::Path, archive: &std::path::Path) -> Result<(), String> {
    use flate2::read::GzDecoder;
    use std::fs;

    // Clear volume contents
    for entry in fs::read_dir(volume_path).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let p = entry.path();
        if p.is_dir() {
            fs::remove_dir_all(&p).map_err(|e| format!("rm dir {:?}: {e}", p))?;
        } else {
            fs::remove_file(&p).map_err(|e| format!("rm file {:?}: {e}", p))?;
        }
    }

    // Extract archive
    let f = fs::File::open(archive).map_err(|e| e.to_string())?;
    let gz = GzDecoder::new(f);
    let mut tar = tar::Archive::new(gz);
    tar.set_preserve_permissions(true);
    tar.unpack(volume_path).map_err(|e| e.to_string())
}

fn docker_extract(volume_path: &std::path::Path, archive: &std::path::Path) -> Result<(), String> {
    let vol_mount = format!("{}:/mnt:rw", volume_path.display());
    let arc_mount = format!("{}:/restore.tar.gz:ro", archive.display());
    // Clear volume then extract, preserving ownership of whatever is inside
    let out = std::process::Command::new("docker")
        .args([
            "run", "--rm",
            "-v", &vol_mount,
            "-v", &arc_mount,
            "alpine",
            "sh", "-c",
            "find /mnt -mindepth 1 -delete && tar xzf /restore.tar.gz -C /mnt",
        ])
        .output()
        .map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).to_string())
    }
}

