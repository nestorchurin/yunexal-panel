mod session;

use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use russh::server::{Auth, Msg, Server as RusshServer, Session};
use russh::{Channel, ChannelId};
use sqlx::SqlitePool;
use tokio::sync::Mutex;
use tracing::{info, warn};

use crate::{db, docker};

// ── SSH session (per connection) ──────────────────────────────────────────────

struct SshSession {
    pool: SqlitePool,
    docker: bollard::Docker,
    volume_path: Option<PathBuf>,
    channels: Arc<Mutex<HashMap<ChannelId, Channel<Msg>>>>,
}

impl SshSession {
    async fn take_channel(&mut self, id: ChannelId) -> Option<Channel<Msg>> {
        self.channels.lock().await.remove(&id)
    }
}

impl russh::server::Handler for SshSession {
    type Error = anyhow::Error;

    async fn auth_password(&mut self, user: &str, password: &str) -> Result<Auth, Self::Error> {
        let Some((server_id_str, username)) = user.split_once('.') else {
            return Ok(Auth::reject());
        };
        let Ok(server_id) = server_id_str.parse::<i64>() else {
            return Ok(Auth::reject());
        };

        let info = match db::get_sftp_auth_info(&self.pool, server_id, username).await {
            Ok(Some(i)) => i,
            Ok(None) => return Ok(Auth::reject()),
            Err(e) => {
                warn!("SFTP auth DB error: {e}");
                return Ok(Auth::reject());
            }
        };

        if !info.has_access {
            return Ok(Auth::reject());
        }
        if !crate::password::verify(password, &info.password_hash) {
            return Ok(Auth::reject());
        }

        let volume_dir = match docker::get_volume_dir(&self.docker, &info.container_id).await {
            Ok(d) => d,
            Err(e) => {
                warn!("SFTP volume_dir error: {e}");
                return Ok(Auth::reject());
            }
        };
        self.volume_path = Some(docker::volume_dir_to_path(&volume_dir));
        info!("SFTP auth ok: {username} → server {server_id}");
        Ok(Auth::Accept)
    }

    async fn channel_open_session(
        &mut self,
        channel: Channel<Msg>,
        _session: &mut Session,
    ) -> Result<bool, Self::Error> {
        self.channels.lock().await.insert(channel.id(), channel);
        Ok(true)
    }

    async fn channel_eof(&mut self, channel: ChannelId, session: &mut Session) -> Result<(), Self::Error> {
        session.close(channel)?;
        Ok(())
    }

    async fn subsystem_request(
        &mut self,
        channel_id: ChannelId,
        name: &str,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        if name != "sftp" {
            session.channel_failure(channel_id)?;
            return Ok(());
        }

        let Some(volume_path) = self.volume_path.clone() else {
            session.channel_failure(channel_id)?;
            return Ok(());
        };

        let Some(channel) = self.take_channel(channel_id).await else {
            session.channel_failure(channel_id)?;
            return Ok(());
        };

        session.channel_success(channel_id)?;
        let sftp_session = session::SftpSession::new(volume_path);
        tokio::spawn(async move {
            russh_sftp::server::run(channel.into_stream(), sftp_session).await;
        });
        Ok(())
    }
}

// ── Server factory ────────────────────────────────────────────────────────────

struct SftpServerFactory {
    pool: SqlitePool,
    docker: bollard::Docker,
}

impl RusshServer for SftpServerFactory {
    type Handler = SshSession;

    fn new_client(&mut self, _addr: Option<SocketAddr>) -> Self::Handler {
        SshSession {
            pool: self.pool.clone(),
            docker: self.docker.clone(),
            volume_path: None,
            channels: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

// ── Public entry point ────────────────────────────────────────────────────────

pub async fn start(pool: SqlitePool, docker: bollard::Docker, port: u16, host_key_pem: String) -> Result<()> {
    let private_key = russh::keys::PrivateKey::from_openssh(&host_key_pem)
        .map_err(|e| anyhow::anyhow!("SFTP host key parse error: {e}"))?;

    let config = Arc::new(russh::server::Config {
        auth_rejection_time: std::time::Duration::from_secs(1),
        auth_rejection_time_initial: Some(std::time::Duration::from_secs(0)),
        keys: vec![private_key],
        ..Default::default()
    });

    let mut factory = SftpServerFactory { pool, docker };
    info!("SFTP server listening on 0.0.0.0:{port}");
    factory.run_on_address(config, ("0.0.0.0", port)).await?;
    Ok(())
}

/// Generate a new Ed25519 host key and return it as OpenSSH PEM string.
pub fn generate_host_key() -> Result<String> {
    let key = russh::keys::PrivateKey::random(
        &mut rand::rng(),
        russh::keys::Algorithm::Ed25519,
    )
    .map_err(|e| anyhow::anyhow!("Key generation error: {e}"))?;

    let pem = key
        .to_openssh(russh::keys::ssh_key::LineEnding::LF)
        .map_err(|e| anyhow::anyhow!("Key serialization error: {e}"))?;

    Ok(pem.to_string())
}
