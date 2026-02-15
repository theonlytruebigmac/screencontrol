//! Dashboard statistics API — aggregated counts for the dashboard.

use std::sync::Arc;
use std::time::Instant;

use axum::{extract::State, routing::get, Json, Router};
use chrono::Utc;
use redis::AsyncCommands;
use serde::{Deserialize, Serialize};

use crate::api::middleware::AuthUser;
use crate::services::redis_services;
use crate::AppState;
use sc_common::AppResult;

/// Capture server start time for uptime calculation.
static SERVER_START: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();

fn server_start() -> Instant {
    *SERVER_START.get_or_init(Instant::now)
}

pub fn router(state: Arc<AppState>) -> Router {
    // Initialize server start time on first call
    let _ = server_start();
    Router::new()
        .route("/", get(get_stats))
        .route("/system-health", get(get_system_health))
        .with_state(state)
}

// ─── Response type ───────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
struct StatsResponse {
    agents_total: i64,
    agents_online: i64,
    sessions_active: i64,
    sessions_today: i64,
    users_total: i64,
}

// ─── Handler ─────────────────────────────────────────────────

async fn get_stats(
    auth: AuthUser,
    State(state): State<Arc<AppState>>,
) -> AppResult<Json<StatsResponse>> {
    // Check cache first (30s TTL)
    let cache_key = format!("stats:{}", auth.tenant_id());
    if let Some(cached) = redis_services::cache_get(&mut state.redis.clone(), &cache_key).await {
        if let Ok(resp) = serde_json::from_str::<StatsResponse>(&cached) {
            return Ok(Json(resp));
        }
    }

    let today_start = Utc::now()
        .date_naive()
        .and_hms_opt(0, 0, 0)
        .unwrap()
        .and_utc();

    // Run all queries concurrently
    let (agents_total, agents_online_db, sessions_active, sessions_today, users_total) = tokio::try_join!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM agents").fetch_one(&state.db),
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM agents WHERE status = 'online'")
            .fetch_one(&state.db),
        sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM sessions WHERE status IN ('active', 'pending')"
        )
        .fetch_one(&state.db),
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM sessions WHERE started_at >= $1")
            .bind(today_start)
            .fetch_one(&state.db),
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM users").fetch_one(&state.db),
    )?;

    // Use the live registry count as a more accurate online count
    let agents_online_live = state.registry.online_agent_count() as i64;
    let agents_online = agents_online_live.max(agents_online_db);

    let resp = StatsResponse {
        agents_total,
        agents_online,
        sessions_active,
        sessions_today,
        users_total,
    };

    // Write through to cache (30s)
    redis_services::cache_set(&mut state.redis.clone(), &cache_key, &resp, 30).await;

    Ok(Json(resp))
}

// ─── System Health ───────────────────────────────────────────

#[derive(Debug, Serialize)]
struct ComponentHealth {
    name: String,
    status: String,
    latency_ms: f64,
    version: Option<String>,
}

#[derive(Debug, Serialize)]
struct ResourceInfo {
    label: String,
    value: f64,
    max: f64,
    unit: String,
}

#[derive(Debug, Serialize)]
struct ServerInfo {
    version: String,
    rust_version: String,
    os: String,
    hostname: String,
    uptime_seconds: u64,
}

#[derive(Debug, Serialize)]
struct SystemHealthResponse {
    server: ServerInfo,
    components: Vec<ComponentHealth>,
    resources: Vec<ResourceInfo>,
}

async fn get_system_health(
    _auth: AuthUser,
    State(state): State<Arc<AppState>>,
) -> AppResult<Json<SystemHealthResponse>> {
    // ── Server info ──
    let uptime = server_start().elapsed().as_secs();
    let hostname = std::fs::read_to_string("/etc/hostname")
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|_| "unknown".into());

    let server = ServerInfo {
        version: env!("CARGO_PKG_VERSION").to_string(),
        rust_version: "rustc (stable)".to_string(),
        os: format!("{}/{}", std::env::consts::OS, std::env::consts::ARCH),
        hostname,
        uptime_seconds: uptime,
    };

    // ── PostgreSQL health ──
    let pg_start = Instant::now();
    let pg_status = match sqlx::query_scalar::<_, i32>("SELECT 1")
        .fetch_one(&state.db)
        .await
    {
        Ok(_) => "healthy",
        Err(_) => "down",
    };
    let pg_latency = pg_start.elapsed().as_secs_f64() * 1000.0;

    // Get PG version
    let pg_version = sqlx::query_scalar::<_, String>("SHOW server_version")
        .fetch_optional(&state.db)
        .await
        .ok()
        .flatten();

    // ── Redis health ──
    let redis_start = Instant::now();
    let mut redis_conn = state.redis.clone();
    let redis_status = match redis::cmd("PING")
        .query_async::<String>(&mut redis_conn)
        .await
    {
        Ok(_) => "healthy",
        Err(_) => "down",
    };
    let redis_latency = redis_start.elapsed().as_secs_f64() * 1000.0;

    // Redis version
    let redis_version: Option<String> = redis::cmd("INFO")
        .arg("server")
        .query_async::<String>(&mut redis_conn)
        .await
        .ok()
        .and_then(|info| {
            info.lines()
                .find(|l| l.starts_with("redis_version:"))
                .map(|l| l.trim_start_matches("redis_version:").trim().to_string())
        });

    let components = vec![
        ComponentHealth {
            name: "REST API".into(),
            status: "healthy".into(),
            latency_ms: 0.0,
            version: Some(env!("CARGO_PKG_VERSION").to_string()),
        },
        ComponentHealth {
            name: "PostgreSQL".into(),
            status: pg_status.into(),
            latency_ms: (pg_latency * 100.0).round() / 100.0,
            version: pg_version,
        },
        ComponentHealth {
            name: "Redis".into(),
            status: redis_status.into(),
            latency_ms: (redis_latency * 100.0).round() / 100.0,
            version: redis_version,
        },
        ComponentHealth {
            name: "WebSocket".into(),
            status: "healthy".into(),
            latency_ms: 0.0,
            version: None,
        },
    ];

    // ── DB pool resources ──
    let pool_size = state.db.size() as f64;
    let pool_idle = state.db.num_idle() as f64;
    let pool_max = state.db.options().get_max_connections() as f64;

    let resources = vec![
        ResourceInfo {
            label: "DB Pool Active".into(),
            value: pool_size - pool_idle,
            max: pool_max,
            unit: "connections".into(),
        },
        ResourceInfo {
            label: "DB Pool Idle".into(),
            value: pool_idle,
            max: pool_max,
            unit: "connections".into(),
        },
        ResourceInfo {
            label: "Online Agents".into(),
            value: state.registry.online_agent_count() as f64,
            max: 1000.0,
            unit: "agents".into(),
        },
    ];

    Ok(Json(SystemHealthResponse {
        server,
        components,
        resources,
    }))
}
