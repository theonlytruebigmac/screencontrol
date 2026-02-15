//! File browser — handles file listing and transfer requests on the agent.
//!
//! Responds to `FileListRequest` with directory listings and
//! `FileTransferRequest` with acceptance/rejection + S3 pre-signed URLs.
//! Performs actual file transfers via HTTP PUT/GET against pre-signed URLs.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use prost::Message as ProstMessage;
use tokio::sync::mpsc;
use uuid::Uuid;

use sc_protocol::{
    envelope, Envelope, FileEntry, FileList, FileListRequest, FileTransferAck, FileTransferRequest,
};

/// Tracks a pending file transfer (URL received before or after request).
struct PendingTransfer {
    /// Pre-signed URL from the server (PUT URL for downloads, GET URL for uploads).
    presigned_url: String,
    /// The original request details (set when FileTransferRequest arrives).
    request: Option<FileTransferRequest>,
}

/// Handles file browsing and transfer operations.
pub struct FileBrowser {
    /// Allowed root directories (prevents path traversal).
    allowed_roots: Vec<PathBuf>,
    /// HTTP client for file transfers via pre-signed URLs.
    http_client: reqwest::Client,
    /// Pending transfers indexed by transfer_id.
    pending_transfers: HashMap<String, PendingTransfer>,
}

impl FileBrowser {
    pub fn new(_server_http_url: &str, _tenant_token: &str) -> Self {
        // Default: allow home directory and common data paths
        let mut roots = Vec::new();
        if let Some(home) = dirs::home_dir() {
            roots.push(home);
        }
        // Include platform-appropriate temp directories
        #[cfg(target_os = "linux")]
        roots.push(PathBuf::from("/tmp"));
        #[cfg(target_os = "macos")]
        roots.push(PathBuf::from("/private/tmp"));
        #[cfg(target_os = "windows")]
        if let Ok(tmp) = std::env::var("TEMP") {
            roots.push(PathBuf::from(tmp));
        }

        Self {
            allowed_roots: roots,
            http_client: reqwest::Client::new(),
            pending_transfers: HashMap::new(),
        }
    }

    /// Handle a `FileListRequest` — list directory contents.
    pub fn handle_file_list(
        &self,
        session_id: &str,
        req: &FileListRequest,
        tx: &mpsc::UnboundedSender<Vec<u8>>,
    ) {
        let path = if req.path.is_empty() {
            dirs::home_dir().unwrap_or_else(|| PathBuf::from("/"))
        } else {
            PathBuf::from(&req.path)
        };

        // Security: validate path is under an allowed root
        if !self.is_path_allowed(&path) {
            tracing::warn!("Path traversal blocked: {:?}", path);
            return;
        }

        let entries = match std::fs::read_dir(&path) {
            Ok(dir) => dir
                .filter_map(|entry| {
                    let entry = entry.ok()?;
                    let metadata = entry.metadata().ok()?;
                    let modified = metadata.modified().ok().and_then(|t| {
                        let duration = t.duration_since(std::time::UNIX_EPOCH).ok()?;
                        Some(prost_types::Timestamp {
                            seconds: duration.as_secs() as i64,
                            nanos: 0,
                        })
                    });

                    Some(FileEntry {
                        name: entry.file_name().to_string_lossy().to_string(),
                        is_directory: metadata.is_dir(),
                        size: metadata.len(),
                        modified,
                        permissions: format_permissions(&metadata),
                    })
                })
                .collect::<Vec<_>>(),
            Err(e) => {
                tracing::error!("Failed to read directory {:?}: {}", path, e);
                Vec::new()
            }
        };

        let envelope = Envelope {
            id: Uuid::new_v4().to_string(),
            session_id: session_id.to_string(),
            timestamp: None,
            payload: Some(envelope::Payload::FileList(FileList {
                path: path.to_string_lossy().to_string(),
                entries,
            })),
        };

        let mut buf = Vec::new();
        if envelope.encode(&mut buf).is_ok() {
            let _ = tx.send(buf);
        }
    }

