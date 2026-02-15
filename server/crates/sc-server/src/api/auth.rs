//! Authentication API — login, JWT issue/refresh, registration.

use std::sync::Arc;

use axum::{
    extract::{ConnectInfo, State},
    routing::{get, patch, post},
    Json, Router,
};
use chrono::Utc;
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::api::middleware::AuthUser;
use crate::services::redis_services;
use crate::AppState;
use sc_common::{AppError, AppResult};

pub fn router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/login", post(login))
        .route("/logout", post(logout))
        .route("/refresh", post(refresh_token))
        .route("/me", get(get_me))
        .route("/profile", patch(update_profile))
        .route("/change-password", post(change_password))
        .with_state(state)
}

// ─── Request / Response types ────────────────────────────────

#[derive(Debug, Deserialize)]
struct LoginRequest {
    email: String,
    password: String,
}

#[derive(Debug, Serialize)]
struct TokenResponse {
    access_token: String,
    refresh_token: String,
    token_type: String,
    expires_in: u64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Claims {
    pub sub: String,
    pub email: String,
    pub role: String,
    pub tenant_id: String,
    pub exp: usize,
    pub iat: usize,
    pub jti: String,
}

#[derive(Debug, Deserialize)]
struct RefreshRequest {
    refresh_token: String,
}

// ─── Handlers ────────────────────────────────────────────────

async fn login(
    State(state): State<Arc<AppState>>,
    axum::extract::ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    Json(payload): Json<LoginRequest>,
) -> AppResult<Json<TokenResponse>> {
    // ── Rate limiting (5 attempts/minute per IP) ─────────
    let ip = addr.ip().to_string();
    let rl = redis_services::check_rate_limit(&mut state.redis.clone(), &ip, "login", 5, 60).await;
    if !rl.allowed {
        return Err(AppError::BadRequest(
            "Too many login attempts. Please try again later.".into(),
        ));
    }

    // Look up user by email
    let user: UserRow = sqlx::query_as(
        "SELECT id, email, password_hash, role, tenant_id FROM users WHERE email = $1",
    )
    .bind(&payload.email)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::Unauthorized("Invalid credentials".into()))?;

    // Verify password
    let parsed_hash = argon2::PasswordHash::new(&user.password_hash)
        .map_err(|_| AppError::Internal(anyhow::anyhow!("Invalid password hash in database")))?;

    argon2::PasswordVerifier::verify_password(
        &argon2::Argon2::default(),
        payload.password.as_bytes(),
        &parsed_hash,
    )
    .map_err(|_| AppError::Unauthorized("Invalid credentials".into()))?;

    // Generate tokens
    let tokens = generate_tokens(&state.config.auth, &user)?;

    // Record audit entry for login
    let _ = crate::api::audit::record(
        &state.db,
        Some(user.id),
        "user.login",
        Some("user"),
        Some(user.id),
        None,
        Some(serde_json::json!({ "email": user.email })),
    )
    .await;

    // Broadcast login event to UI subscribers
    state.registry.broadcast_event(&serde_json::json!({
        "type": "user.login",
        "user_id": user.id.to_string(),
        "email": user.email
    }));

    Ok(Json(tokens))
}

// ─── Logout ──────────────────────────────────────────────────

async fn logout(
    auth: AuthUser,
    State(state): State<Arc<AppState>>,
) -> AppResult<Json<serde_json::Value>> {
    // Calculate remaining TTL for the token
    let now = Utc::now().timestamp() as u64;
    let exp = auth.0.exp as u64;
    let ttl = exp.saturating_sub(now).max(1);

    // Blacklist both the access token's JTI
    let _ = redis_services::blacklist_token(&mut state.redis.clone(), &auth.0.jti, ttl).await;

    // Record audit
    let _ = crate::api::audit::record(
        &state.db,
        Some(auth.user_id()),
        "user.logout",
        Some("user"),
        Some(auth.user_id()),
        None,
        None,
    )
    .await;

    tracing::info!(user_id = %auth.user_id(), "User logged out");
    Ok(Json(serde_json::json!({ "logged_out": true })))
}

async fn refresh_token(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<RefreshRequest>,
) -> AppResult<Json<TokenResponse>> {
    let token_data = decode::<Claims>(
        &payload.refresh_token,
        &DecodingKey::from_secret(state.config.auth.jwt_secret.as_bytes()),
        &Validation::default(),
    )
    .map_err(|_| AppError::Unauthorized("Invalid refresh token".into()))?;

    let user_id = Uuid::parse_str(&token_data.claims.sub)
        .map_err(|_| AppError::BadRequest("Invalid user ID".into()))?;

    let user: UserRow =
        sqlx::query_as("SELECT id, email, password_hash, role, tenant_id FROM users WHERE id = $1")
            .bind(user_id)
            .fetch_optional(&state.db)
            .await?
            .ok_or_else(|| AppError::Unauthorized("User not found".into()))?;

    let tokens = generate_tokens(&state.config.auth, &user)?;
    Ok(Json(tokens))
}

// ─── Helpers ─────────────────────────────────────────────────

