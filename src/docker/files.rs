use bollard::Docker;
use anyhow::{Result, Context};

// ── File listing ─────────────────────────────────────────────────────────────

pub async fn list_files(_docker: &Docker, id: &str, path: &str) -> Result<Vec<String>> {
    let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    // 'id' is expected to be the container name (server_id)
    let volume_path = cwd.join("volumes").join(id);
    
    // path is relative to the mount point (/app/data), so it should be relative to volume_path
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
    let mut files = Vec::new();
    
    while let Some(entry) = entries.next_entry().await? {
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.') || name.ends_with(".example") || name.ends_with(".test") { continue; }
        if entry.file_type().await?.is_dir() {
            files.push(format!("{}/", name));
        } else {
            files.push(name);
        }
    }
    
    // Sort files: directories first, then alphabetical
    files.sort_by(|a, b| {
        let a_is_dir = a.ends_with('/');
        let b_is_dir = b.ends_with('/');
        if a_is_dir && !b_is_dir {
            std::cmp::Ordering::Less
        } else if !a_is_dir && b_is_dir {
            std::cmp::Ordering::Greater
        } else {
            a.cmp(b)
        }
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

// ── Volume directory resolution ──────────────────────────────────────────────

/// Returns the volume directory key for this container.
/// Resolution order:
///   1. Label `yunexal.volume_dir` — if the directory actually exists on disk.
///   2. Full 64-char container ID — if `./volumes/<full_id>` exists on disk.
///   3. Label value or container name as a last-resort string (directory may be missing).
pub async fn get_volume_dir(docker: &Docker, id: &str) -> Result<String> {
    let c = docker.inspect_container(id, None).await
        .context("Container not found")?;

    let full_id = c.id.clone().unwrap_or_default();
    let name = c.name.clone().unwrap_or_default().trim_start_matches('/').to_string();

    let label_key = c.config.as_ref()
        .and_then(|cfg| cfg.labels.as_ref())
        .and_then(|labels| labels.get("yunexal.volume_dir").cloned());

    // Extract volume dir from the actual bind mount source path
    let bind_dir = c.host_config.as_ref()
        .and_then(|hc| hc.binds.as_ref())
        .and_then(|binds| binds.first())
        .and_then(|b| b.split(':').next())
        .and_then(|path| std::path::Path::new(path).file_name())
        .and_then(|f| f.to_str())
        .map(|s| s.to_string());

    let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));

    // 1. Label path exists on disk
    if let Some(ref key) = label_key {
        if cwd.join("volumes").join(key).exists() {
            return Ok(key.clone());
        }
    }

    // 2. Bind mount source directory exists on disk
    if let Some(ref dir) = bind_dir {
        if cwd.join("volumes").join(dir).exists() {
            return Ok(dir.clone());
        }
    }

    // 3. Full container ID directory exists on disk
    if !full_id.is_empty() && cwd.join("volumes").join(&full_id).exists() {
        return Ok(full_id);
    }

    // 4. Fallback — return bind dir, label, or name even if missing
    Ok(bind_dir.or(label_key).unwrap_or(name))
}
