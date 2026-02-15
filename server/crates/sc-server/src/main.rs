#![allow(unused)]
//! # ScreenControl Server
//!
//! REST API + WebSocket gateway for session management,
//! agent orchestration, and authentication.

mod api;
mod db;
mod services;
mod workers;
mod ws;

use std::net::SocketAddr;
use std::sync::Arc;

use axum::Router;
use sqlx::postgres::PgPoolOptions;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;
use tracing_subscriber::EnvFilter;

/// Shared application state available to all handlers.
pub struct AppState {
    pub db: sqlx::PgPool,
    pub redis: redis::aio::ConnectionManager,
    pub s3: aws_sdk_s3::Client,
    /// S3 client configured with the public endpoint for browser-facing pre-signed URLs.
    pub s3_public: aws_sdk_s3::Client,
    pub config: sc_common::AppConfig,
    pub registry: ws::registry::ConnectionRegistry,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Load .env
    dotenvy::dotenv().ok();

    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .json()
        .init();

    tracing::info!("Starting ScreenControl server...");

    // Load configuration
    let config = sc_common::AppConfig::load().expect("Failed to load configuration");

    // Connect to PostgreSQL
    let db = PgPoolOptions::new()
        .max_connections(config.database.max_connections)
        .connect(&config.database.url)
        .await?;

    // Run migrations
    let migrator = sqlx::migrate::Migrator::new(std::path::Path::new("./migrations")).await?;
    migrator.run(&db).await?;
    tracing::info!("Database migrations applied");

    // Connect to Redis
    let redis_client = redis::Client::open(config.redis.url.as_str())?;
    let redis = redis_client.get_connection_manager().await?;
    tracing::info!("Connected to Redis");

    // Initialize S3/MinIO client
    let s3 = services::s3::init_client(&config.s3).await;
    let s3_public = services::s3::init_public_client(&config.s3).await;
    services::s3::ensure_bucket(&s3, &config.s3.bucket).await?;
    services::s3::ensure_bucket(&s3, "sc-installers").await?;
    tracing::info!("S3/MinIO connected, buckets ready");

    // Build shared state
    let state = Arc::new(AppState {
        db,
        redis,
        s3,
        s3_public,
        config: config.clone(),
        registry: ws::registry::ConnectionRegistry::new(),
    });

    // Start Redis pub/sub subscribers for multi-instance routing
    let _subscriber_handles = services::pubsub::start_all_subscribers(state.clone());
    tracing::info!("Redis pub/sub subscribers started");

    // Start background workers
    let _worker_handles = workers::start_all_workers(state.clone());
    tracing::info!("Background workers started");

    // Build router
    let app = Router::new()
        .nest("/api", api::router(state.clone()))
        .nest("/ws", ws::router(state.clone()))
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http());

    // Start server
    let addr = SocketAddr::from(([0, 0, 0, 0], config.server.api_port));
    tracing::info!("Listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal())
    .await?;

    Ok(())
}

async fn shutdown_signal() {
    tokio::signal::ctrl_c()
        .await
        .expect("Failed to install CTRL+C handler");
    tracing::info!("Shutdown signal received");
}
