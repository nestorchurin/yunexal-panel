use anyhow::{Context, Result};
use sqlx::{FromRow, Pool, Sqlite};

#[derive(Debug, Clone, FromRow, serde::Serialize)]
pub struct AuditEntry {
    pub id: i64,
    pub actor: String,
    pub action: String,
    pub target: String,
    pub detail: String,
    pub ip: String,
    pub user_agent: String,
    pub created_at: String,
}

pub async fn audit_log(
    pool: &Pool<Sqlite>,
    actor: &str,
    action: &str,
    target: &str,
    detail: &str,
    ip: &str,
    user_agent: &str,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO audit_log (actor, action, target, detail, ip, user_agent) VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(actor)
    .bind(action)
    .bind(target)
    .bind(detail)
    .bind(ip)
    .bind(user_agent)
    .execute(pool)
    .await
    .context("Failed to insert audit log")?;
    Ok(())
}

pub async fn audit_list(
    pool: &Pool<Sqlite>,
    limit: i64,
    offset: i64,
    action: &str,
    actor: &str,
    search: &str,
) -> Result<Vec<AuditEntry>> {
    let action_parts: Vec<&str> = action.split(',').filter(|s| !s.is_empty()).collect();
    let mut sql = String::from("SELECT id, actor, action, target, detail, ip, COALESCE(user_agent,'') as user_agent, created_at FROM audit_log WHERE 1=1");
    if !action_parts.is_empty() {
        let ph = action_parts.iter().map(|_| "?").collect::<Vec<_>>().join(", ");
        sql.push_str(&format!(" AND action IN ({})", ph));
    }
    if !actor.is_empty()  { sql.push_str(" AND actor = ?"); }
    if !search.is_empty() { sql.push_str(" AND (target LIKE ? OR detail LIKE ? OR actor LIKE ? OR action LIKE ? OR ip LIKE ?)"); }
    sql.push_str(" ORDER BY id DESC LIMIT ? OFFSET ?");

    let mut q = sqlx::query_as::<_, AuditEntry>(sqlx::AssertSqlSafe(sql.as_str()));
    for a in &action_parts { q = q.bind(*a); }
    if !actor.is_empty()  { q = q.bind(actor); }
    if !search.is_empty() {
        let pat = format!("%{}%", search);
        q = q.bind(pat.clone()).bind(pat.clone()).bind(pat.clone()).bind(pat.clone()).bind(pat);
    }
    q = q.bind(limit).bind(offset);

    let rows = q.fetch_all(pool).await.context("Failed to list audit log")?;
    Ok(rows)
}

pub async fn audit_count(
    pool: &Pool<Sqlite>,
    action: &str,
    actor: &str,
    search: &str,
) -> Result<i64> {
    let action_parts: Vec<&str> = action.split(',').filter(|s| !s.is_empty()).collect();
    let mut sql = String::from("SELECT COUNT(*) FROM audit_log WHERE 1=1");
    if !action_parts.is_empty() {
        let ph = action_parts.iter().map(|_| "?").collect::<Vec<_>>().join(", ");
        sql.push_str(&format!(" AND action IN ({})", ph));
    }
    if !actor.is_empty()  { sql.push_str(" AND actor = ?"); }
    if !search.is_empty() { sql.push_str(" AND (target LIKE ? OR detail LIKE ? OR actor LIKE ? OR action LIKE ? OR ip LIKE ?)"); }

    let mut q = sqlx::query_scalar::<_, i64>(sqlx::AssertSqlSafe(sql.as_str()));
    for a in &action_parts { q = q.bind(*a); }
    if !actor.is_empty()  { q = q.bind(actor); }
    if !search.is_empty() {
        let pat = format!("%{}%", search);
        q = q.bind(pat.clone()).bind(pat.clone()).bind(pat.clone()).bind(pat.clone()).bind(pat);
    }

    let count = q.fetch_one(pool).await.context("Failed to count audit log")?;
    Ok(count)
}

