use axum::{
    extract::{ConnectInfo, Path, Query, State},
    http::HeaderMap,
    Json,
};
use axum_extra::extract::cookie::PrivateCookieJar;
use std::net::SocketAddr;
use serde::Deserialize;
use serde_json::{json, Value};
use crate::{auth, db, dns as dns_lib};
use crate::state::AppState;
use tracing::error;

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Strip sensitive fields from credential JSON before sending to frontend.
/// Returns only the "shape" with keys present but values redacted to "••••".
fn redact_credentials(creds_json: &str) -> Value {
    match serde_json::from_str::<Value>(creds_json) {
        Ok(obj) => {
            if let Some(map) = obj.as_object() {
                let redacted: serde_json::Map<String, Value> = map.iter().map(|(k, v)| {
                    if v.as_str().map(|s| !s.is_empty()).unwrap_or(false) {
                        (k.clone(), Value::String("••••".to_string()))
                    } else {
                        (k.clone(), v.clone())
                    }
                }).collect();
                Value::Object(redacted)
            } else { Value::Object(Default::default()) }
        }
        Err(_) => Value::Object(Default::default()),
    }
}

fn dns_client(provider: &db::DnsProvider) -> anyhow::Result<dns_lib::DnsClient> {
    let creds: Value = serde_json::from_str(&provider.credentials)
        .unwrap_or(Value::Object(Default::default()));
    dns_lib::DnsClient::from_type(&provider.provider_type, &creds)
}

// ── GET /api/admin/dns/providers ──────────────────────────────────────────────

pub async fn api_dns_list_providers(State(state): State<AppState>) -> Json<Value> {
    match db::dns_list_providers(&state.db).await {
        Ok(providers) => {
            let list: Vec<Value> = providers.iter().map(|p| json!({
                "id":            p.id,
                "name":          p.name,
                "provider_type": p.provider_type,
                "enabled":       p.enabled,
                "credentials":   redact_credentials(&p.credentials),
                "created_at":    p.created_at,
            })).collect();  
            Json(json!({ "ok": true, "providers": list }))
        }
        Err(e) => { error!("{}", e); Json(json!({ "ok": false, "error": e.to_string() })) }
    }
}

// ── POST /api/admin/dns/providers ─────────────────────────────────────────────

#[derive(Deserialize)]
pub struct AddProviderBody {
    pub name:          String,
    pub provider_type: String,
    pub credentials:   Value, // raw credential object from form
}

pub async fn api_dns_add_provider(
    State(state): State<AppState>,
    addr: ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(body): Json<AddProviderBody>,
) -> Json<Value> {
    let ip = auth::client_ip(&headers, addr);
    let creds_str = body.credentials.to_string();
    match db::dns_add_provider(&state.db, &body.name, &body.provider_type, &creds_str).await {
        Ok(id) => {
            let _ = db::audit_log(&state.db, "admin", "dns.provider_add", &body.name, &body.provider_type, &ip).await;
            Json(json!({ "ok": true, "id": id }))
        }
        Err(e) => { error!("{}", e); Json(json!({ "ok": false, "error": e.to_string() })) }
    }
}

// ── POST /api/admin/dns/providers/:id/update ─────────────────────────────────

#[derive(Deserialize)]
pub struct UpdateProviderBody {
    pub name:        String,
    pub credentials: Value,
    pub enabled:     i64,
}

pub async fn api_dns_update_provider(
    State(state): State<AppState>,
    addr: ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Path(id): Path<i64>,
    Json(body): Json<UpdateProviderBody>,
) -> Json<Value> {
    let ip = auth::client_ip(&headers, addr);
    // Merge incoming credentials with existing (to allow partial updates — "••••" means keep)
    let existing = db::dns_get_provider(&state.db, id).await.ok().flatten();
    let creds_str = if let Some(existing) = existing {
        let mut cur: Value = serde_json::from_str(&existing.credentials)
            .unwrap_or(Value::Object(Default::default()));
        if let (Some(cur_map), Some(new_map)) = (cur.as_object_mut(), body.credentials.as_object()) {
            for (k, v) in new_map {
                if v.as_str() != Some("••••") {
                    cur_map.insert(k.clone(), v.clone());
                }
            }
        }
        cur.to_string()
    } else {
        body.credentials.to_string()
    };

    match db::dns_update_provider(&state.db, id, &body.name, &creds_str, body.enabled).await {
        Ok(_) => {
            let _ = db::audit_log(&state.db, "admin", "dns.provider_edit", &body.name, &format!("id={}", id), &ip).await;
            Json(json!({ "ok": true }))
        }
        Err(e) => { error!("{}", e); Json(json!({ "ok": false, "error": e.to_string() })) }
    }
}