#[derive(Debug, Serialize)]
struct MeResponse {
    id: String,
    email: String,
    display_name: String,
    role: String,
}

async fn get_me(auth: AuthUser, State(state): State<Arc<AppState>>) -> AppResult<Json<MeResponse>> {
    let user_id = auth.user_id();

    let row: Option<(String, String)> = sqlx::query_as(
        "SELECT email, COALESCE(display_name, email) as display_name FROM users WHERE id = $1",
    )
    .bind(user_id)
    .fetch_optional(&state.db)
    .await?;

    match row {
        Some((email, display_name)) => Ok(Json(MeResponse {
            id: user_id.to_string(),
            email,
            display_name,
            role: auth.role().to_string(),
        })),
        None => Err(AppError::NotFound("User not found".into())),
    }
}

#[derive(sqlx::FromRow)]
struct UserRow {
    id: Uuid,
    email: String,
    password_hash: String,
    role: String,
    tenant_id: Uuid,
}

fn generate_tokens(
    auth_config: &sc_common::config::AuthConfig,
    user: &UserRow,
) -> AppResult<TokenResponse> {
    let now = Utc::now().timestamp() as usize;

    let access_claims = Claims {
        sub: user.id.to_string(),
        email: user.email.clone(),
        role: user.role.clone(),
        tenant_id: user.tenant_id.to_string(),
        exp: now + auth_config.access_token_ttl_secs as usize,
        iat: now,
        jti: Uuid::new_v4().to_string(),
    };

    let refresh_claims = Claims {
        exp: now + auth_config.refresh_token_ttl_secs as usize,
        jti: Uuid::new_v4().to_string(),
        ..access_claims.clone()
    };

    let key = EncodingKey::from_secret(auth_config.jwt_secret.as_bytes());

    let access_token = encode(&Header::default(), &access_claims, &key)
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Token encoding error: {}", e)))?;

    let refresh_token = encode(&Header::default(), &refresh_claims, &key)
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Token encoding error: {}", e)))?;

    Ok(TokenResponse {
        access_token,
        refresh_token,
        token_type: "Bearer".into(),
        expires_in: auth_config.access_token_ttl_secs,
    })
}

// ─── Self-service profile endpoints ─────────────────────────

#[derive(Debug, Deserialize)]
struct UpdateProfileRequest {
    display_name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ChangePasswordRequest {
    current_password: String,
    new_password: String,
}

async fn update_profile(
    auth: AuthUser,
    State(state): State<Arc<AppState>>,
    Json(payload): Json<UpdateProfileRequest>,
) -> AppResult<Json<MeResponse>> {
    let user_id = auth.user_id();

    sqlx::query(
        "UPDATE users SET display_name = COALESCE($2, display_name), updated_at = NOW() WHERE id = $1",
    )
    .bind(user_id)
    .bind(&payload.display_name)
    .execute(&state.db)
    .await?;

    // Return updated profile
    let row: (String, String) = sqlx::query_as(
        "SELECT email, COALESCE(display_name, email) as display_name FROM users WHERE id = $1",
    )
    .bind(user_id)
    .fetch_one(&state.db)
    .await?;

    Ok(Json(MeResponse {
        id: user_id.to_string(),
        email: row.0,
        display_name: row.1,
        role: auth.role().to_string(),
    }))
}

async fn change_password(
    auth: AuthUser,
    State(state): State<Arc<AppState>>,
    Json(payload): Json<ChangePasswordRequest>,
) -> AppResult<Json<serde_json::Value>> {
    let user_id = auth.user_id();

    if payload.new_password.len() < 8 {
        return Err(AppError::BadRequest(
            "New password must be at least 8 characters".into(),
        ));
    }

    // Fetch current hash
    let current_hash: String = sqlx::query_scalar("SELECT password_hash FROM users WHERE id = $1")
        .bind(user_id)
        .fetch_one(&state.db)
        .await?;

    // Verify current password
    let parsed = argon2::PasswordHash::new(&current_hash)
        .map_err(|_| AppError::Internal(anyhow::anyhow!("Invalid password hash in database")))?;

    argon2::PasswordVerifier::verify_password(
        &argon2::Argon2::default(),
        payload.current_password.as_bytes(),
        &parsed,
    )
    .map_err(|_| AppError::BadRequest("Current password is incorrect".into()))?;

    // Hash new password
    let salt =
        argon2::password_hash::SaltString::generate(&mut argon2::password_hash::rand_core::OsRng);
    let new_hash = argon2::PasswordHasher::hash_password(
        &argon2::Argon2::default(),
        payload.new_password.as_bytes(),
        &salt,
    )
    .map_err(|e| AppError::Internal(anyhow::anyhow!("Failed to hash password: {}", e)))?
    .to_string();

    sqlx::query("UPDATE users SET password_hash = $2, updated_at = NOW() WHERE id = $1")
        .bind(user_id)
        .bind(&new_hash)
        .execute(&state.db)
        .await?;

    Ok(Json(serde_json::json!({ "changed": true })))
}
