use bollard::Docker;
use anyhow::{Result, Context};

// ── File listing ─────────────────────────────────────────────────────────────

/// File entry: (name_with_trailing_slash_for_dirs, size_in_bytes_for_files)
pub type FileEntry = (String, Option<u64>);

pub async fn list_files(_docker: &Docker, id: &str, path: &str) -> Result<Vec<FileEntry>> {
    let volume_path = volume_dir_to_path(id);

    let rel_path = path.trim_start_matches('/');
    let target_joined = volume_path.join(rel_path);
    // Normalize ".." to prevent path traversal (defense in depth)
    let mut target_path = std::path::PathBuf::new();
    for component in target_joined.components() {
        match component {
            std::path::Component::ParentDir => { target_path.pop(); },
            std::path::Component::CurDir    => {},
            c => target_path.push(c),
        }
    }
    if !target_path.starts_with(&volume_path) {
        anyhow::bail!("Access denied: path traversal");
    }

    if !target_path.exists() {
        return Ok(vec![]);
    }

    let mut entries = tokio::fs::read_dir(target_path).await
        .context(format!("Failed to read directory {:?}", rel_path))?;
    let mut files: Vec<FileEntry> = Vec::new();

    while let Some(entry) = entries.next_entry().await? {
        let name = entry.file_name().to_string_lossy().to_string();
        if name.ends_with(".example") || name.ends_with(".test") { continue; }
        if entry.file_type().await?.is_dir() {
            files.push((format!("{}/", name), None));
        } else {
            let size = entry.metadata().await.ok().map(|m| m.len());
            files.push((name, size));
        }
    }

    // Sort: directories first, then alphabetical
    files.sort_by(|(a, _), (b, _)| {
        let a_is_dir = a.ends_with('/');
        let b_is_dir = b.ends_with('/');
        if a_is_dir && !b_is_dir      { std::cmp::Ordering::Less }
        else if !a_is_dir && b_is_dir { std::cmp::Ordering::Greater }
        else                          { a.cmp(b) }
    });

    Ok(files)
}

// ── Copy image files to volume ───────────────────────────────────────────────

/// Copies files from a container path into the host `dest` directory using `docker cp`.
/// The container does NOT need to be running — works on created (stopped) containers too.
/// Silently succeeds if the path doesn't exist in the image.
pub async fn copy_image_files_to_volume(container_id: &str, src_path: &str, dest: &std::path::Path) -> Result<()> {
    let src = format!("{}:{}/.", container_id, src_path.trim_end_matches('/'));
    let dest_str = dest.to_string_lossy().to_string();

    let output = tokio::process::Command::new("docker")
        .args(["cp", &src, &dest_str])
        .output()
        .await
        .context("Failed to spawn docker cp")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).to_lowercase();
        if stderr.contains("no such") || stderr.contains("not found") || stderr.contains("could not find") {
            return Ok(());
        }
        tracing::warn!("docker cp: {}", String::from_utf8_lossy(&output.stderr));
    }
    Ok(())
}

// ── Volume path helper ─────────────────────────────────────────────────────

/// Converts a `volume_dir` (as returned by `get_volume_dir`) to an absolute
/// `PathBuf`. If `volume_dir` is already absolute, returns it as-is.
/// Otherwise, resolves relative to `<cwd>/volumes/`.
pub fn volume_dir_to_path(volume_dir: &str) -> std::path::PathBuf {
    let p = std::path::Path::new(volume_dir);
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
        cwd.join("volumes").join(volume_dir)
    }
}

/// Returns the backup directory for a container volume.
/// Stored adjacent to the volume, never inside it:
///   /data/servers/abc123  →  /data/servers/abc123_backups
pub fn backup_dir_for_volume(volume_path: &std::path::Path) -> std::path::PathBuf {
    let mut s = volume_path.to_string_lossy().into_owned();
    s.push_str("_backups");
    std::path::PathBuf::from(s)
}

// ── Volume directory resolution ──────────────────────────────────────────────

/// Returns the volume path for this container as a String.
/// - If the bind mount source exists on disk, returns the full absolute path.
/// - If the `yunexal.volume_dir` label key exists under `cwd/volumes/`, returns the key.
/// - Falls back to label / container name.
pub async fn get_volume_dir(docker: &Docker, id: &str) -> Result<String> {
    let c = docker.inspect_container(id, None).await
        .context("Container not found")?;

    let full_id = c.id.clone().unwrap_or_default();
    let name = c.name.clone().unwrap_or_default().trim_start_matches('/').to_string();

    let label_key = c.config.as_ref()
        .and_then(|cfg| cfg.labels.as_ref())
        .and_then(|labels| labels.get("yunexal.volume_dir").cloned());

    // Full bind-mount source path (e.g. "/var/lib/docker/yunexal-volumes/abc123")
    let bind_source = c.host_config.as_ref()
        .and_then(|hc| hc.binds.as_ref())
        .and_then(|binds| binds.first())
        .and_then(|b| b.split(':').next())
        .map(|s| s.to_string());

    let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));

    // 1. Bind mount source is absolute and exists — return the full path directly.
    if let Some(ref src) = bind_source {
        let src_path = std::path::Path::new(src);
        if src_path.is_absolute() && src_path.exists() {
            return Ok(src.clone());
        }
    }

    // 2. Label key exists under cwd/volumes
    if let Some(ref key) = label_key {
        if cwd.join("volumes").join(key).exists() {
            return Ok(key.clone());
        }
    }

    // 3. Bind source filename exists under cwd/volumes (legacy containers)
    if let Some(ref src) = bind_source {
        let dir_name = std::path::Path::new(src)
            .file_name().and_then(|f| f.to_str()).map(|s| s.to_string());
        if let Some(ref dir) = dir_name {
            if cwd.join("volumes").join(dir).exists() {
                return Ok(dir.clone());
            }
        }
    }

    // 4. Full container ID under cwd/volumes
    if !full_id.is_empty() && cwd.join("volumes").join(&full_id).exists() {
        return Ok(full_id);
    }

    // 5. Fallback — return full bind source (possibly non-existent) or label or name
    Ok(bind_source.or(label_key).unwrap_or(name))
}