// ── POST /api/admin/dns/providers/:id/delete ─────────────────────────────────

pub async fn api_dns_delete_provider(
    State(state): State<AppState>,
    addr: ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Path(id): Path<i64>,
) -> Json<Value> {
    let ip = auth::client_ip(&headers, addr);
    match db::dns_delete_provider(&state.db, id).await {
        Ok(_) => {
            let _ = db::audit_log(&state.db, "admin", "dns.provider_delete", "", &format!("id={}", id), &ip).await;
            Json(json!({ "ok": true }))
        }
        Err(e) => { error!("{}", e); Json(json!({ "ok": false, "error": e.to_string() })) }
    }
}

// ── POST /api/admin/dns/providers/:id/test ────────────────────────────────────

pub async fn api_dns_test_provider(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Json<Value> {
    let provider = match db::dns_get_provider(&state.db, id).await {
        Ok(Some(p)) => p,
        Ok(None)    => return Json(json!({ "ok": false, "error": "Provider not found" })),
        Err(e)      => return Json(json!({ "ok": false, "error": e.to_string() })),
    };
    let client = match dns_client(&provider) {
        Ok(c)  => c,
        Err(e) => return Json(json!({ "ok": false, "error": e.to_string() })),
    };
    match client.test().await {
        Ok(msg) => Json(json!({ "ok": true, "message": msg })),
        Err(e)  => Json(json!({ "ok": false, "error": e.to_string() })),
    }
}

// ── GET /api/admin/dns/providers/:id/zones ────────────────────────────────────

pub async fn api_dns_list_zones(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Json<Value> {
    let provider = match db::dns_get_provider(&state.db, id).await {
        Ok(Some(p)) => p,
        Ok(None)    => return Json(json!({ "ok": false, "error": "Provider not found" })),
        Err(e)      => return Json(json!({ "ok": false, "error": e.to_string() })),
    };
    let client = match dns_client(&provider) {
        Ok(c)  => c,
        Err(e) => return Json(json!({ "ok": false, "error": e.to_string() })),
    };
    match client.list_zones().await {
        Ok(zones) => Json(json!({ "ok": true, "zones": zones })),
        Err(e)    => Json(json!({ "ok": false, "error": e.to_string() })),
    }
}

// ── GET /api/admin/dns/providers/:id/records-remote?zone=<zone_id> ───────────

#[derive(Deserialize)]
pub struct ZoneQuery { pub zone: Option<String> }

pub async fn api_dns_remote_records(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Query(q): Query<ZoneQuery>,
) -> Json<Value> {
    let provider = match db::dns_get_provider(&state.db, id).await {
        Ok(Some(p)) => p,
        Ok(None)    => return Json(json!({ "ok": false, "error": "Provider not found" })),
        Err(e)      => return Json(json!({ "ok": false, "error": e.to_string() })),
    };
    let zone_id = q.zone.unwrap_or_default();
    let client = match dns_client(&provider) {
        Ok(c)  => c,
        Err(e) => return Json(json!({ "ok": false, "error": e.to_string() })),
    };
    match client.list_records(&zone_id).await {
        Ok(records) => Json(json!({ "ok": true, "records": records })),
        Err(e)      => Json(json!({ "ok": false, "error": e.to_string() })),
    }
}

// ── GET /api/admin/dns/providers/:id/records ─────────────────────────────────

pub async fn api_dns_local_records(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Json<Value> {
    match db::dns_list_records(&state.db, id).await {
        Ok(records) => Json(json!({ "ok": true, "records": records })),
        Err(e)      => Json(json!({ "ok": false, "error": e.to_string() })),
    }
}

// ── POST /api/admin/dns/records ───────────────────────────────────────────────

#[derive(Deserialize)]
pub struct AddRecordBody {
    pub provider_id:   i64,
    pub zone_id:       String,
    pub zone_name:     String,
    pub record_type:   String,
    pub name:          String,
    pub value:         String,
    pub ttl:           Option<i64>,
    pub priority:      Option<i64>,
    pub proxied:       Option<bool>,
    pub ddns_enabled:  Option<bool>,
    pub ddns_interval: Option<i64>,
    pub container_id:  Option<i64>,
    pub push_to_provider: Option<bool>, // if true, create new record in provider
    pub remote_id:        Option<String>, // existing remote record ID (for import)
    pub tag_on_provider:  Option<bool>,   // if true + remote_id set, update comment on provider
}

pub async fn api_dns_add_record(
    State(state): State<AppState>,
    addr: ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(body): Json<AddRecordBody>,
) -> Json<Value> {
    let ip = auth::client_ip(&headers, addr);
    let provider = match db::dns_get_provider(&state.db, body.provider_id).await {
        Ok(Some(p)) => p,
        Ok(None)    => return Json(json!({ "ok": false, "error": "Provider not found" })),
        Err(e)      => return Json(json!({ "ok": false, "error": e.to_string() })),
    };

    let ttl        = body.ttl.unwrap_or(300);
    let priority   = body.priority.unwrap_or(0);
    let proxied    = body.proxied.unwrap_or(false);
    let ddns       = body.ddns_enabled.unwrap_or(false);
    let interval   = body.ddns_interval.unwrap_or(300);
    let push       = body.push_to_provider.unwrap_or(false);
    let tag        = body.tag_on_provider.unwrap_or(false);
    let given_rid  = body.remote_id.clone().unwrap_or_default();

    // Determine the remote_id to store:
    // 1. push_to_provider=true  → create new record, use returned ID
    // 2. tag_on_provider=true   → record already exists; call update_record to write
    //                             yunexal.managed=true comment, keep the given remote_id
    // 3. neither                → store empty remote_id
    let remote_id = if push {
        match dns_client(&provider) {
            Ok(client) => {
                match client.create_record(&body.zone_id, &dns_lib::DnsRecordInput {
                    record_type: body.record_type.clone(),
                    name:        body.name.clone(),
                    value:       body.value.clone(),
                    ttl, priority, proxied,
                }).await {
                    Ok(rid) => rid,
                    Err(e)  => return Json(json!({ "ok": false, "error": format!("Provider error: {}", e) })),
                }
            }
            Err(e) => return Json(json!({ "ok": false, "error": e.to_string() })),
        }
    } else if tag && !given_rid.is_empty() {
        // Update the existing record on the provider to stamp yunexal.managed=true
        match dns_client(&provider) {
            Ok(client) => {
                // Best-effort: ignore errors (record gets tracked even if comment fails)
                let _ = client.update_record(&body.zone_id, &given_rid, &dns_lib::DnsRecordInput {
                    record_type: body.record_type.clone(),
                    name:        body.name.clone(),
                    value:       body.value.clone(),
                    ttl, priority, proxied,
                }).await;
            }
            Err(_) => {}
        }
        given_rid.clone()
    } else if !given_rid.is_empty() {
        // Import without tagging — just store the remote_id as reference
        given_rid.clone()
    } else { String::new() };

    match db::dns_add_record(
        &state.db, body.provider_id,
        &body.zone_id, &body.zone_name,
        &body.record_type, &body.name, &body.value,
        ttl, priority, proxied, &remote_id,
        body.container_id, ddns, interval,
    ).await {
        Ok(id) => {
            let _ = db::audit_log(&state.db, "admin", "dns.record_add", &body.name, &format!("{} {}", body.record_type, body.value), &ip).await;
            Json(json!({ "ok": true, "id": id, "remote_id": remote_id }))
        }
        Err(e) => { error!("{}", e); Json(json!({ "ok": false, "error": e.to_string() })) }
    }
}

// ── POST /api/admin/dns/records/:id/update ────────────────────────────────────

#[derive(Deserialize)]
pub struct UpdateRecordBody {
    pub name:          String,
    pub value:         String,
    pub ttl:           Option<i64>,
    pub priority:      Option<i64>,
    pub proxied:       Option<bool>,
    pub ddns_enabled:  Option<bool>,
    pub ddns_interval: Option<i64>,
    pub push_to_provider: Option<bool>,
}

pub async fn api_dns_update_record(
    State(state): State<AppState>,
    addr: ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Path(id): Path<i64>,
    Json(body): Json<UpdateRecordBody>,
) -> Json<Value> {
    let ip = auth::client_ip(&headers, addr);
    let ttl      = body.ttl.unwrap_or(300);
    let priority = body.priority.unwrap_or(0);
    let proxied  = body.proxied.unwrap_or(false);
    let ddns     = body.ddns_enabled.unwrap_or(false);
    let interval = body.ddns_interval.unwrap_or(300);
    let push     = body.push_to_provider.unwrap_or(false);

    // Push to provider if requested
    if push {
        // Get the record's provider id and zone by searching all providers
        if let Ok(providers) = db::dns_list_providers(&state.db).await {
            'outer: for provider in &providers {
                if let Ok(recs) = db::dns_list_records(&state.db, provider.id).await {
                    for rec in &recs {
                        if rec.id == id {
                            if let Ok(client) = dns_client(provider) {
                                let _ = client.update_record(&rec.zone_id, &rec.remote_id, &dns_lib::DnsRecordInput {
                                    record_type: rec.record_type.clone(),
                                    name: body.name.clone(),
                                    value: body.value.clone(),
                                    ttl, priority, proxied,
                                }).await;
                            }
                            break 'outer;
                        }
                    }
                }
            }
        }
    }

    match db::dns_update_record(&state.db, id, &body.name, &body.value, ttl, priority, proxied, ddns, interval).await {
        Ok(_)  => {
            let _ = db::audit_log(&state.db, "admin", "dns.record_edit", &body.name, &format!("id={}", id), &ip).await;
            Json(json!({ "ok": true }))
        }
        Err(e) => { error!("{}", e); Json(json!({ "ok": false, "error": e.to_string() })) }
    }
}

// ── POST /api/admin/dns/records/:id/delete ────────────────────────────────────

#[derive(Deserialize)]
pub struct DeleteRecordBody { pub remove_from_provider: Option<bool> }

pub async fn api_dns_delete_record(
    State(state): State<AppState>,
    addr: ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Path(id): Path<i64>,
    Json(body): Json<DeleteRecordBody>,
) -> Json<Value> {
    let ip = auth::client_ip(&headers, addr);
    let remove = body.remove_from_provider.unwrap_or(false);
    if remove {
        if let Ok(providers) = db::dns_list_providers(&state.db).await {
            'outer: for provider in &providers {
                if let Ok(recs) = db::dns_list_records(&state.db, provider.id).await {
                    for rec in &recs {
                        if rec.id == id && !rec.remote_id.is_empty() {
                            if let Ok(client) = dns_client(provider) {
                                let _ = client.delete_record(&rec.zone_id, &rec.remote_id).await;
                            }
                            break 'outer;
                        }
                    }
                }
            }
        }
    }

    match db::dns_delete_record(&state.db, id).await {
        Ok(_)  => {
            let _ = db::audit_log(&state.db, "admin", "dns.record_delete", "", &format!("id={}", id), &ip).await;
            Json(json!({ "ok": true }))
        }
        Err(e) => { error!("{}", e); Json(json!({ "ok": false, "error": e.to_string() })) }
    }
}

// ── POST /api/admin/dns/records/:id/set-proxy ─────────────────────────────────
/// Toggle the Cloudflare orange-cloud proxy for a single tracked record.
/// Sends a minimal PATCH to Cloudflare (only the `proxied` field) and
/// updates the local DB.

#[derive(Deserialize)]
pub struct SetProxyBody { pub proxied: bool }

pub async fn api_dns_set_proxy(
    State(state): State<AppState>,
    addr: ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Path(id): Path<i64>,
    Json(body): Json<SetProxyBody>,
) -> Json<Value> {
    let ip = auth::client_ip(&headers, addr);
    // Find the record + its provider
    if let Ok(providers) = db::dns_list_providers(&state.db).await {
        'outer: for provider in &providers {
            if let Ok(recs) = db::dns_list_records(&state.db, provider.id).await {
                for rec in &recs {
                    if rec.id != id { continue; }
                    if rec.remote_id.is_empty() {
                        return Json(json!({ "ok": false, "error": "Record has no remote ID — not pushed to provider yet" }));
                    }
                    match dns_client(provider) {
                        Ok(client) => {
                            if let Err(e) = client.set_proxy(&rec.zone_id, &rec.remote_id, body.proxied).await {
                                return Json(json!({ "ok": false, "error": e.to_string() }));
                            }
                        }
                        Err(e) => return Json(json!({ "ok": false, "error": e.to_string() })),
                    }
                    // Persist to local DB (preserve all other fields)
                    let _ = db::dns_update_record(
                        &state.db, id,
                        &rec.name, &rec.value, rec.ttl, rec.priority,
                        body.proxied, rec.ddns_enabled != 0, rec.ddns_interval,
                    ).await;
                    break 'outer;
                }
            }
        }
    }
    let _ = db::audit_log(&state.db, "admin", "dns.proxy_toggle", "", &format!("id={} proxied={}", id, body.proxied), &ip).await;
    Json(json!({ "ok": true, "proxied": body.proxied }))
}

