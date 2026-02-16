//! FFmpeg auto-provisioning module.
//!
//! Provides [`ensure_ffmpeg()`] which returns the path to a usable `ffmpeg`
//! binary. The resolution order is:
//!
//! 1. System PATH (`which ffmpeg` / `where ffmpeg`)
//! 2. Previously-downloaded binary in the agent data directory
//! 3. Download a static build from GitHub (BtbN for Linux/Windows,
//!    evermeet.cx for macOS Intel, or the system `curl` fallback)
//!
//! On Linux, FFmpeg is not required (GStreamer handles encoding directly),
//! so this module is only active on macOS and Windows.

use std::path::PathBuf;

/// Returns the path to an FFmpeg binary, downloading one if necessary.
///
/// On Linux this always returns `None` because GStreamer is used directly.
pub async fn ensure_ffmpeg() -> Option<PathBuf> {
    // Linux uses GStreamer, not FFmpeg
    if cfg!(target_os = "linux") {
        return None;
    }

    // 1. Check system PATH first
    if let Some(path) = find_in_path() {
        tracing::info!("FFmpeg found in PATH: {}", path.display());
        return Some(path);
    }

    // 2. Check cached download
    let bin_dir = ffmpeg_bin_dir()?;
    let ffmpeg_name = if cfg!(target_os = "windows") {
        "ffmpeg.exe"
    } else {
        "ffmpeg"
    };
    let cached = bin_dir.join(ffmpeg_name);
    if cached.exists() {
        tracing::info!("FFmpeg found in cache: {}", cached.display());
        return Some(cached);
    }

    // 3. Download
    tracing::info!("FFmpeg not found — downloading static build...");
    match download_ffmpeg(&bin_dir).await {
        Ok(path) => {
            tracing::info!("FFmpeg downloaded to: {}", path.display());
            Some(path)
        }
        Err(e) => {
            tracing::warn!("FFmpeg download failed: {} — H264 encoding unavailable", e);
            None
        }
    }
}

/// Locate `ffmpeg` in the system PATH.
fn find_in_path() -> Option<PathBuf> {
    let name = if cfg!(target_os = "windows") {
        "ffmpeg.exe"
    } else {
        "ffmpeg"
    };

    // Check common locations first (Homebrew, system)
    let extra_paths: &[&str] = if cfg!(target_os = "macos") {
        &["/opt/homebrew/bin", "/usr/local/bin", "/usr/bin"]
    } else if cfg!(target_os = "windows") {
        &[r"C:\Program Files\FFmpeg\bin", r"C:\FFmpeg\bin"]
    } else {
        &["/usr/bin", "/usr/local/bin"]
    };

    for dir in extra_paths {
        let candidate = PathBuf::from(dir).join(name);
        if candidate.exists() {
            return Some(candidate);
        }
    }

    // Fall back to PATH lookup via `which`/`where`
    let cmd = if cfg!(target_os = "windows") {
        "where"
    } else {
        "which"
    };

    std::process::Command::new(cmd)
        .arg(name)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| {
            let s = String::from_utf8_lossy(&o.stdout);
            let line = s.lines().next()?.trim().to_string();
            if line.is_empty() {
                None
            } else {
                Some(PathBuf::from(line))
            }
        })
}

/// Agent data directory for storing the FFmpeg binary.
///
/// - macOS:   `/Library/Application Support/ScreenControl/bin/`
/// - Windows: `C:\ProgramData\ScreenControl\bin\`
/// - Linux:   `~/.local/share/screencontrol/bin/`
fn ffmpeg_bin_dir() -> Option<PathBuf> {
    let base = if cfg!(target_os = "macos") {
        // System-wide location (agent runs as root service)
        PathBuf::from("/Library/Application Support/ScreenControl")
    } else if cfg!(target_os = "windows") {
        PathBuf::from(r"C:\ProgramData\ScreenControl")
    } else {
        dirs::data_local_dir()?.join("screencontrol")
    };
    Some(base.join("bin"))
}

