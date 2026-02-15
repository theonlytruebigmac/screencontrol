//! User management API — list, create, update, deactivate.

use std::sync::Arc;

use axum::{
    extract::{Path, State},
    routing::{delete, get, patch, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::api::middleware::AuthUser;
use crate::AppState;
use sc_common::{AppError, AppResult};

pub fn router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/", get(list_users).post(create_user))
        .route("/{id}", patch(update_user).delete(delete_user))
        .with_state(state)
}

// ─── Types ───────────────────────────────────────────────────

#[derive(Debug, Serialize, sqlx::FromRow)]
struct UserRow {
    id: Uuid,
    email: String,
    display_name: Option<String>,
    role: String,
    is_active: bool,
    last_login: Option<chrono::DateTime<chrono::Utc>>,
    created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Deserialize)]
struct CreateUserRequest {
    email: String,
    password: String,
    display_name: Option<String>,
    role: Option<String>,
}

#[derive(Debug, Deserialize)]
struct UpdateUserRequest {
    display_name: Option<String>,
    role: Option<String>,
    is_active: Option<bool>,
}

// ─── Handlers ────────────────────────────────────────────────

async fn list_users(
    auth: AuthUser,
    State(state): State<Arc<AppState>>,
) -> AppResult<Json<Vec<UserRow>>> {
    // Only admins can list users
    if auth.0.role != "admin" {
        return Err(AppError::Unauthorized("Admin access required".into()));
    }

    let users: Vec<UserRow> = sqlx::query_as(
        r#"SELECT id, email, display_name, role, is_active, last_login, created_at
           FROM users
           WHERE tenant_id = $1
           ORDER BY created_at ASC"#,
    )
    .bind(Uuid::parse_str(&auth.0.tenant_id).unwrap_or_default())
    .fetch_all(&state.db)
    .await?;

    Ok(Json(users))
}

async fn create_user(
    auth: AuthUser,
    State(state): State<Arc<AppState>>,
    Json(payload): Json<CreateUserRequest>,
) -> AppResult<Json<UserRow>> {
    if auth.0.role != "admin" {
        return Err(AppError::Unauthorized("Admin access required".into()));
    }

    let tenant_id = Uuid::parse_str(&auth.0.tenant_id)
        .map_err(|_| AppError::Internal(anyhow::anyhow!("Invalid tenant ID")))?;

    // Hash the password with Argon2
    let salt =
        argon2::password_hash::SaltString::generate(&mut argon2::password_hash::rand_core::OsRng);
    let password_hash = argon2::PasswordHasher::hash_password(
        &argon2::Argon2::default(),
        payload.password.as_bytes(),
        &salt,
    )
    .map_err(|e| AppError::Internal(anyhow::anyhow!("Failed to hash password: {}", e)))?
    .to_string();

    let role = payload.role.unwrap_or_else(|| "technician".to_string());

    let user: UserRow = sqlx::query_as(
        r#"INSERT INTO users (tenant_id, email, password_hash, display_name, role)
           VALUES ($1, $2, $3, $4, $5)
           RETURNING id, email, display_name, role, is_active, last_login, created_at"#,
    )
    .bind(tenant_id)
    .bind(&payload.email)
    .bind(&password_hash)
    .bind(&payload.display_name)
    .bind(&role)
    .fetch_one(&state.db)
    .await
    .map_err(|e| {
        if let sqlx::Error::Database(ref db_err) = e {
            if db_err.constraint() == Some("users_tenant_id_email_key") {
                return AppError::BadRequest("A user with this email already exists".into());
            }
        }
        AppError::from(e)
    })?;

    // Audit
    let _ = crate::api::audit::record(
        &state.db,
        Uuid::parse_str(&auth.0.sub).ok(),
        "user.create",
        Some("user"),
        Some(user.id),
        None,
        Some(serde_json::json!({ "email": user.email, "role": role })),
    )
    .await;

    Ok(Json(user))
}

async fn update_user(
    auth: AuthUser,
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
    Json(payload): Json<UpdateUserRequest>,
) -> AppResult<Json<UserRow>> {
    if auth.0.role != "admin" {
        return Err(AppError::Unauthorized("Admin access required".into()));
    }

    let user: UserRow = sqlx::query_as(
        r#"UPDATE users SET
               display_name = COALESCE($2, display_name),
               role = COALESCE($3, role),
               is_active = COALESCE($4, is_active),
               updated_at = NOW()
           WHERE id = $1
           RETURNING id, email, display_name, role, is_active, last_login, created_at"#,
    )
    .bind(id)
    .bind(payload.display_name)
    .bind(payload.role)
    .bind(payload.is_active)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound("User not found".into()))?;

    Ok(Json(user))
}

async fn delete_user(
    auth: AuthUser,
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<serde_json::Value>> {
    if auth.0.role != "admin" {
        return Err(AppError::Unauthorized("Admin access required".into()));
    }

    // Soft-delete: deactivate instead of removing
    let result =
        sqlx::query("UPDATE users SET is_active = false, updated_at = NOW() WHERE id = $1")
            .bind(id)
            .execute(&state.db)
            .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound("User not found".into()));
    }

    // Audit
    let _ = crate::api::audit::record(
        &state.db,
        Uuid::parse_str(&auth.0.sub).ok(),
        "user.delete",
        Some("user"),
        Some(id),
        None,
        None,
    )
    .await;

    Ok(Json(serde_json::json!({ "deleted": true })))
}