// ── GET /api/admin/dns/public-ip ─────────────────────────────────────────────

pub async fn api_dns_public_ip() -> Json<Value> {
    match dns_lib::get_public_ip().await {
        Ok(ip) => Json(json!({ "ok": true, "ip": ip })),
        Err(e) => Json(json!({ "ok": false, "error": e.to_string() })),
    }
}

// ── POST /api/admin/dns/sync ──────────────────────────────────────────────────
/// Manually trigger DDNS sync for all enabled records.

pub async fn api_dns_sync(State(state): State<AppState>, addr: ConnectInfo<SocketAddr>, headers: HeaderMap) -> Json<Value> {
    let req_ip = auth::client_ip(&headers, addr);
    let ip = match dns_lib::get_public_ip().await {
        Ok(ip) => ip,
        Err(e) => return Json(json!({ "ok": false, "error": format!("Cannot get public IP: {}", e) })),
    };

    let records = match db::dns_list_ddns_records(&state.db).await {
        Ok(r)  => r,
        Err(e) => return Json(json!({ "ok": false, "error": e.to_string() })),
    };

    let mut synced = 0u32;
    let mut errors: Vec<String> = vec![];

    for rec in &records {
        let provider = match db::dns_get_provider(&state.db, rec.provider_id).await {
            Ok(Some(p)) => p,
            _ => continue,
        };
        let client = match dns_client(&provider) {
            Ok(c)  => c,
            Err(e) => { errors.push(format!("{}: {}", rec.name, e)); continue; }
        };
        match client.update_ddns(&rec.zone_id, &rec.name, &ip).await {
            Ok(_) => {
                let _ = db::dns_update_record_ip(&state.db, rec.id, &ip).await;
                synced += 1;
            }
            Err(e) => errors.push(format!("{}: {}", rec.name, e)),
        }
    }

    let _ = db::audit_log(&state.db, "admin", "dns.sync", "", &format!("ip={} synced={}", ip, synced), &req_ip).await;

    Json(json!({
        "ok":     true,
        "ip":     ip,
        "synced": synced,
        "errors": errors,
    }))
}

