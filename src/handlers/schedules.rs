use axum::{
    extract::{Extension, Path, State},
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};
use axum_extra::extract::cookie::PrivateCookieJar;
use crate::{auth, db};
use crate::db::schedules;
use crate::state::AppState;
use crate::docker;
use super::CspNonce;
use super::templates::{render, SchedulesTemplate};

pub async fn schedules_page(
    State(state): State<AppState>,
    jar: PrivateCookieJar,
    Path(db_id): Path<i64>,
    Extension(CspNonce(nonce)): Extension<CspNonce>,
) -> impl IntoResponse {
    if !auth::can_access_server_permission(&state, &jar, db_id, "schedules", false).await {
        return (axum::http::StatusCode::FORBIDDEN, "Access denied").into_response();
    }
    let can_write = auth::can_access_server_permission(&state, &jar, db_id, "schedules", true).await;
    let can_members = auth::can_access_server_permission(&state, &jar, db_id, "members", false).await;
    let (docker_id, db_name) = match db::get_server_info_by_db_id(&state.db, db_id).await {
        Ok(Some(v)) => v,
        _ => return (axum::http::StatusCode::NOT_FOUND, "Server not found").into_response(),
    };
    match docker::get_container(&state.docker, &docker_id).await {
        Ok(mut c) => {
            c.db_id = db_id;
            c.name = db_name;
            render(SchedulesTemplate {
                id: db_id,
                container: c,
                can_members,
                can_write,
                active_tab: "schedules",
                nonce,
            }).into_response()
        }
        Err(e) => format!("Error: {e}").into_response(),
    }
}

#[derive(Serialize)]
struct ScheduleJson {
    id: i64,
    name: String,
    cron_expression: String,
    task_type: String,
    task_payload: String,
    enabled: bool,
    last_run_at: Option<String>,
    next_run_at: Option<String>,
    created_at: String,
}

pub async fn api_list_schedules(
    State(state): State<AppState>,
    jar: PrivateCookieJar,
    Path(db_id): Path<i64>,
) -> impl IntoResponse {
    if !auth::can_access_server_permission(&state, &jar, db_id, "schedules", false).await {
        return (axum::http::StatusCode::FORBIDDEN, Json(serde_json::json!({"error":"Access denied"}))).into_response();
    }
    match schedules::list_schedules(&state.db, db_id).await {
        Ok(list) => {
            let out: Vec<ScheduleJson> = list.into_iter().map(|s| ScheduleJson {
                id: s.id,
                name: s.name,
                cron_expression: s.cron_expression,
                task_type: s.task_type,
                task_payload: s.task_payload,
                enabled: s.enabled != 0,
                last_run_at: s.last_run_at,
                next_run_at: s.next_run_at,
                created_at: s.created_at,
            }).collect();
            Json(serde_json::json!({"schedules": out})).into_response()
        }
        Err(e) => (axum::http::StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e.to_string()}))).into_response(),
    }
}

#[derive(Deserialize)]
pub struct CreateScheduleBody {
    pub name: String,
    pub cron_expression: String,
    pub task_type: String,
    pub task_payload: String,
}

pub async fn api_create_schedule(
    State(state): State<AppState>,
    jar: PrivateCookieJar,
    Path(db_id): Path<i64>,
    Json(body): Json<CreateScheduleBody>,
) -> impl IntoResponse {
    if !auth::can_access_server_permission(&state, &jar, db_id, "schedules", true).await {
        return (axum::http::StatusCode::FORBIDDEN, Json(serde_json::json!({"error":"Access denied"}))).into_response();
    }
    if body.name.trim().is_empty() {
        return (axum::http::StatusCode::BAD_REQUEST, Json(serde_json::json!({"error":"Name is required"}))).into_response();
    }
    if schedules::compute_next_run(&body.cron_expression).is_none() {
        return (axum::http::StatusCode::BAD_REQUEST, Json(serde_json::json!({"error":"Invalid cron expression"}))).into_response();
    }
    if !matches!(body.task_type.as_str(), "command" | "power" | "backup" | "script") {
        return (axum::http::StatusCode::BAD_REQUEST, Json(serde_json::json!({"error":"Invalid task type"}))).into_response();
    }
    if let Err(err) = validate_task_payload(&body.task_type, &body.task_payload) {
        return (axum::http::StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": err}))).into_response();
    }
    match schedules::create_schedule(&state.db, db_id, &body.name, &body.cron_expression, &body.task_type, &body.task_payload).await {
        Ok(id) => Json(serde_json::json!({"id": id})).into_response(),
        Err(e) => (axum::http::StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e.to_string()}))).into_response(),
    }
}

