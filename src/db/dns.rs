use anyhow::Result;
use sqlx::{FromRow, Pool, Sqlite};

#[derive(Debug, Clone, FromRow, serde::Serialize)]
pub struct DnsProvider {
    pub id:            i64,
    pub name:          String,
    pub provider_type: String,
    pub credentials:   String, // JSON string — never expose raw to frontend
    pub enabled:       i64,
    pub created_at:    String,
}

#[derive(Debug, Clone, FromRow, serde::Serialize)]
pub struct DnsRecord {
    pub id:            i64,
    pub provider_id:   i64,
    pub zone_id:       String,
    pub zone_name:     String,
    pub record_type:   String,
    pub name:          String,
    pub value:         String,
    pub ttl:           i64,
    pub priority:      i64,
    pub proxied:       i64,
    pub remote_id:     String,
    pub container_id:  Option<i64>,
    pub ddns_enabled:  i64,
    pub ddns_interval: i64,
    pub last_ip:       String,
    pub last_synced:   String,
    pub created_at:    String,
}

pub async fn dns_list_providers(pool: &Pool<Sqlite>) -> Result<Vec<DnsProvider>> {
    let rows = sqlx::query_as::<_, DnsProvider>(
        "SELECT * FROM dns_providers ORDER BY id",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

pub async fn dns_get_provider(pool: &Pool<Sqlite>, id: i64) -> Result<Option<DnsProvider>> {
    let row = sqlx::query_as::<_, DnsProvider>(
        "SELECT * FROM dns_providers WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

pub async fn dns_add_provider(
    pool: &Pool<Sqlite>,
    name: &str,
    provider_type: &str,
    credentials: &str,
) -> Result<i64> {
    let id = sqlx::query(
        "INSERT INTO dns_providers (name, provider_type, credentials) VALUES (?, ?, ?)",
    )
    .bind(name)
    .bind(provider_type)
    .bind(credentials)
    .execute(pool)
    .await?
    .last_insert_rowid();
    Ok(id)
}

pub async fn dns_update_provider(
    pool: &Pool<Sqlite>,
    id: i64,
    name: &str,
    credentials: &str,
    enabled: i64,
) -> Result<()> {
    sqlx::query(
        "UPDATE dns_providers SET name=?, credentials=?, enabled=? WHERE id=?",
    )
    .bind(name)
    .bind(credentials)
    .bind(enabled)
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn dns_delete_provider(pool: &Pool<Sqlite>, id: i64) -> Result<()> {
    sqlx::query("DELETE FROM dns_providers WHERE id=?").bind(id).execute(pool).await?;
    Ok(())
}

pub async fn dns_list_records(pool: &Pool<Sqlite>, provider_id: i64) -> Result<Vec<DnsRecord>> {
    let rows = sqlx::query_as::<_, DnsRecord>(
        "SELECT * FROM dns_records WHERE provider_id=? ORDER BY zone_name, name",
    )
    .bind(provider_id)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

pub async fn dns_list_ddns_records(pool: &Pool<Sqlite>) -> Result<Vec<DnsRecord>> {
    sqlx::query_as::<_, DnsRecord>(
        "SELECT * FROM dns_records WHERE ddns_enabled=1 ORDER BY id",
    )
    .fetch_all(pool)
    .await
    .map_err(Into::into)
}

pub async fn dns_add_record(
    pool: &Pool<Sqlite>,
    provider_id: i64,
    zone_id: &str,
    zone_name: &str,
    record_type: &str,
    name: &str,
    value: &str,
    ttl: i64,
    priority: i64,
    proxied: bool,
    remote_id: &str,
    container_id: Option<i64>,
    ddns_enabled: bool,
    ddns_interval: i64,
) -> Result<i64> {
    let id = sqlx::query(
        r#"INSERT INTO dns_records
           (provider_id,zone_id,zone_name,record_type,name,value,ttl,priority,proxied,remote_id,container_id,ddns_enabled,ddns_interval)
           VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?)"#,
    )
    .bind(provider_id).bind(zone_id).bind(zone_name)
    .bind(record_type).bind(name).bind(value)
    .bind(ttl).bind(priority).bind(proxied as i64)
    .bind(remote_id).bind(container_id)
    .bind(ddns_enabled as i64).bind(ddns_interval)
    .execute(pool)
    .await?
    .last_insert_rowid();
    Ok(id)
}

pub async fn dns_update_record(
    pool: &Pool<Sqlite>,
    id: i64,
    name: &str,
    value: &str,
    ttl: i64,
    priority: i64,
    proxied: bool,
    ddns_enabled: bool,
    ddns_interval: i64,
) -> Result<()> {
    sqlx::query(
        "UPDATE dns_records SET name=?,value=?,ttl=?,priority=?,proxied=?,ddns_enabled=?,ddns_interval=? WHERE id=?",
    )
    .bind(name).bind(value).bind(ttl).bind(priority)
    .bind(proxied as i64).bind(ddns_enabled as i64).bind(ddns_interval)
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn dns_update_record_ip(pool: &Pool<Sqlite>, id: i64, ip: &str) -> Result<()> {
    sqlx::query(
        "UPDATE dns_records SET last_ip=?, last_synced=datetime('now') WHERE id=?",
    )
    .bind(ip).bind(id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn dns_delete_record(pool: &Pool<Sqlite>, id: i64) -> Result<()> {
    sqlx::query("DELETE FROM dns_records WHERE id=?").bind(id).execute(pool).await?;
    Ok(())
}

/// Returns all DNS records linked to a specific server (by SQLite server id).
pub async fn dns_list_records_by_server_id(pool: &Pool<Sqlite>, server_id: i64) -> Result<Vec<DnsRecord>> {
    let rows = sqlx::query_as::<_, DnsRecord>(
        "SELECT * FROM dns_records WHERE container_id = ?",
    )
    .bind(server_id)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// Deletes all DNS records in the panel DB linked to a specific server.
pub async fn dns_delete_records_by_server_id(pool: &Pool<Sqlite>, server_id: i64) -> Result<()> {
    sqlx::query("DELETE FROM dns_records WHERE container_id = ?")
        .bind(server_id).execute(pool).await?;
    Ok(())
}

/// Returns all DNS records that are linked to a container (container_id IS NOT NULL),
/// ordered by server name then record name.
pub async fn dns_list_all_container_records(pool: &Pool<Sqlite>) -> Result<Vec<DnsRecord>> {
    sqlx::query_as::<_, DnsRecord>(
        "SELECT * FROM dns_records WHERE container_id IS NOT NULL ORDER BY container_id, name",
    )
    .fetch_all(pool)
    .await
    .map_err(Into::into)
}