// ── POST /api/admin/dns/providers/:id/sync-records?zone=:zid ──────────────────
// Fetches live records from provider and updates the local DB for all tracked
// records that have a matching remote_id (name, value, ttl, priority, proxied).

#[derive(Deserialize)]
pub struct SyncRecordsQuery { pub zone: String }

pub async fn api_dns_sync_records(
    State(state): State<AppState>,
    addr: ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Path(pid): Path<i64>,
    Query(params): Query<SyncRecordsQuery>,
) -> Json<Value> {
    let ip = auth::client_ip(&headers, addr);
    let zid = &params.zone;

    let provider = match db::dns_get_provider(&state.db, pid).await {
        Ok(Some(p)) => p,
        Ok(None)    => return Json(json!({ "ok": false, "error": "Provider not found" })),
        Err(e)      => return Json(json!({ "ok": false, "error": e.to_string() })),
    };
    let client = match dns_client(&provider) {
        Ok(c)  => c,
        Err(e) => return Json(json!({ "ok": false, "error": e.to_string() })),
    };

    // Fetch live records from provider
    let remote = match client.list_records(zid).await {
        Ok(r)  => r,
        Err(e) => return Json(json!({ "ok": false, "error": format!("Provider error: {}", e) })),
    };

    // Fetch local tracked records for this provider
    let local = match db::dns_list_records(&state.db, pid).await {
        Ok(r)  => r,
        Err(e) => return Json(json!({ "ok": false, "error": e.to_string() })),
    };

    // Build map remote_id → live remote record
    let mut rmap: std::collections::HashMap<String, &dns_lib::RemoteDnsRecord> =
        std::collections::HashMap::new();
    for r in &remote {
        if !r.id.is_empty() { rmap.insert(r.id.clone(), r); }
    }

    // Update each locally-tracked record whose remote_id matches a live record
    let mut synced = 0i64;
    for loc in &local {
        if loc.remote_id.is_empty() { continue; }
        if let Some(rem) = rmap.get(&loc.remote_id) {
            let _ = db::dns_update_record(
                &state.db, loc.id,
                &rem.name, &rem.value, rem.ttl, rem.priority,
                rem.proxied, loc.ddns_enabled != 0, loc.ddns_interval,
            ).await;
            synced += 1;
        }
    }

    let _ = db::audit_log(&state.db, "admin", "dns.sync_records", "", &format!("provider={} synced={}", pid, synced), &ip).await;

    Json(json!({ "ok": true, "synced": synced }))
}