#[derive(Deserialize)]
pub struct UpdateScheduleBody {
    pub name: String,
    pub cron_expression: String,
    pub task_type: String,
    pub task_payload: String,
    pub enabled: bool,
}

pub async fn api_update_schedule(
    State(state): State<AppState>,
    jar: PrivateCookieJar,
    Path((db_id, sid)): Path<(i64, i64)>,
    Json(body): Json<UpdateScheduleBody>,
) -> impl IntoResponse {
    if !auth::can_access_server_permission(&state, &jar, db_id, "schedules", true).await {
        return (axum::http::StatusCode::FORBIDDEN, Json(serde_json::json!({"error":"Access denied"}))).into_response();
    }
    if schedules::compute_next_run(&body.cron_expression).is_none() {
        return (axum::http::StatusCode::BAD_REQUEST, Json(serde_json::json!({"error":"Invalid cron expression"}))).into_response();
    }
    if let Err(err) = validate_task_payload(&body.task_type, &body.task_payload) {
        return (axum::http::StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": err}))).into_response();
    }
    // Verify schedule belongs to this server
    match schedules::get_schedule(&state.db, sid).await {
        Ok(Some(s)) if s.server_id == db_id => {}
        _ => return (axum::http::StatusCode::NOT_FOUND, Json(serde_json::json!({"error":"Schedule not found"}))).into_response(),
    }
    match schedules::update_schedule(&state.db, sid, &body.name, &body.cron_expression, &body.task_type, &body.task_payload, body.enabled).await {
        Ok(_) => Json(serde_json::json!({"ok": true})).into_response(),
        Err(e) => (axum::http::StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e.to_string()}))).into_response(),
    }
}

fn validate_task_payload(task_type: &str, task_payload: &str) -> Result<(), String> {
    let value: serde_json::Value = serde_json::from_str(task_payload)
        .map_err(|_| "Task payload must be valid JSON".to_string())?;

    match task_type {
        "command" => {
            let cmd = value.get("cmd").and_then(|v| v.as_str()).unwrap_or("").trim();
            if cmd.is_empty() { Err("Command payload needs a cmd value".into()) } else { Ok(()) }
        }
        "power" => {
            let action = value.get("action").and_then(|v| v.as_str()).unwrap_or("");
            if matches!(action, "start" | "stop" | "restart" | "kill") { Ok(()) } else { Err("Power payload needs a valid action".into()) }
        }
        "backup" => Ok(()),
        "script" => {
            let steps = value.get("steps").and_then(|v| v.as_array()).ok_or_else(|| "Script payload needs a steps array".to_string())?;
            if steps.is_empty() { return Err("Script needs at least one step".into()); }
            for step in steps {
                let step_type = step.get("type").and_then(|v| v.as_str()).unwrap_or("");
                match step_type {
                    "command" => {
                        let cmd = step.get("cmd").and_then(|v| v.as_str()).unwrap_or("").trim();
                        if cmd.is_empty() { return Err("Script command step needs cmd".into()); }
                    }
                    "power" => {
                        let action = step.get("action").and_then(|v| v.as_str()).unwrap_or("");
                        if !matches!(action, "start" | "stop" | "restart" | "kill") { return Err("Script power step needs valid action".into()); }
                    }
                    "backup" => {}
                    "delay" => {
                        let secs = step.get("seconds").and_then(|v| v.as_u64()).unwrap_or(0);
                        if secs == 0 { return Err("Delay step needs seconds > 0".into()); }
                    }
                    _ => return Err("Script steps must be command, power, backup, or delay".into()),
                }
            }
            Ok(())
        }
        _ => Err("Invalid task type".into()),
    }
}

