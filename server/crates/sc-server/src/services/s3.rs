//! S3 / MinIO object storage service.
//!
//! Provides helpers for initializing the S3 client, managing buckets,
//! and generating pre-signed upload/download URLs.

use aws_sdk_s3::{
    config::{Credentials, Region},
    presigning::PresigningConfig,
    Client,
};
use std::time::Duration;

/// Initialize an S3 client from our config (works with MinIO).
pub async fn init_client(s3_config: &sc_common::config::S3Config) -> Client {
    let creds = Credentials::new(
        &s3_config.access_key,
        &s3_config.secret_key,
        None,
        None,
        "sc-server",
    );

    let config = aws_sdk_s3::Config::builder()
        .behavior_version_latest()
        .endpoint_url(&s3_config.endpoint)
        .region(Region::new(s3_config.region.clone()))
        .credentials_provider(creds)
        .force_path_style(true) // Required for MinIO
        .build();

    Client::from_conf(config)
}

/// Ensure a bucket exists, creating it if necessary.
pub async fn ensure_bucket(client: &Client, bucket: &str) -> anyhow::Result<()> {
    match client.head_bucket().bucket(bucket).send().await {
        Ok(_) => {
            tracing::info!(bucket, "S3 bucket already exists");
        }
        Err(_) => {
            client
                .create_bucket()
                .bucket(bucket)
                .send()
                .await
                .map_err(|e| anyhow::anyhow!("Failed to create bucket '{}': {}", bucket, e))?;
            tracing::info!(bucket, "S3 bucket created");
        }
    }
    Ok(())
}

/// Generate a pre-signed URL for uploading (PUT) an object.
pub async fn presigned_upload_url(
    client: &Client,
    bucket: &str,
    key: &str,
    ttl_secs: u64,
) -> anyhow::Result<String> {
    let presigning = PresigningConfig::expires_in(Duration::from_secs(ttl_secs))
        .map_err(|e| anyhow::anyhow!("Presigning config error: {}", e))?;

    let resp = client
        .put_object()
        .bucket(bucket)
        .key(key)
        .presigned(presigning)
        .await
        .map_err(|e| anyhow::anyhow!("Presigned PUT error: {}", e))?;

    Ok(resp.uri().to_string())
}

/// Generate a pre-signed URL for downloading (GET) an object.
pub async fn presigned_download_url(
    client: &Client,
    bucket: &str,
    key: &str,
    ttl_secs: u64,
) -> anyhow::Result<String> {
    let presigning = PresigningConfig::expires_in(Duration::from_secs(ttl_secs))
        .map_err(|e| anyhow::anyhow!("Presigning config error: {}", e))?;

    let resp = client
        .get_object()
        .bucket(bucket)
        .key(key)
        .presigned(presigning)
        .await
        .map_err(|e| anyhow::anyhow!("Presigned GET error: {}", e))?;

    Ok(resp.uri().to_string())
}

/// Initialize an S3 client targeting the **public** endpoint.
/// Use this to generate pre-signed URLs that browsers can directly access.
pub async fn init_public_client(s3_config: &sc_common::config::S3Config) -> Client {
    let public_ep = s3_config.public_endpoint();
    let creds = Credentials::new(
        &s3_config.access_key,
        &s3_config.secret_key,
        None,
        None,
        "sc-server-public",
    );

    let config = aws_sdk_s3::Config::builder()
        .behavior_version_latest()
        .endpoint_url(public_ep)
        .region(Region::new(s3_config.region.clone()))
        .credentials_provider(creds)
        .force_path_style(true)
        .build();

    Client::from_conf(config)
}

/// Generate a browser-facing pre-signed download URL.
/// Uses the public-endpoint S3 client so the signature matches the host
/// the browser will actually connect to.
pub async fn presigned_download_url_public(
    public_client: &Client,
    bucket: &str,
    key: &str,
    ttl_secs: u64,
) -> anyhow::Result<String> {
    presigned_download_url(public_client, bucket, key, ttl_secs).await
}

/// Generate a browser-facing pre-signed upload URL.
/// Uses the public-endpoint S3 client so the signature matches the host
/// the browser will actually connect to.
pub async fn presigned_upload_url_public(
    public_client: &Client,
    bucket: &str,
    key: &str,
    ttl_secs: u64,
) -> anyhow::Result<String> {
    presigned_upload_url(public_client, bucket, key, ttl_secs).await
}

/// Delete an object from a bucket.
pub async fn delete_object(client: &Client, bucket: &str, key: &str) -> anyhow::Result<()> {
    client
        .delete_object()
        .bucket(bucket)
        .key(key)
        .send()
        .await
        .map_err(|e| anyhow::anyhow!("Delete object error: {}", e))?;
    Ok(())
}