// ── GET /api/admin/dns/container-records ─────────────────────────────────────
// Lists every panel-tracked record linked to a container, enriched with server name.

pub async fn api_dns_container_records(State(state): State<AppState>) -> Json<Value> {
    let records = match db::dns_list_all_container_records(&state.db).await {
        Ok(r)  => r,
        Err(e) => return Json(json!({ "ok": false, "error": e.to_string() })),
    };
    // Build server_id → name map
    let servers: std::collections::HashMap<i64, String> =
        db::list_servers_basic_info(&state.db).await
            .unwrap_or_default()
            .into_iter()
            .map(|(id, name, _)| (id, name))
            .collect();

    let list: Vec<Value> = records.iter().map(|r| {
        let sname = r.container_id
            .and_then(|id| servers.get(&id).cloned())
            .unwrap_or_default();
        json!({
            "id":           r.id,
            "server_id":    r.container_id,
            "server_name":  sname,
            "provider_id":  r.provider_id,
            "zone_id":      r.zone_id,
            "zone_name":    r.zone_name,
            "record_type":  r.record_type,
            "name":         r.name,
            "value":        r.value,
            "ttl":          r.ttl,
            "priority":     r.priority,
            "proxied":      r.proxied != 0,
            "remote_id":    r.remote_id,
            "ddns_enabled": r.ddns_enabled != 0,
            "created_at":   r.created_at,
        })
    }).collect();

    Json(json!({ "ok": true, "records": list }))
}