pub async fn api_delete_schedule(
    State(state): State<AppState>,
    jar: PrivateCookieJar,
    Path((db_id, sid)): Path<(i64, i64)>,
) -> impl IntoResponse {
    if !auth::can_access_server_permission(&state, &jar, db_id, "schedules", true).await {
        return (axum::http::StatusCode::FORBIDDEN, Json(serde_json::json!({"error":"Access denied"}))).into_response();
    }
    match schedules::get_schedule(&state.db, sid).await {
        Ok(Some(s)) if s.server_id == db_id => {}
        _ => return (axum::http::StatusCode::NOT_FOUND, Json(serde_json::json!({"error":"Schedule not found"}))).into_response(),
    }
    match schedules::delete_schedule(&state.db, sid).await {
        Ok(_) => Json(serde_json::json!({"ok": true})).into_response(),
        Err(e) => (axum::http::StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e.to_string()}))).into_response(),
    }
}

#[derive(Deserialize)]
pub struct ToggleBody {
    pub enabled: bool,
}

pub async fn api_toggle_schedule(
    State(state): State<AppState>,
    jar: PrivateCookieJar,
    Path((db_id, sid)): Path<(i64, i64)>,
    Json(body): Json<ToggleBody>,
) -> impl IntoResponse {
    if !auth::can_access_server_permission(&state, &jar, db_id, "schedules", true).await {
        return (axum::http::StatusCode::FORBIDDEN, Json(serde_json::json!({"error":"Access denied"}))).into_response();
    }
    match schedules::get_schedule(&state.db, sid).await {
        Ok(Some(s)) if s.server_id == db_id => {}
        _ => return (axum::http::StatusCode::NOT_FOUND, Json(serde_json::json!({"error":"Schedule not found"}))).into_response(),
    }
    match schedules::set_schedule_enabled(&state.db, sid, body.enabled).await {
        Ok(_) => Json(serde_json::json!({"ok": true})).into_response(),
        Err(e) => (axum::http::StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e.to_string()}))).into_response(),
    }
}

pub async fn api_list_runs(
    State(state): State<AppState>,
    jar: PrivateCookieJar,
    Path((db_id, sid)): Path<(i64, i64)>,
) -> impl IntoResponse {
    if !auth::can_access_server_permission(&state, &jar, db_id, "schedules", false).await {
        return (axum::http::StatusCode::FORBIDDEN, Json(serde_json::json!({"error":"Access denied"}))).into_response();
    }
    match schedules::get_schedule(&state.db, sid).await {
        Ok(Some(s)) if s.server_id == db_id => {}
        _ => return (axum::http::StatusCode::NOT_FOUND, Json(serde_json::json!({"error":"Schedule not found"}))).into_response(),
    }
    let max_runs = schedules::get_server_max_schedule_runs(&state.db, db_id).await;
    match schedules::list_runs(&state.db, sid, max_runs).await {
        Ok(runs) => Json(serde_json::json!({"runs": runs})).into_response(),
        Err(e) => (axum::http::StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e.to_string()}))).into_response(),
    }
}

pub async fn api_run_now(
    State(state): State<AppState>,
    jar: PrivateCookieJar,
    Path((db_id, sid)): Path<(i64, i64)>,
) -> impl IntoResponse {
    if !auth::can_access_server_permission(&state, &jar, db_id, "schedules", true).await {
        return (axum::http::StatusCode::FORBIDDEN, Json(serde_json::json!({"error":"Access denied"}))).into_response();
    }
    let schedule = match schedules::get_schedule(&state.db, sid).await {
        Ok(Some(s)) if s.server_id == db_id => s,
        _ => return (axum::http::StatusCode::NOT_FOUND, Json(serde_json::json!({"error":"Schedule not found"}))).into_response(),
    };

    let pool = state.db.clone();
    let docker = state.docker.clone();
    tokio::spawn(async move {
        let _ = crate::scheduler::execute_manual(&pool, &docker, schedule).await;
    });

    Json(serde_json::json!({"ok": true, "message": "Queued"})).into_response()
}
