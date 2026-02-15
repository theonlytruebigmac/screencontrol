use serde::Deserialize;

/// Top-level application configuration.
/// Loaded from environment variables and/or config files.
#[derive(Debug, Clone, Deserialize)]
pub struct AppConfig {
    /// Server settings
    pub server: ServerConfig,
    /// Database settings
    pub database: DatabaseConfig,
    /// Redis settings
    pub redis: RedisConfig,
    /// JWT settings
    pub auth: AuthConfig,
    /// S3 / MinIO settings
    #[serde(default)]
    pub s3: S3Config,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ServerConfig {
    /// Host to bind to (default: 0.0.0.0)
    #[serde(default = "default_host")]
    pub host: String,
    /// HTTP API port (default: 8080)
    #[serde(default = "default_api_port")]
    pub api_port: u16,
    /// Relay port (default: 8041)
    #[serde(default = "default_relay_port")]
    pub relay_port: u16,
    /// Log level (default: info)
    #[serde(default = "default_log_level")]
    pub log_level: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DatabaseConfig {
    /// PostgreSQL connection URL
    pub url: String,
    /// Max connections in pool
    #[serde(default = "default_max_connections")]
    pub max_connections: u32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RedisConfig {
    /// Redis connection URL
    pub url: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AuthConfig {
    /// JWT signing secret
    pub jwt_secret: String,
    /// Access token TTL in seconds (default: 3600 = 1 hour)
    #[serde(default = "default_access_ttl")]
    pub access_token_ttl_secs: u64,
    /// Refresh token TTL in seconds (default: 604800 = 7 days)
    #[serde(default = "default_refresh_ttl")]
    pub refresh_token_ttl_secs: u64,
}

impl AppConfig {
    /// Load config from environment variables and optional config file.
    pub fn load() -> Result<Self, config::ConfigError> {
        let cfg = config::Config::builder()
            .add_source(
                config::Environment::default()
                    .prefix("SC")
                    .separator("__")
                    .try_parsing(true),
            )
            .build()?;

        cfg.try_deserialize()
    }
}

fn default_host() -> String {
    "0.0.0.0".to_string()
}
fn default_api_port() -> u16 {
    8080
}
fn default_relay_port() -> u16 {
    8041
}
fn default_log_level() -> String {
    "info".to_string()
}
fn default_max_connections() -> u32 {
    10
}
fn default_access_ttl() -> u64 {
    3600
}
fn default_refresh_ttl() -> u64 {
    604800
}

#[derive(Debug, Clone, Deserialize)]
pub struct S3Config {
    /// S3-compatible endpoint URL (MinIO) — used for server-to-server communication
    #[serde(default = "default_s3_endpoint")]
    pub endpoint: String,
    /// Public S3 endpoint for browser-facing pre-signed URLs.
    /// Defaults to the same as `endpoint` if not set.
    #[serde(default)]
    pub public_endpoint: Option<String>,
    /// Default bucket for recordings
    #[serde(default = "default_s3_bucket")]
    pub bucket: String,
    /// AWS region
    #[serde(default = "default_s3_region")]
    pub region: String,
    /// Access key
    #[serde(default = "default_s3_access_key")]
    pub access_key: String,
    /// Secret key
    #[serde(default = "default_s3_secret_key")]
    pub secret_key: String,
}

impl S3Config {
    /// Returns the public endpoint for browser-facing URLs,
    /// falling back to the internal endpoint if not configured.
    pub fn public_endpoint(&self) -> &str {
        self.public_endpoint.as_deref().unwrap_or(&self.endpoint)
    }
}

impl Default for S3Config {
    fn default() -> Self {
        Self {
            endpoint: default_s3_endpoint(),
            public_endpoint: None,
            bucket: default_s3_bucket(),
            region: default_s3_region(),
            access_key: default_s3_access_key(),
            secret_key: default_s3_secret_key(),
        }
    }
}

fn default_s3_endpoint() -> String {
    "http://localhost:9000".to_string()
}
fn default_s3_bucket() -> String {
    "sc-recordings".to_string()
}
fn default_s3_region() -> String {
    "us-east-1".to_string()
}
fn default_s3_access_key() -> String {
    "screencontrol".to_string()
}
fn default_s3_secret_key() -> String {
    "screencontrol123".to_string()
}