/// Download a static FFmpeg build for the current platform.
async fn download_ffmpeg(bin_dir: &std::path::Path) -> anyhow::Result<PathBuf> {
    std::fs::create_dir_all(bin_dir)?;

    let ffmpeg_name = if cfg!(target_os = "windows") {
        "ffmpeg.exe"
    } else {
        "ffmpeg"
    };
    let dest = bin_dir.join(ffmpeg_name);

    // Platform-specific download
    #[cfg(target_os = "macos")]
    {
        download_macos(&dest).await?;
    }
    #[cfg(target_os = "windows")]
    {
        download_windows(&dest).await?;
    }
    #[cfg(target_os = "linux")]
    {
        download_linux(&dest).await?;
    }

    Ok(dest)
}

// ─── macOS ───────────────────────────────────────────────────────────────────

/// Download FFmpeg for macOS.
///
/// Strategy: use the system `curl` to download from evermeet.cx (Intel) or
/// the Homebrew bottle API. As a fallback, try BtbN-style builds.
///
/// Since macOS doesn't have static builds in BtbN, we:
/// 1. Try to install via Homebrew non-interactively
/// 2. Fall back to downloading a pre-built universal binary
#[cfg(target_os = "macos")]
async fn download_macos(dest: &std::path::Path) -> anyhow::Result<()> {
    use tokio::process::Command;

    // Strategy 1: Try `brew install ffmpeg` silently (best option — gets
    // native arm64 build with VideoToolbox support)
    tracing::info!("Attempting Homebrew FFmpeg install...");
    let brew_result = Command::new("brew")
        .args(["install", "--quiet", "ffmpeg"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .await;

    if let Ok(status) = brew_result {
        if status.success() {
            // Find where brew installed it
            if let Some(brew_path) = find_in_path() {
                tracing::info!("FFmpeg installed via Homebrew: {}", brew_path.display());
                // Symlink to our cache location so future lookups are fast
                let _ = std::fs::hard_link(&brew_path, dest)
                    .or_else(|_| std::fs::copy(&brew_path, dest).map(|_| ()));
                return Ok(());
            }
        }
    }

    // Strategy 2: Download a static build via curl
    // evermeet.cx provides macOS Intel builds; for ARM we use the Homebrew
    // bottle directly from GitHub.
    let arch = std::env::consts::ARCH;
    tracing::info!(
        "Homebrew not available, downloading static FFmpeg for macOS {}...",
        arch
    );

    // Use curl (always available on macOS) for the download because reqwest
    // may have TLS issues with some CDNs
    let temp_path = dest.with_extension("download");

    // Try evermeet.cx for Intel, or the FFmpeg static builds project for ARM
    let url = if arch == "aarch64" {
        // For Apple Silicon, we compile from Homebrew bottles
        // As a last resort, use a GitHub-hosted static build
        "https://github.com/eugeneware/ffmpeg-static/releases/latest/download/ffmpeg-darwin-arm64.gz"
    } else {
        // Intel Mac
        "https://github.com/eugeneware/ffmpeg-static/releases/latest/download/ffmpeg-darwin-x64.gz"
    };

    let curl_status = Command::new("curl")
        .args(["-fSL", "--max-time", "120", "-o"])
        .arg(&temp_path)
        .arg(url)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .await?;

    if !curl_status.success() {
        anyhow::bail!("curl download failed for macOS FFmpeg");
    }

    // Decompress gzip
    let gunzip_status = Command::new("gunzip")
        .arg("-f")
        .arg(&temp_path)
        .status()
        .await?;

    if !gunzip_status.success() {
        anyhow::bail!("gunzip failed for macOS FFmpeg");
    }

    // gunzip removes the .download extension, producing the file at `dest`
    let decompressed = dest.with_extension("");
    if decompressed.exists() && decompressed != dest {
        std::fs::rename(&decompressed, dest)?;
    }
    // If temp_path without .download exists, that might be the result
    if !dest.exists() {
        // Try the path without .gz
        let no_ext = temp_path.with_extension("");
        if no_ext.exists() {
            std::fs::rename(&no_ext, dest)?;
        }
    }

    // Make executable
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(dest, std::fs::Permissions::from_mode(0o755))?;
    }

    if !dest.exists() {
        anyhow::bail!("FFmpeg binary not found after download");
    }

    Ok(())
}

// ─── Windows ─────────────────────────────────────────────────────────────────

/// Download FFmpeg for Windows from BtbN GitHub releases.
///
/// Downloads the GPL shared build (smaller than static) and extracts just
/// the ffmpeg.exe binary.
#[cfg(target_os = "windows")]
async fn download_windows(dest: &std::path::Path) -> anyhow::Result<()> {
    let url = "https://github.com/BtbN/FFmpeg-Builds/releases/download/latest/ffmpeg-master-latest-win64-gpl-shared.zip";

    tracing::info!("Downloading FFmpeg for Windows from BtbN...");

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(300))
        .build()?;

    let response = client.get(url).send().await?;
    if !response.status().is_success() {
        anyhow::bail!("Download failed: HTTP {}", response.status());
    }

    let bytes = response.bytes().await?;
    tracing::info!(
        "Downloaded {} MB, extracting ffmpeg.exe...",
        bytes.len() / (1024 * 1024)
    );

    // Extract ffmpeg.exe from the zip
    let reader = std::io::Cursor::new(&bytes);
    let mut archive = zip::ZipArchive::new(reader)?;

    for i in 0..archive.len() {
        let mut file = archive.by_index(i)?;
        let name = file.name().to_string();
        if name.ends_with("/bin/ffmpeg.exe") || name == "ffmpeg.exe" {
            let mut out = std::fs::File::create(dest)?;
            std::io::copy(&mut file, &mut out)?;
            tracing::info!("Extracted ffmpeg.exe ({} bytes)", dest.metadata()?.len());
            return Ok(());
        }
    }

    anyhow::bail!("ffmpeg.exe not found in downloaded archive");
}