    /// Handle a `FileTransferAck` from the server — stores the pre-signed URL.
    /// If a matching `FileTransferRequest` was already received, initiates the transfer.
    pub fn handle_transfer_ack(
        &mut self,
        session_id: &str,
        ack: &FileTransferAck,
        tx: &mpsc::UnboundedSender<Vec<u8>>,
    ) {
        if !ack.accepted {
            tracing::warn!(
                "Transfer {} rejected by server: {}",
                ack.transfer_id,
                ack.message
            );
            return;
        }

        tracing::info!("Received presigned URL for transfer {}", ack.transfer_id);

        if let Some(pending) = self.pending_transfers.get_mut(&ack.transfer_id) {
            // Request already arrived — store URL and execute
            pending.presigned_url = ack.presigned_url.clone();
            if let Some(req) = pending.request.take() {
                let url = pending.presigned_url.clone();
                self.execute_transfer(session_id, &req, &url, tx);
            }
        } else {
            // URL arrived first — store it for when the request arrives
            self.pending_transfers.insert(
                ack.transfer_id.clone(),
                PendingTransfer {
                    presigned_url: ack.presigned_url.clone(),
                    request: None,
                },
            );
        }
    }

    /// Handle a `FileTransferRequest` — validate and execute (or queue for URL).
    pub fn handle_transfer_request(
        &mut self,
        session_id: &str,
        req: &FileTransferRequest,
        tx: &mpsc::UnboundedSender<Vec<u8>>,
    ) {
        let path = PathBuf::from(&req.file_path);

        // Validate the path
        let accepted = if req.upload {
            // Upload (console → agent): check parent dir is writable and allowed
            path.parent()
                .map(|p| self.is_path_allowed(p) && p.exists())
                .unwrap_or(false)
        } else {
            // Download (agent → console): check file exists and is allowed
            self.is_path_allowed(&path) && path.is_file()
        };

        if !accepted {
            let message = "Transfer rejected: path not allowed or file not found".to_string();
            tracing::warn!("Transfer {} rejected: {}", req.transfer_id, message);

            let envelope = Envelope {
                id: Uuid::new_v4().to_string(),
                session_id: session_id.to_string(),
                timestamp: None,
                payload: Some(envelope::Payload::FileTransferAck(FileTransferAck {
                    transfer_id: req.transfer_id.clone(),
                    accepted: false,
                    presigned_url: String::new(),
                    message,
                })),
            };
            let mut buf = Vec::new();
            if envelope.encode(&mut buf).is_ok() {
                let _ = tx.send(buf);
            }
            return;
        }

        // Check if we already have a pre-signed URL for this transfer
        if let Some(pending) = self.pending_transfers.get_mut(&req.transfer_id) {
            if !pending.presigned_url.is_empty() {
                let url = pending.presigned_url.clone();
                self.execute_transfer(session_id, req, &url, tx);
                self.pending_transfers.remove(&req.transfer_id);
                return;
            }
            // URL not yet arrived — store the request
            pending.request = Some(req.clone());
        } else {
            // No URL yet — store request and wait
            self.pending_transfers.insert(
                req.transfer_id.clone(),
                PendingTransfer {
                    presigned_url: String::new(),
                    request: Some(req.clone()),
                },
            );
        }
    }

