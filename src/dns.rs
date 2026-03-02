// DNS provider implementations — Cloudflare, DuckDNS, GoDaddy, Namecheap, Generic

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;

// ── Shared types ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DnsZone {
    pub id:   String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteDnsRecord {
    pub id:          String,
    pub zone_id:     String,
    pub zone_name:   String,
    pub name:        String,
    pub record_type: String,
    pub value:       String,
    pub ttl:         i64,
    pub priority:    i64,
    pub proxied:     bool,
    /// Set to "yunexal.managed=true" by this panel; used to identify panel-managed records.
    pub comment:     Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DnsRecordInput {
    pub record_type: String,
    pub name:        String,
    pub value:       String,
    pub ttl:         i64,
    pub priority:    i64,
    pub proxied:     bool,
}

// ── Provider credentials shapes (parsed from JSON stored in DB) ───────────────

/// Construct per-provider credential fields from a provider_type string + JSON credentials.
pub enum DnsClient {
    Cloudflare { token: String },
    DuckDns    { token: String, domain: String },
    GoDaddy    { api_key: String, api_secret: String },
    Namecheap  { api_key: String, #[allow(dead_code)] api_user: String, #[allow(dead_code)] username: String },
    Generic    { update_url: String, method: String },
}

impl DnsClient {
    pub fn from_type(provider_type: &str, creds: &Value) -> Result<Self> {
        let s = |key: &str| creds[key].as_str().unwrap_or("").to_string();
        match provider_type {
            "cloudflare" => Ok(Self::Cloudflare { token: s("api_token") }),
            "duckdns"    => Ok(Self::DuckDns   { token: s("token"), domain: s("domain") }),
            "godaddy"    => Ok(Self::GoDaddy   { api_key: s("api_key"), api_secret: s("api_secret") }),
            "namecheap"  => Ok(Self::Namecheap { api_key: s("api_key"), api_user: s("api_user"), username: s("username") }),
            "generic"    => Ok(Self::Generic   { update_url: s("update_url"), method: s("method").to_uppercase() }),
            other        => Err(anyhow!("Unknown provider type: {}", other)),
        }
    }

    fn http() -> reqwest::Client {
        reqwest::Client::builder()
            .user_agent("yunexal-panel/0.2")
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("HTTP client")
    }

    // ── List zones ────────────────────────────────────────────────────────────

    pub async fn list_zones(&self) -> Result<Vec<DnsZone>> {
        match self {
            Self::Cloudflare { token } => {
                let resp = Self::http()
                    .get("https://api.cloudflare.com/client/v4/zones?per_page=200")
                    .header("Authorization", format!("Bearer {}", token))
                    .send().await?.json::<Value>().await?;
                if !resp["success"].as_bool().unwrap_or(false) {
                    return Err(anyhow!("Cloudflare: {}", resp["errors"]));
                }
                Ok(resp["result"].as_array().map(|arr| arr.iter().filter_map(|z| {
                    Some(DnsZone { id: z["id"].as_str()?.to_string(), name: z["name"].as_str()?.to_string() })
                }).collect()).unwrap_or_default())
            }
            Self::GoDaddy { api_key, api_secret } => {
                let resp = Self::http()
                    .get("https://api.godaddy.com/v1/domains?limit=200&statuses=ACTIVE")
                    .header("Authorization", format!("sso-key {}:{}", api_key, api_secret))
                    .send().await?.json::<Value>().await?;
                Ok(resp.as_array().map(|arr| arr.iter().filter_map(|d| {
                    let name = d["domain"].as_str()?.to_string();
                    Some(DnsZone { id: name.clone(), name })
                }).collect()).unwrap_or_default())
            }
            Self::DuckDns { token: _, domain: _ } => {
                // DuckDNS has no zone list; domain(s) come from credentials
                Ok(vec![DnsZone { id: "duckdns".to_string(), name: "duckdns.org".to_string() }])
            }
            Self::Namecheap { .. } => {
                Ok(vec![DnsZone { id: "namecheap".to_string(), name: "Namecheap DDNS".to_string() }])
            }
            Self::Generic { .. } => {
                Ok(vec![DnsZone { id: "generic".to_string(), name: "Generic Webhook".to_string() }])
            }
        }
    }

    // ── List records ──────────────────────────────────────────────────────────

    pub async fn list_records(&self, zone_id: &str) -> Result<Vec<RemoteDnsRecord>> {
        match self {
            Self::Cloudflare { token } => {
                let resp = Self::http()
                    .get(format!(
                        "https://api.cloudflare.com/client/v4/zones/{}/dns_records?per_page=1000",
                        zone_id
                    ))
                    .header("Authorization", format!("Bearer {}", token))
                    .send().await?.json::<Value>().await?;
                if !resp["success"].as_bool().unwrap_or(false) {
                    return Err(anyhow!("Cloudflare: {}", resp["errors"]));
                }
                Ok(resp["result"].as_array().map(|arr| arr.iter().filter_map(|r| {
                    Some(RemoteDnsRecord {
                        id:          r["id"].as_str()?.to_string(),
                        zone_id:     zone_id.to_string(),
                        zone_name:   r["zone_name"].as_str().unwrap_or(zone_id).to_string(),
                        name:        r["name"].as_str()?.to_string(),
                        record_type: r["type"].as_str()?.to_string(),
                        value:       r["content"].as_str()?.to_string(),
                        ttl:         r["ttl"].as_i64().unwrap_or(1),
                        priority:    r["priority"].as_i64().unwrap_or(0),
                        proxied:     r["proxied"].as_bool().unwrap_or(false),
                        comment:     r["comment"].as_str().map(str::to_string),
                    })
                }).collect()).unwrap_or_default())
            }
            Self::GoDaddy { api_key, api_secret } => {
                let resp = Self::http()
                    .get(format!("https://api.godaddy.com/v1/domains/{}/records", zone_id))
                    .header("Authorization", format!("sso-key {}:{}", api_key, api_secret))
                    .send().await?.json::<Value>().await?;
                Ok(resp.as_array().map(|arr| arr.iter().filter_map(|r| {
                    let rtype = r["type"].as_str()?;
                    let rname = r["name"].as_str()?;
                    Some(RemoteDnsRecord {
                        id:          format!("{}/{}", rtype, rname),
                        zone_id:     zone_id.to_string(),
                        zone_name:   zone_id.to_string(),
                        name:        rname.to_string(),
                        record_type: rtype.to_string(),
                        value:       r["data"].as_str()?.to_string(),
                        ttl:         r["ttl"].as_i64().unwrap_or(600),
                        priority:    r["priority"].as_i64().unwrap_or(0),
                        proxied:     false,
                        comment:     None,
                    })
                }).collect()).unwrap_or_default())
            }
            _ => Ok(vec![]), // DuckDNS, Namecheap, Generic: no record listing
        }
    }

    // ── Create record ─────────────────────────────────────────────────────────

    pub async fn create_record(&self, zone_id: &str, rec: &DnsRecordInput) -> Result<String> {
        match self {
            Self::Cloudflare { token } => {
                let body = serde_json::json!({
                    "type":    rec.record_type,
                    "name":    rec.name,
                    "content": rec.value,
                    "ttl":     rec.ttl,
                    "proxied": rec.proxied,
                    "comment": "yunexal.managed=true",
                });
                let resp = Self::http()
                    .post(format!(
                        "https://api.cloudflare.com/client/v4/zones/{}/dns_records",
                        zone_id
                    ))
                    .header("Authorization", format!("Bearer {}", token))
                    .json(&body).send().await?.json::<Value>().await?;
                if resp["success"].as_bool().unwrap_or(false) {
                    Ok(resp["result"]["id"].as_str().unwrap_or("").to_string())
                } else {
                    Err(anyhow!("Cloudflare create failed: {}", resp["errors"]))
                }
            }
            Self::GoDaddy { api_key, api_secret } => {
                let mut entry = serde_json::json!({ "data": rec.value, "ttl": rec.ttl });
                if rec.priority > 0 { entry["priority"] = rec.priority.into(); }
                let body = serde_json::json!([entry]);
                let status = Self::http()
                    .put(format!(
                        "https://api.godaddy.com/v1/domains/{}/records/{}/{}",
                        zone_id, rec.record_type, rec.name
                    ))
                    .header("Authorization", format!("sso-key {}:{}", api_key, api_secret))
                    .json(&body).send().await?.status();
                if status.is_success() {
                    Ok(format!("{}/{}", rec.record_type, rec.name))
                } else {
                    Err(anyhow!("GoDaddy create failed: {}", status))
                }
            }
            _ => Err(anyhow!("Record creation not supported for this provider type")),
        }
    }

    // ── Update record ─────────────────────────────────────────────────────────

    pub async fn update_record(
        &self,
        zone_id: &str,
        remote_id: &str,
        rec: &DnsRecordInput,
    ) -> Result<()> {
        match self {
            Self::Cloudflare { token } => {
                let body = serde_json::json!({
                    "type":    rec.record_type,
                    "name":    rec.name,
                    "content": rec.value,
                    "ttl":     rec.ttl,
                    "proxied": rec.proxied,
                    "comment": "yunexal.managed=true",
                });
                let resp = Self::http()
                    .patch(format!(
                        "https://api.cloudflare.com/client/v4/zones/{}/dns_records/{}",
                        zone_id, remote_id
                    ))
                    .header("Authorization", format!("Bearer {}", token))
                    .json(&body).send().await?.json::<Value>().await?;
                if resp["success"].as_bool().unwrap_or(false) { Ok(()) }
                else { Err(anyhow!("Cloudflare update failed: {}", resp["errors"])) }
            }
            Self::GoDaddy { api_key, api_secret } => {
                let body = serde_json::json!([{ "data": rec.value, "ttl": rec.ttl }]);
                let status = Self::http()
                    .put(format!(
                        "https://api.godaddy.com/v1/domains/{}/records/{}/{}",
                        zone_id, rec.record_type, rec.name
                    ))
                    .header("Authorization", format!("sso-key {}:{}", api_key, api_secret))
                    .json(&body).send().await?.status();
                if status.is_success() { Ok(()) } else { Err(anyhow!("GoDaddy update failed: {}", status)) }
            }
            _ => Err(anyhow!("Record update not supported for this provider type")),
        }
    }

    // ── Delete record ─────────────────────────────────────────────────────────

    pub async fn delete_record(&self, zone_id: &str, remote_id: &str) -> Result<()> {
        match self {
            Self::Cloudflare { token } => {
                let status = Self::http()
                    .delete(format!(
                        "https://api.cloudflare.com/client/v4/zones/{}/dns_records/{}",
                        zone_id, remote_id
                    ))
                    .header("Authorization", format!("Bearer {}", token))
                    .send().await?.status();
                if status.is_success() { Ok(()) } else { Err(anyhow!("Cloudflare delete failed: {}", status)) }
            }
            Self::GoDaddy { api_key, api_secret } => {
                // GoDaddy remote_id is "TYPE/name"
                let parts: Vec<&str> = remote_id.splitn(2, '/').collect();
                if parts.len() == 2 {
                    let status = Self::http()
                        .delete(format!(
                            "https://api.godaddy.com/v1/domains/{}/records/{}/{}",
                            zone_id, parts[0], parts[1]
                        ))
                        .header("Authorization", format!("sso-key {}:{}", api_key, api_secret))
                        .send().await?.status();
                    if status.is_success() { return Ok(()); }
                }
                Err(anyhow!("GoDaddy delete failed"))
            }
            _ => Err(anyhow!("Record deletion not supported for this provider type")),
        }
    }

    // ── DDNS: update A record with a new IP ───────────────────────────────────

    pub async fn update_ddns(&self, zone_id: &str, name: &str, ip: &str) -> Result<()> {
        match self {
            Self::Cloudflare { .. } => {
                let records = self.list_records(zone_id).await?;
                let matched: Vec<_> = records.iter()
                    .filter(|r| r.record_type == "A" && r.name == name)
                    .collect();
                if matched.is_empty() {
                    // Create new A record with TTL=1 (auto)
                    self.create_record(zone_id, &DnsRecordInput {
                        record_type: "A".to_string(),
                        name: name.to_string(),
                        value: ip.to_string(),
                        ttl: 1,
                        priority: 0,
                        proxied: false,
                    }).await?;
                } else {
                    for r in matched {
                        if r.value != ip {
                            self.update_record(zone_id, &r.id, &DnsRecordInput {
                                record_type: "A".to_string(),
                                name: r.name.clone(),
                                value: ip.to_string(),
                                ttl: r.ttl,
                                priority: 0,
                                proxied: r.proxied,
                            }).await?;
                        }
                    }
                }
                Ok(())
            }
            Self::DuckDns { token, .. } => {
                // Strip .duckdns.org suffix if the name was stored as FQDN
                let subdomain = name
                    .trim_end_matches('.')
                    .trim_end_matches(".duckdns.org");
                let url = format!(
                    "https://www.duckdns.org/update?domains={}&token={}&ip={}&verbose=true",
                    subdomain, token, ip
                );
                let text = Self::http().get(&url).send().await?.text().await?;
                if text.trim_start().starts_with("OK") { Ok(()) }
                else { Err(anyhow!("DuckDNS update failed: {}", text.lines().next().unwrap_or("KO"))) }
            }
            Self::GoDaddy { api_key, api_secret } => {
                let body = serde_json::json!([{ "data": ip, "ttl": 600 }]);
                let status = Self::http()
                    .put(format!(
                        "https://api.godaddy.com/v1/domains/{}/records/A/{}",
                        zone_id, name
                    ))
                    .header("Authorization", format!("sso-key {}:{}", api_key, api_secret))
                    .json(&body).send().await?.status();
                if status.is_success() { Ok(()) } else { Err(anyhow!("GoDaddy DDNS failed: {}", status)) }
            }
            Self::Namecheap { api_key, .. } => {
                // Namecheap Dynamic DNS — password is the Dynamic DNS password from the dashboard
                let url = format!(
                    "https://dynamicdns.park-your-domain.com/update?host={}&domain={}&password={}&ip={}",
                    name, zone_id, api_key, ip
                );
                Self::http().get(&url).send().await?;
                Ok(())
            }
            Self::Generic { update_url, method } => {
                let url = update_url
                    .replace("{ip}", ip)
                    .replace("{domain}", zone_id)
                    .replace("{host}", name)
                    .replace("{name}", name);
                if method == "POST" {
                    Self::http().post(&url).send().await?;
                } else {
                    Self::http().get(&url).send().await?;
                }
                Ok(())
            }
        }
    }

    // ── Test connectivity ─────────────────────────────────────────────────────

    pub async fn test(&self) -> Result<String> {
        match self {
            Self::Cloudflare { token } => {
                let resp = Self::http()
                    .get("https://api.cloudflare.com/client/v4/user/tokens/verify")
                    .header("Authorization", format!("Bearer {}", token))
                    .send().await?.json::<Value>().await?;
                if resp["success"].as_bool().unwrap_or(false) {
                    Ok(format!("Token valid — status: {}", resp["result"]["status"].as_str().unwrap_or("active")))
                } else {
                    Err(anyhow!("Cloudflare token invalid: {}", resp["errors"]))
                }
            }
            Self::DuckDns { token, domain } => {
                // Use the stored test domain; fall back to a no-op probe if blank
                let subdomain = if domain.is_empty() {
                    return Ok("DuckDNS token saved — add a 'domain' credential to verify it".to_string());
                } else {
                    domain.trim_end_matches('.').trim_end_matches(".duckdns.org")
                };
                let text = Self::http()
                    .get(format!(
                        "https://www.duckdns.org/update?domains={}&token={}&ip=&verbose=true",
                        subdomain, token
                    ))
                    .send().await?.text().await?;
                if text.trim_start().starts_with("OK") {
                    let status = text.lines().nth(3).unwrap_or("NOCHANGE");
                    Ok(format!("DuckDNS token valid — {}", status))
                } else {
                    Err(anyhow!("DuckDNS token invalid or domain not owned (response: {})", text.lines().next().unwrap_or("KO")))
                }
            }
            Self::GoDaddy { api_key, api_secret } => {
                let status = Self::http()
                    .get("https://api.godaddy.com/v1/domains?limit=1")
                    .header("Authorization", format!("sso-key {}:{}", api_key, api_secret))
                    .send().await?.status();
                if status.is_success() {
                    Ok("GoDaddy credentials valid".to_string())
                } else {
                    Err(anyhow!("GoDaddy authentication failed: {}", status))
                }
            }
            Self::Namecheap { .. } => {
                Ok("Namecheap credentials saved (verification requires a live domain)".to_string())
            }
            Self::Generic { update_url, .. } => {
                if update_url.starts_with("http") {
                    Ok(format!("Generic webhook URL set: {}", update_url))
                } else {
                    Err(anyhow!("Invalid webhook URL — must start with http(s)://"))
                }
            }
        }
    }
}

// ── Public IP helper ──────────────────────────────────────────────────────────

pub async fn get_public_ip() -> Result<String> {
    let services = [
        "https://api.ipify.org",
        "https://api4.my-ip.io/v2/ip.txt",
        "https://checkip.amazonaws.com",
    ];
    for url in services {
        if let Ok(resp) = reqwest::get(url).await {
            if let Ok(text) = resp.text().await {
                let ip = text.trim().to_string();
                if !ip.is_empty() && ip.len() < 50 { return Ok(ip); }
            }
        }
    }
    Err(anyhow!("Could not determine public IP"))
}
