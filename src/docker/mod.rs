mod containers;
mod stats;
mod images;
mod files;
mod network;
mod edit;

pub use containers::*;
pub use stats::*;
pub use images::*;
pub use files::*;
pub use network::*;
pub use edit::*;

use bollard::Docker;
use serde::{Deserialize, Serialize};
use anyhow::{Result, Context};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ContainerInfo {
    pub id: String,
    pub short_id: String,
    pub name: String,
    pub status: String,
    pub state: String,
    pub cpu_usage: String,
    pub ram_usage: String,
    /// Internal SQLite id. 0 if not yet resolved from DB.
    pub db_id: i64,
    /// Owner username. Empty string if not yet resolved from DB.
    pub owner: String,
}

pub async fn get_docker_client() -> Result<Docker> {
    Docker::connect_with_socket_defaults().context("Failed to connect to Docker socket")
}