pub async fn audit_list_for_server(
    pool: &Pool<Sqlite>,
    server_db_id: i64,
    limit: i64,
    offset: i64,
    action: &str,
    actor: &str,
    search: &str,
) -> Result<Vec<AuditEntry>> {
    let action_parts: Vec<&str> = action.split(',').filter(|s| !s.is_empty()).collect();
    let marker_hash = format!("#{}", server_db_id);
    let marker_server = format!("server={}", server_db_id);

    let mut sql = String::from(
        "SELECT id, actor, action, target, detail, ip, COALESCE(user_agent,'') as user_agent, created_at \
         FROM audit_log \
         WHERE (target = ? OR detail = ? OR target = ? OR detail = ?)",
    );
    if !action_parts.is_empty() {
        let ph = action_parts.iter().map(|_| "?").collect::<Vec<_>>().join(", ");
        sql.push_str(&format!(" AND action IN ({})", ph));
    }
    if !actor.is_empty() {
        sql.push_str(" AND actor = ?");
    }
    if !search.is_empty() {
        sql.push_str(" AND (target LIKE ? OR detail LIKE ? OR actor LIKE ? OR action LIKE ? OR ip LIKE ?)");
    }
    sql.push_str(" ORDER BY id DESC LIMIT ? OFFSET ?");

    let mut q = sqlx::query_as::<_, AuditEntry>(sqlx::AssertSqlSafe(sql.as_str()))
        .bind(&marker_hash)
        .bind(&marker_hash)
        .bind(&marker_server)
        .bind(&marker_server);
    for a in &action_parts {
        q = q.bind(*a);
    }
    if !actor.is_empty() {
        q = q.bind(actor);
    }
    if !search.is_empty() {
        let pat = format!("%{}%", search);
        q = q
            .bind(pat.clone())
            .bind(pat.clone())
            .bind(pat.clone())
            .bind(pat.clone())
            .bind(pat);
    }
    q = q.bind(limit).bind(offset);

    let rows = q
        .fetch_all(pool)
        .await
        .context("Failed to list server audit log")?;
    Ok(rows)
}

pub async fn audit_list_all_for_server(
    pool: &Pool<Sqlite>,
    server_db_id: i64,
    action: &str,
    actor: &str,
    search: &str,
) -> Result<Vec<AuditEntry>> {
    let action_parts: Vec<&str> = action.split(',').filter(|s| !s.is_empty()).collect();
    let marker_hash = format!("#{}", server_db_id);
    let marker_server = format!("server={}", server_db_id);

    let mut sql = String::from(
        "SELECT id, actor, action, target, detail, ip, COALESCE(user_agent,'') as user_agent, created_at \
         FROM audit_log \
         WHERE (target = ? OR detail = ? OR target = ? OR detail = ?)",
    );
    if !action_parts.is_empty() {
        let ph = action_parts.iter().map(|_| "?").collect::<Vec<_>>().join(", ");
        sql.push_str(&format!(" AND action IN ({})", ph));
    }
    if !actor.is_empty() {
        sql.push_str(" AND actor = ?");
    }
    if !search.is_empty() {
        sql.push_str(" AND (target LIKE ? OR detail LIKE ? OR actor LIKE ? OR action LIKE ? OR ip LIKE ?)");
    }
    sql.push_str(" ORDER BY id ASC");

    let mut q = sqlx::query_as::<_, AuditEntry>(sqlx::AssertSqlSafe(sql.as_str()))
        .bind(&marker_hash)
        .bind(&marker_hash)
        .bind(&marker_server)
        .bind(&marker_server);
    for a in &action_parts {
        q = q.bind(*a);
    }
    if !actor.is_empty() {
        q = q.bind(actor);
    }
    if !search.is_empty() {
        let pat = format!("%{}%", search);
        q = q
            .bind(pat.clone())
            .bind(pat.clone())
            .bind(pat.clone())
            .bind(pat.clone())
            .bind(pat);
    }

    let rows = q
        .fetch_all(pool)
        .await
        .context("Failed to list full server audit log")?;
    Ok(rows)
}

pub async fn audit_count_for_server(
    pool: &Pool<Sqlite>,
    server_db_id: i64,
    action: &str,
    actor: &str,
    search: &str,
) -> Result<i64> {
    let action_parts: Vec<&str> = action.split(',').filter(|s| !s.is_empty()).collect();
    let marker_hash = format!("#{}", server_db_id);
    let marker_server = format!("server={}", server_db_id);

    let mut sql = String::from(
        "SELECT COUNT(*) \
         FROM audit_log \
         WHERE (target = ? OR detail = ? OR target = ? OR detail = ?)",
    );
    if !action_parts.is_empty() {
        let ph = action_parts.iter().map(|_| "?").collect::<Vec<_>>().join(", ");
        sql.push_str(&format!(" AND action IN ({})", ph));
    }
    if !actor.is_empty() {
        sql.push_str(" AND actor = ?");
    }
    if !search.is_empty() {
        sql.push_str(" AND (target LIKE ? OR detail LIKE ? OR actor LIKE ? OR action LIKE ? OR ip LIKE ?)");
    }

    let mut q = sqlx::query_scalar::<_, i64>(sqlx::AssertSqlSafe(sql.as_str()))
        .bind(&marker_hash)
        .bind(&marker_hash)
        .bind(&marker_server)
        .bind(&marker_server);
    for a in &action_parts {
        q = q.bind(*a);
    }
    if !actor.is_empty() {
        q = q.bind(actor);
    }
    if !search.is_empty() {
        let pat = format!("%{}%", search);
        q = q
            .bind(pat.clone())
            .bind(pat.clone())
            .bind(pat.clone())
            .bind(pat.clone())
            .bind(pat);
    }

    let count = q
        .fetch_one(pool)
        .await
        .context("Failed to count server audit log")?;
    Ok(count)
}
