//! JWT authentication middleware — Axum `FromRequestParts` extractor.
//!
//! Extracts and validates JWT tokens from the `Authorization: Bearer <token>` header.
//! Protected routes use `AuthUser` as an extractor argument.
//! Revoked tokens are checked against the Redis blacklist.

use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use jsonwebtoken::{decode, DecodingKey, Validation};

use crate::api::auth::Claims;
use crate::services::redis_services;
use crate::AppState;
use sc_common::AppError;

/// Authenticated user extractor.
///
/// Use as a handler parameter to require authentication:
/// ```ignore
/// async fn protected(user: AuthUser) -> impl IntoResponse { ... }
/// ```
pub struct AuthUser(pub Claims);

impl AuthUser {
    /// User ID parsed from the JWT `sub` claim.
    pub fn user_id(&self) -> uuid::Uuid {
        uuid::Uuid::parse_str(&self.0.sub).unwrap_or_default()
    }

    /// Tenant ID parsed from the JWT claim.
    pub fn tenant_id(&self) -> uuid::Uuid {
        uuid::Uuid::parse_str(&self.0.tenant_id).unwrap_or_default()
    }

    /// User role from JWT.
    pub fn role(&self) -> &str {
        &self.0.role
    }
}

impl FromRequestParts<std::sync::Arc<AppState>> for AuthUser {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &std::sync::Arc<AppState>,
    ) -> Result<Self, Self::Rejection> {
        // Extract Authorization header
        let auth_header = parts
            .headers
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| AppError::Unauthorized("Missing authorization header".into()))?;

        // Strip "Bearer " prefix
        let token = auth_header
            .strip_prefix("Bearer ")
            .ok_or_else(|| AppError::Unauthorized("Invalid authorization format".into()))?;

        // Decode and validate JWT
        let token_data = decode::<Claims>(
            token,
            &DecodingKey::from_secret(state.config.auth.jwt_secret.as_bytes()),
            &Validation::default(),
        )
        .map_err(|e| {
            tracing::debug!("JWT validation failed: {}", e);
            AppError::Unauthorized("Invalid or expired token".into())
        })?;

        // Check if the token has been revoked (blacklisted)
        if redis_services::is_token_blacklisted(&mut state.redis.clone(), &token_data.claims.jti)
            .await
        {
            tracing::debug!(jti = %token_data.claims.jti, "Token is blacklisted");
            return Err(AppError::Unauthorized("Token has been revoked".into()));
        }

        Ok(AuthUser(token_data.claims))
    }
}
