use bollard::Docker;
use bollard::query_parameters::ListContainersOptions;
use serde::{Deserialize, Serialize};
use anyhow::{Result, Context};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DockerImageInfo {
    pub id: String,           // short sha (12 chars)
    pub full_id: String,      // full sha256:... string
    pub repo_tags: Vec<String>,
    pub size_mb: String,
    pub created: String,
    pub in_use: bool,         // true if any yunexal container references this image
}

/// Lists all local Docker images, annotated with whether a managed container uses them.
pub async fn list_docker_images(docker: &Docker) -> Result<Vec<DockerImageInfo>> {
    use bollard::query_parameters::ListImagesOptions;

    let images_fut = docker
        .list_images(Some(ListImagesOptions { all: false, ..Default::default() }));

    let mut filters = std::collections::HashMap::new();
    filters.insert("label".to_string(), vec!["yunexal.managed=true".to_string()]);
    let containers_fut = docker.list_containers(Some(ListContainersOptions {
        all: true,
        filters: Some(filters),
        ..Default::default()
    }));

    let (summaries, containers) = tokio::try_join!(images_fut, containers_fut)
        .context("Failed to list images/containers")?;

    // Collect image IDs referenced by yunexal containers.
    let used_images: std::collections::HashSet<String> = containers
        .into_iter()
        .filter_map(|c| c.image_id)
        .collect();

    let images = summaries
        .into_iter()
        .filter_map(|s| {
            let full_id = s.id.clone();
            let short_id = if full_id.starts_with("sha256:") {
                full_id[7..].chars().take(12).collect()
            } else {
                full_id.chars().take(12).collect()
            };
            let size_mb = if s.size >= 1_073_741_824 {
                format!("{:.1} GB", s.size as f64 / 1_073_741_824.0)
            } else {
                format!("{:.0} MB", s.size as f64 / 1_048_576.0)
            };
            // Format created Unix timestamp as YYYY-MM-DD
            let created = {
                let ts = s.created as u64;
                let days = ts / 86400;
                fn civil(d: u64) -> (u64, u64, u64) {
                    let z = d + 719468;
                    let era = z / 146097;
                    let doe = z - era * 146097;
                    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
                    let y = yoe + era * 400;
                    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
                    let mp = (5 * doy + 2) / 153;
                    let d = doy - (153 * mp + 2) / 5 + 1;
                    let m = if mp < 10 { mp + 3 } else { mp - 9 };
                    let y = if m <= 2 { y + 1 } else { y };
                    (y, m, d)
                }
                let (y, m, d) = civil(days);
                format!("{:04}-{:02}-{:02}", y, m, d)
            };
            let in_use = used_images.contains(&full_id);
            let repo_tags: Vec<String> = s.repo_tags.into_iter()
                .filter(|t| t != "<none>:<none>")
                .collect();
            if repo_tags.is_empty() {
                return None; // hide untagged (<none>:<none>-only) images
            }
            Some(DockerImageInfo { id: short_id, full_id, repo_tags, size_mb, created, in_use })
        })
        .collect();

    Ok(images)
}

/// Deletes an image by full ID (sha256:...) or tag, with force=true.
pub async fn delete_docker_image(docker: &Docker, image_ref: &str) -> Result<()> {
    use bollard::query_parameters::RemoveImageOptionsBuilder;
    let opts = RemoveImageOptionsBuilder::default().force(true).build();
    docker
        .remove_image(image_ref, Some(opts), None)
        .await
        .context("Failed to remove image")?;
    Ok(())
}

/// Adds a new tag `new_tag` (format `repo:tag`) to an image identified by `image_ref`.
pub async fn retag_docker_image(docker: &Docker, image_ref: &str, new_repo: &str, new_tag: &str) -> Result<()> {
    use bollard::query_parameters::TagImageOptionsBuilder;
    let opts = TagImageOptionsBuilder::default()
        .repo(new_repo)
        .tag(new_tag)
        .build();
    docker
        .tag_image(image_ref, Some(opts))
        .await
        .context("Failed to tag image")?;
    Ok(())
}

/// Creates a full independent copy of an image via `docker commit` of a temp container.
/// Returns the new image's full ID (sha256:...).
pub async fn duplicate_docker_image(docker: &Docker, image_ref: &str) -> Result<String> {
    use bollard::query_parameters::{CreateContainerOptions, RemoveContainerOptions};

    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let temp_name = format!("yunexal-dup-tmp-{}", ts);

    // 1. Create a stopped temporary container from the source image
    let create_opts = CreateContainerOptions {
        name: Some(temp_name.clone()),
        platform: String::new(),
    };
    let create_body = bollard::models::ContainerCreateBody {
        image: Some(image_ref.to_string()),
        ..Default::default()
    };
    let container = docker
        .create_container(Some(create_opts), create_body)
        .await
        .context("Failed to create temp container for image duplication")?;
    let cid = container.id.clone();

    // 2. Commit via docker CLI (most reliable, matches CLI behaviour exactly)
    let commit_out = tokio::process::Command::new("docker")
        .args(["commit", &cid])
        .output()
        .await
        .context("Failed to spawn docker commit")?;

    // 3. Always clean up the temp container
    let _ = docker
        .remove_container(&cid, Some(RemoveContainerOptions { force: true, ..Default::default() }))
        .await;

    if !commit_out.status.success() {
        let err = String::from_utf8_lossy(&commit_out.stderr).trim().to_string();
        return Err(anyhow::anyhow!("docker commit failed: {}", err));
    }

    // stdout is "sha256:<hex>\n"
    let new_id = String::from_utf8_lossy(&commit_out.stdout).trim().to_string();
    if new_id.is_empty() {
        return Err(anyhow::anyhow!("docker commit returned empty ID"));
    }
    Ok(new_id)
}

pub async fn ensure_image(docker: &Docker, image: &str) -> Result<()> {
    use bollard::errors::Error as BollardError;
    use bollard::query_parameters::CreateImageOptions;
    use futures_util::stream::StreamExt;

    let options = CreateImageOptions {
        from_image: Some(image.to_string()),
        ..Default::default()
    };

    let mut stream = docker.create_image(Some(options), None, None);
    while let Some(msg) = stream.next().await {
        if let Err(err) = msg {
            return match err {
                BollardError::DockerResponseServerError { status_code, message } => {
                    let reason = if status_code == 404 {
                        "image not found or private; check the name/tag or login first"
                    } else {
                        "docker registry returned an error"
                    };
                    Err(anyhow::anyhow!(
                        "Failed to pull image '{}': {} ({})",
                        image,
                        message,
                        reason
                    ))
                }
                other => Err(anyhow::anyhow!("Failed to pull image '{}': {}", image, other)),
            };
        }
    }
    Ok(())
}

pub async fn get_image_info(docker: &Docker, image: &str) -> Result<bollard::models::ImageInspect> {
    docker.inspect_image(image).await.map_err(|e| anyhow::anyhow!("Failed to inspect image: {}", e))
}
