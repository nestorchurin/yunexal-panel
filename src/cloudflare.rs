// Cloudflare API helpers — Under Attack Mode management

use anyhow::{anyhow, Result};

const CF_BASE: &str = "https://api.cloudflare.com/client/v4";

fn http() -> reqwest::Client {
    reqwest::Client::builder()
        .user_agent("yunexal-panel")
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .expect("CF http client")
}

/// Returns the current Cloudflare security level, e.g. "medium", "high", "under_attack".
pub async fn get_security_level(zone_id: &str, token: &str) -> Result<String> {
    let url = format!("{CF_BASE}/zones/{zone_id}/settings/security_level");
    let resp = http()
        .get(&url)
        .header("Authorization", format!("Bearer {token}"))
        .send().await?
        .json::<serde_json::Value>().await?;
    if !resp["success"].as_bool().unwrap_or(false) {
        return Err(anyhow!("CF API error: {}", resp["errors"]));
    }
    Ok(resp["result"]["value"].as_str().unwrap_or("unknown").to_string())
}

/// Sets the Cloudflare security level (e.g. "under_attack", "high", "medium", "low", "essentially_off").
pub async fn set_security_level(zone_id: &str, token: &str, level: &str) -> Result<()> {
    let url = format!("{CF_BASE}/zones/{zone_id}/settings/security_level");
    let resp = http()
        .patch(&url)
        .header("Authorization", format!("Bearer {token}"))
        .header("Content-Type", "application/json")
        .json(&serde_json::json!({"value": level}))
        .send().await?
        .json::<serde_json::Value>().await?;
    if !resp["success"].as_bool().unwrap_or(false) {
        return Err(anyhow!("CF API error: {}", resp["errors"]));
    }
    Ok(())
}

/// Enable Under Attack Mode.
pub async fn enable_under_attack(zone_id: &str, token: &str) -> Result<()> {
    set_security_level(zone_id, token, "under_attack").await
}

/// Restore normal security level ("medium").
pub async fn disable_under_attack(zone_id: &str, token: &str) -> Result<()> {
    set_security_level(zone_id, token, "medium").await
}