// ─── Linux ───────────────────────────────────────────────────────────────────

/// Download FFmpeg for Linux from BtbN GitHub releases.
/// (Normally not needed — Linux uses GStreamer. Provided as a safety net.)
#[cfg(target_os = "linux")]
async fn download_linux(dest: &std::path::Path) -> anyhow::Result<()> {
    let arch = std::env::consts::ARCH;
    let url = if arch == "aarch64" {
        "https://github.com/BtbN/FFmpeg-Builds/releases/download/latest/ffmpeg-master-latest-linuxarm64-gpl.tar.xz"
    } else {
        "https://github.com/BtbN/FFmpeg-Builds/releases/download/latest/ffmpeg-master-latest-linux64-gpl.tar.xz"
    };

    tracing::info!("Downloading FFmpeg for Linux {} from BtbN...", arch);

    // Use system curl + tar for extraction (avoids bringing in xz decompression crate)
    let temp_dir = dest.parent().unwrap().join(".ffmpeg_download");
    std::fs::create_dir_all(&temp_dir)?;
    let archive_path = temp_dir.join("ffmpeg.tar.xz");

    let status = tokio::process::Command::new("curl")
        .args(["-fSL", "--max-time", "300", "-o"])
        .arg(&archive_path)
        .arg(url)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .await?;

    if !status.success() {
        let _ = std::fs::remove_dir_all(&temp_dir);
        anyhow::bail!("curl download failed for Linux FFmpeg");
    }

    // Extract just the ffmpeg binary
    let tar_status = tokio::process::Command::new("tar")
        .args(["xf"])
        .arg(&archive_path)
        .args(["--strip-components=2", "--wildcards", "*/bin/ffmpeg", "-C"])
        .arg(dest.parent().unwrap())
        .status()
        .await?;

    let _ = std::fs::remove_dir_all(&temp_dir);

    if !tar_status.success() {
        anyhow::bail!("tar extraction failed for Linux FFmpeg");
    }

    // Make executable
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(dest, std::fs::Permissions::from_mode(0o755))?;

    if !dest.exists() {
        anyhow::bail!("FFmpeg binary not found after extraction");
    }

    Ok(())
}