// ── GET /api/servers/:id/dns ──────────────────────────────────────────────────
// Lists DNS records linked to a specific server (authenticated, not admin-only).

pub async fn api_server_dns_list(
    State(state): State<AppState>,
    jar: PrivateCookieJar,
    Path(server_id): Path<i64>,
) -> Json<Value> {
    if !auth::can_access_server(&state, &jar, server_id).await {
        return Json(json!({ "ok": false, "error": "Access denied" }));
    }
    match db::dns_list_records_by_server_id(&state.db, server_id).await {
        Ok(records) => {
            let list: Vec<Value> = records.iter().map(|r| json!({
                "id":          r.id,
                "provider_id": r.provider_id,
                "zone_name":   r.zone_name,
                "record_type": r.record_type,
                "name":        r.name,
                "value":       r.value,
                "ttl":         r.ttl,
                "priority":    r.priority,
                "proxied":     r.proxied != 0,
                "remote_id":   r.remote_id,
                "ddns_enabled":r.ddns_enabled != 0,
                "created_at":  r.created_at,
            })).collect();
            Json(json!({ "ok": true, "records": list }))
        }
        Err(e) => Json(json!({ "ok": false, "error": e.to_string() })),
    }
}

// ── POST /api/servers/:id/dns/add ─────────────────────────────────────────────
// Adds a DNS record linked to a server. Any authenticated user (owner check is
// done at the auth layer since protected routes only reach owner-accessible ids).

#[derive(Deserialize)]
pub struct AddServerDnsBody {
    pub provider_id:   i64,
    pub zone_id:       String,
    pub zone_name:     String,
    pub record_type:   String,
    pub name:          String,
    pub value:         String,
    pub ttl:           Option<i64>,
    pub priority:      Option<i64>,
    pub proxied:       Option<bool>,
}

