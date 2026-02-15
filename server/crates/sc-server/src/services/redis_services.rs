//! Redis-backed services: JWT blacklist, rate limiting, and stats caching.

use redis::AsyncCommands;
use serde::Serialize;

/// Redis key prefix for blacklisted JWT tokens.
const BLACKLIST_PREFIX: &str = "sc:blacklist:";

/// Redis key prefix for rate limiting.
const RATE_PREFIX: &str = "sc:rate:";

/// Redis key prefix for cached values.
const CACHE_PREFIX: &str = "sc:cache:";

// ─── JWT Token Blacklist ─────────────────────────────────────

/// Add a JWT token ID to the blacklist with automatic expiry.
///
/// The TTL should match the remaining lifetime of the token so the
/// blacklist entry disappears once the token would have expired anyway.
pub async fn blacklist_token(
    redis: &mut redis::aio::ConnectionManager,
    jti: &str,
    ttl_secs: u64,
) -> anyhow::Result<()> {
    let key = format!("{}{}", BLACKLIST_PREFIX, jti);
    redis.set_ex::<_, _, ()>(&key, 1u8, ttl_secs).await?;
    tracing::info!(jti, ttl_secs, "Token blacklisted");
    Ok(())
}

/// Check if a token ID has been blacklisted.
pub async fn is_token_blacklisted(redis: &mut redis::aio::ConnectionManager, jti: &str) -> bool {
    let key = format!("{}{}", BLACKLIST_PREFIX, jti);
    redis.exists::<_, bool>(&key).await.unwrap_or(false)
}

// ─── Rate Limiting ───────────────────────────────────────────

/// Result of a rate limit check.
pub struct RateLimitResult {
    pub allowed: bool,
    pub remaining: u64,
    pub limit: u64,
}

/// Check and increment a rate limit counter using a sliding window.
///
/// - `identifier`: usually the client IP address
/// - `endpoint`: the endpoint being rate-limited (e.g. "login")
/// - `max_requests`: maximum allowed requests in the window
/// - `window_secs`: window size in seconds
pub async fn check_rate_limit(
    redis: &mut redis::aio::ConnectionManager,
    identifier: &str,
    endpoint: &str,
    max_requests: u64,
    window_secs: u64,
) -> RateLimitResult {
    let key = format!("{}{}:{}", RATE_PREFIX, endpoint, identifier);

    // INCR + conditional EXPIRE (atomic-ish via pipeline)
    let count: u64 = match redis.incr::<_, _, u64>(&key, 1u64).await {
        Ok(c) => c,
        Err(_) => {
            // On Redis error, allow the request (fail-open)
            return RateLimitResult {
                allowed: true,
                remaining: max_requests,
                limit: max_requests,
            };
        }
    };

    // Set expiry only on first increment
    if count == 1 {
        let _ = redis.expire::<_, ()>(&key, window_secs as i64).await;
    }

    let remaining = max_requests.saturating_sub(count);
    RateLimitResult {
        allowed: count <= max_requests,
        remaining,
        limit: max_requests,
    }
}

// ─── Stats Cache ─────────────────────────────────────────────

/// Try to get a cached value.
pub async fn cache_get(
    redis: &mut redis::aio::ConnectionManager,
    cache_key: &str,
) -> Option<String> {
    let key = format!("{}{}", CACHE_PREFIX, cache_key);
    redis.get::<_, Option<String>>(&key).await.ok().flatten()
}

/// Store a value in cache with TTL.
pub async fn cache_set<T: Serialize>(
    redis: &mut redis::aio::ConnectionManager,
    cache_key: &str,
    value: &T,
    ttl_secs: u64,
) {
    let key = format!("{}{}", CACHE_PREFIX, cache_key);
    if let Ok(json) = serde_json::to_string(value) {
        let _ = redis.set_ex::<_, _, ()>(&key, &json, ttl_secs).await;
    }
}
