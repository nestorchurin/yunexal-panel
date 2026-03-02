use axum::{
    extract::{Path, Query, State},
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};
use crate::{db, dns as dns_lib};
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
    Json(body): Json<AddProviderBody>,
) -> Json<Value> {
    let creds_str = body.credentials.to_string();
    match db::dns_add_provider(&state.db, &body.name, &body.provider_type, &creds_str).await {
        Ok(id) => Json(json!({ "ok": true, "id": id })),
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
    Path(id): Path<i64>,
    Json(body): Json<UpdateProviderBody>,
) -> Json<Value> {
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
        Ok(_) => Json(json!({ "ok": true })),
        Err(e) => { error!("{}", e); Json(json!({ "ok": false, "error": e.to_string() })) }
    }
}

// ── POST /api/admin/dns/providers/:id/delete ─────────────────────────────────

pub async fn api_dns_delete_provider(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Json<Value> {
    match db::dns_delete_provider(&state.db, id).await {
        Ok(_) => Json(json!({ "ok": true })),
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
    pub push_to_provider: Option<bool>, // if true, also create in provider
}

pub async fn api_dns_add_record(
    State(state): State<AppState>,
    Json(body): Json<AddRecordBody>,
) -> Json<Value> {
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

    // Optionally push to provider API
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
    } else { String::new() };

    match db::dns_add_record(
        &state.db, body.provider_id,
        &body.zone_id, &body.zone_name,
        &body.record_type, &body.name, &body.value,
        ttl, priority, proxied, &remote_id,
        body.container_id, ddns, interval,
    ).await {
        Ok(id) => Json(json!({ "ok": true, "id": id, "remote_id": remote_id })),
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
    Path(id): Path<i64>,
    Json(body): Json<UpdateRecordBody>,
) -> Json<Value> {
    let ttl      = body.ttl.unwrap_or(300);
    let priority = body.priority.unwrap_or(0);
    let proxied  = body.proxied.unwrap_or(false);
    let ddns     = body.ddns_enabled.unwrap_or(false);
    let interval = body.ddns_interval.unwrap_or(300);
    let push     = body.push_to_provider.unwrap_or(false);

    // Fetch the current record to get remote_id + provider
    let records = match db::dns_list_records(&state.db, 0).await {
        Ok(_) => (), // just checking — we'll get the provider separately
        Err(_) => (),
    };
    let _ = records;

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
        Ok(_)  => Json(json!({ "ok": true })),
        Err(e) => { error!("{}", e); Json(json!({ "ok": false, "error": e.to_string() })) }
    }
}

// ── POST /api/admin/dns/records/:id/delete ────────────────────────────────────

#[derive(Deserialize)]
pub struct DeleteRecordBody { pub remove_from_provider: Option<bool> }

pub async fn api_dns_delete_record(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(body): Json<DeleteRecordBody>,
) -> Json<Value> {
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
        Ok(_)  => Json(json!({ "ok": true })),
        Err(e) => { error!("{}", e); Json(json!({ "ok": false, "error": e.to_string() })) }
    }
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

pub async fn api_dns_sync(State(state): State<AppState>) -> Json<Value> {
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
    Path(pid): Path<i64>,
    Query(params): Query<SyncRecordsQuery>,
) -> Json<Value> {
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

    Json(json!({ "ok": true, "synced": synced }))
}