pub async fn api_server_dns_add(
    State(state): State<AppState>,
    jar: PrivateCookieJar,
    addr: ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Path(server_id): Path<i64>,
    Json(body): Json<AddServerDnsBody>,
) -> Json<Value> {
    let ip = auth::client_ip(&headers, addr);
    if !auth::can_access_server(&state, &jar, server_id).await {
        return Json(json!({ "ok": false, "error": "Access denied" }));
    }
    let provider = match db::dns_get_provider(&state.db, body.provider_id).await {
        Ok(Some(p)) => p,
        Ok(None)    => return Json(json!({ "ok": false, "error": "Provider not found" })),
        Err(e)      => return Json(json!({ "ok": false, "error": e.to_string() })),
    };
    let ttl      = body.ttl.unwrap_or(1);
    let priority = body.priority.unwrap_or(0);
    let proxied  = body.proxied.unwrap_or(false);

    let creds: Value = serde_json::from_str(&provider.credentials)
        .unwrap_or(Value::Object(Default::default()));
    let client = match dns_lib::DnsClient::from_type(&provider.provider_type, &creds) {
        Ok(c)  => c,
        Err(e) => return Json(json!({ "ok": false, "error": e.to_string() })),
    };
    let remote_id = client.create_record(&body.zone_id, &dns_lib::DnsRecordInput {
        record_type: body.record_type.clone(),
        name:        body.name.clone(),
        value:       body.value.clone(),
        ttl, priority, proxied,
    }).await.unwrap_or_default();

    match db::dns_add_record(
        &state.db, body.provider_id,
        &body.zone_id, &body.zone_name,
        &body.record_type, &body.name, &body.value,
        ttl, priority, proxied, &remote_id,
        Some(server_id), false, 300,
    ).await {
        Ok(id) => {
            let actor = auth::session_username(&jar).unwrap_or_default();
            let _ = db::audit_log(&state.db, &actor, "dns.server_record_add", &body.name, &format!("server={}", server_id), &ip).await;
            Json(json!({ "ok": true, "id": id, "remote_id": remote_id }))
        }
        Err(e) => Json(json!({ "ok": false, "error": e.to_string() })),
    }
}

// ── POST /api/servers/:id/dns/:record_id/delete ───────────────────────────────
// Deletes a server-linked DNS record from provider API and local DB.

pub async fn api_server_dns_delete(
    State(state): State<AppState>,
    jar: PrivateCookieJar,
    addr: ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Path((server_id, record_id)): Path<(i64, i64)>,
) -> Json<Value> {
    let ip = auth::client_ip(&headers, addr);
    if !auth::can_access_server(&state, &jar, server_id).await {
        return Json(json!({ "ok": false, "error": "Access denied" }));
    }
    // Load record and verify it belongs to this server
    let records = match db::dns_list_records_by_server_id(&state.db, server_id).await {
        Ok(r)  => r,
        Err(e) => return Json(json!({ "ok": false, "error": e.to_string() })),
    };
    let rec = match records.iter().find(|r| r.id == record_id) {
        Some(r) => r.clone(),
        None    => return Json(json!({ "ok": false, "error": "Record not found or not owned by this server" })),
    };

    // Remove from provider (best-effort)
    if !rec.remote_id.is_empty() {
        if let Ok(Some(provider)) = db::dns_get_provider(&state.db, rec.provider_id).await {
            let creds: Value = serde_json::from_str(&provider.credentials)
                .unwrap_or(Value::Object(Default::default()));
            if let Ok(client) = dns_lib::DnsClient::from_type(&provider.provider_type, &creds) {
                if let Err(e) = client.delete_record(&rec.zone_id, &rec.remote_id).await {
                    error!("api_server_dns_delete: provider delete failed: {}", e);
                }
            }
        }
    }

    match db::dns_delete_record(&state.db, record_id).await {
        Ok(()) => {
            let actor = auth::session_username(&jar).unwrap_or_default();
            let _ = db::audit_log(&state.db, &actor, "dns.server_record_delete", &rec.name, &format!("server={}", server_id), &ip).await;
            Json(json!({ "ok": true }))
        }
        Err(e) => Json(json!({ "ok": false, "error": e.to_string() })),
    }
}