    /// Execute the actual file transfer via HTTP.
    fn execute_transfer(
        &self,
        session_id: &str,
        req: &FileTransferRequest,
        presigned_url: &str,
        tx: &mpsc::UnboundedSender<Vec<u8>>,
    ) {
        let client = self.http_client.clone();
        let url = presigned_url.to_string();
        let file_path = req.file_path.clone();
        let file_name = req.file_name.clone();
        let transfer_id = req.transfer_id.clone();
        let upload = req.upload;
        let session_id = session_id.to_string();
        let tx = tx.clone();

        tokio::spawn(async move {
            if upload {
                // Console → Agent: agent downloads (GETs) the file from S3,
                // then saves to local path
                tracing::info!("Downloading {} from S3 to {}", file_name, file_path);
                match download_from_s3(&client, &url, &file_path).await {
                    Ok(()) => {
                        tracing::info!("Download complete: {}", file_name);
                        send_transfer_ack(
                            &session_id,
                            &transfer_id,
                            true,
                            "Download complete",
                            &tx,
                        );
                    }
                    Err(e) => {
                        tracing::error!("Download failed for {}: {}", file_name, e);
                        send_transfer_ack(
                            &session_id,
                            &transfer_id,
                            false,
                            &format!("Download failed: {}", e),
                            &tx,
                        );
                    }
                }
            } else {
                // Agent → Console: agent uploads (PUTs) the local file to S3
                tracing::info!("Uploading {} to S3 from {}", file_name, file_path);
                match upload_to_s3(&client, &url, &file_path).await {
                    Ok(()) => {
                        tracing::info!("Upload complete: {}", file_name);
                        send_transfer_ack(&session_id, &transfer_id, true, "Upload complete", &tx);
                    }
                    Err(e) => {
                        tracing::error!("Upload failed for {}: {}", file_name, e);
                        send_transfer_ack(
                            &session_id,
                            &transfer_id,
                            false,
                            &format!("Upload failed: {}", e),
                            &tx,
                        );
                    }
                }
            }
        });
    }

    /// Check if a path is under any allowed root.
    fn is_path_allowed(&self, path: &Path) -> bool {
        let canonical = match path.canonicalize() {
            Ok(p) => p,
            // If path doesn't exist yet, check parent
            Err(_) => match path.parent().and_then(|p| p.canonicalize().ok()) {
                Some(p) => p,
                None => return false,
            },
        };

        self.allowed_roots
            .iter()
            .any(|root| canonical.starts_with(root))
    }
}

/// Download a file from an S3 pre-signed URL and save locally.
async fn download_from_s3(
    client: &reqwest::Client,
    url: &str,
    local_path: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let response = client.get(url).send().await?;

    if !response.status().is_success() {
        return Err(format!("HTTP {} from S3", response.status()).into());
    }

    let bytes = response.bytes().await?;
    tokio::fs::write(local_path, &bytes).await?;

    Ok(())
}

/// Upload a local file to an S3 pre-signed URL.
async fn upload_to_s3(
    client: &reqwest::Client,
    url: &str,
    local_path: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let data = tokio::fs::read(local_path).await?;

    let response = client
        .put(url)
        .body(data)
        .header("Content-Type", "application/octet-stream")
        .send()
        .await?;

    if !response.status().is_success() {
        return Err(format!("HTTP {} from S3", response.status()).into());
    }

    Ok(())
}

/// Send a FileTransferAck back to the server/console.
fn send_transfer_ack(
    session_id: &str,
    transfer_id: &str,
    accepted: bool,
    message: &str,
    tx: &mpsc::UnboundedSender<Vec<u8>>,
) {
    let envelope = Envelope {
        id: Uuid::new_v4().to_string(),
        session_id: session_id.to_string(),
        timestamp: None,
        payload: Some(envelope::Payload::FileTransferAck(FileTransferAck {
            transfer_id: transfer_id.to_string(),
            accepted,
            presigned_url: String::new(),
            message: message.to_string(),
        })),
    };
    let mut buf = Vec::new();
    if envelope.encode(&mut buf).is_ok() {
        let _ = tx.send(buf);
    }
}

/// Format file permissions as a string (Unix only).
#[cfg(unix)]
fn format_permissions(metadata: &std::fs::Metadata) -> String {
    use std::os::unix::fs::PermissionsExt;
    format!("{:o}", metadata.permissions().mode() & 0o777)
}

#[cfg(not(unix))]
fn format_permissions(metadata: &std::fs::Metadata) -> String {
    if metadata.permissions().readonly() {
        "r--".to_string()
    } else {
        "rw-".to_string()
    }
}
