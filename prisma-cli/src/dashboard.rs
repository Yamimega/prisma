use anyhow::Result;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Start the dashboard web server with auto-download support
pub async fn run_dashboard(
    mgmt_url: String,
    token: Option<String>,
    port: u16,
    bind: String,
    no_open: bool,
    update: bool,
    dir: Option<String>,
) -> Result<()> {
    let dashboard_dir = resolve_dashboard_dir(dir, update).await?;

    // Build the reverse proxy + static file server
    let listen_addr = format!("{}:{}", bind, port);
    println!(
        "Dashboard running at http://{}:{}",
        if bind == "0.0.0.0" {
            "127.0.0.1"
        } else {
            &bind
        },
        port
    );

    let app = build_server(dashboard_dir, mgmt_url, token)?;

    // Auto-open browser (skip on headless/SSH)
    if !no_open && !is_headless() {
        let url = format!("http://127.0.0.1:{}", port);
        if let Err(e) = open::that(&url) {
            eprintln!("Failed to open browser: {}. Open manually: {}", e, url);
        }
    } else {
        println!(
            "Open in your local browser: http://{}:{}",
            get_server_ip().unwrap_or_else(|| "127.0.0.1".to_string()),
            port
        );
    }

    let listener = tokio::net::TcpListener::bind(&listen_addr).await?;
    println!("Listening on {}", listen_addr);
    axum::serve(listener, app).await?;

    Ok(())
}

/// Resolve which directory to serve the dashboard from.
/// Priority: --dir flag > cached download > dev build > fresh download
async fn resolve_dashboard_dir(dir: Option<String>, update: bool) -> Result<PathBuf> {
    // 1. Explicit --dir flag
    if let Some(dir) = dir {
        let path = PathBuf::from(&dir);
        if path.join("index.html").exists() {
            println!("Using local dashboard from {}", path.display());
            return Ok(path);
        }
        anyhow::bail!("Dashboard directory '{}' does not contain index.html", dir);
    }

    let cache_dir = get_cache_dir()?;
    let dashboard_dir = cache_dir.join("dashboard");
    let version_file = dashboard_dir.join(".version");

    // 2. If we have a cached version and no --update, use it
    if !update && dashboard_dir.join("index.html").exists() {
        if let Ok(version) = std::fs::read_to_string(&version_file) {
            println!("Using cached dashboard {}", version.trim());
        }
        return Ok(dashboard_dir);
    }

    // 3. Try downloading
    println!("Downloading latest dashboard...");
    match download_dashboard(&dashboard_dir, &version_file).await {
        Ok(version) => {
            println!("Dashboard {} downloaded successfully", version);
            Ok(dashboard_dir)
        }
        Err(e) => {
            // If we have a stale cache, use it
            if dashboard_dir.join("index.html").exists() {
                eprintln!("Warning: Failed to download latest dashboard: {}", e);
                eprintln!("Using cached version");
                return Ok(dashboard_dir);
            }

            // 4. Fall back to local dev build (prisma-dashboard/out/)
            let dev_dir = PathBuf::from("prisma-dashboard/out");
            if dev_dir.join("index.html").exists() {
                eprintln!("Warning: Failed to download dashboard: {}", e);
                println!("Using local dev build from {}", dev_dir.display());
                return Ok(dev_dir);
            }

            anyhow::bail!(
                "Failed to download dashboard: {}\n\n\
                 No cached or local build found. Options:\n\
                 - Build locally: cd prisma-dashboard && npm ci && npm run build\n\
                   Then: prisma dashboard --dir prisma-dashboard/out\n\
                 - Ensure the latest GitHub release has a prisma-dashboard.tar.gz asset",
                e
            );
        }
    }
}

fn get_cache_dir() -> Result<PathBuf> {
    let dir = if cfg!(target_os = "windows") {
        dirs_or_fallback("LOCALAPPDATA", "prisma")
    } else if cfg!(target_os = "macos") {
        dirs_or_fallback("HOME", "Library/Caches/prisma")
    } else {
        // XDG_CACHE_HOME or ~/.cache/prisma
        std::env::var("XDG_CACHE_HOME")
            .map(|p| PathBuf::from(p).join("prisma"))
            .unwrap_or_else(|_| dirs_or_fallback("HOME", ".cache/prisma"))
    };
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

fn dirs_or_fallback(env_var: &str, suffix: &str) -> PathBuf {
    std::env::var(env_var)
        .map(|p| PathBuf::from(p).join(suffix))
        .unwrap_or_else(|_| PathBuf::from(".").join(suffix))
}

async fn download_dashboard(dest: &Path, version_file: &Path) -> Result<String> {
    // Use GitHub Releases API to find latest release with dashboard asset
    let client = reqwest::Client::new();

    let release_url = "https://api.github.com/repos/prisma-proxy/prisma/releases/latest";
    let release: serde_json::Value = client
        .get(release_url)
        .header("User-Agent", "prisma-cli")
        .send()
        .await?
        .json()
        .await?;

    // Check for API errors (rate limit, not found, etc.)
    if let Some(msg) = release["message"].as_str() {
        anyhow::bail!("GitHub API error: {}", msg);
    }

    let version = release["tag_name"]
        .as_str()
        .unwrap_or("unknown")
        .to_string();

    // Check if we already have this version
    if let Ok(cached_version) = std::fs::read_to_string(version_file) {
        if cached_version.trim() == version {
            return Ok(version);
        }
    }

    // Find dashboard asset
    let assets = release["assets"].as_array().ok_or_else(|| {
        anyhow::anyhow!(
            "Release {} has no assets (dashboard may not have been built for this release)",
            version
        )
    })?;

    let asset = assets
        .iter()
        .find(|a| {
            a["name"]
                .as_str()
                .map(|n| n.contains("dashboard") && n.ends_with(".tar.gz"))
                .unwrap_or(false)
        })
        .ok_or_else(|| {
            anyhow::anyhow!(
                "Release {} has no prisma-dashboard.tar.gz asset (found {} other assets)",
                version,
                assets.len()
            )
        })?;

    let download_url = asset["browser_download_url"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("No download URL for asset"))?;

    println!("Downloading from {}", download_url);

    let response = client
        .get(download_url)
        .header("User-Agent", "prisma-cli")
        .send()
        .await?;

    let bytes = response.bytes().await?;

    // Extract tar.gz
    if dest.exists() {
        std::fs::remove_dir_all(dest)?;
    }
    std::fs::create_dir_all(dest)?;

    let decoder = flate2::read::GzDecoder::new(&bytes[..]);
    let mut archive = tar::Archive::new(decoder);
    archive.unpack(dest)?;

    // Write version file
    std::fs::write(version_file, &version)?;

    Ok(version)
}

fn build_server(
    dashboard_dir: PathBuf,
    mgmt_url: String,
    token: Option<String>,
) -> Result<axum::Router> {
    use axum::{routing::any, Router};
    use tower_http::services::ServeDir;

    let mgmt_url = Arc::new(mgmt_url);
    let token = Arc::new(token);

    let app = Router::new()
        .route(
            "/api/{*path}",
            any({
                let mgmt_url = mgmt_url.clone();
                let token = token.clone();
                move |req: axum::extract::Request| {
                    let mgmt_url = mgmt_url.clone();
                    let token = token.clone();
                    async move { proxy_request(req, &mgmt_url, token.as_deref()).await }
                }
            }),
        )
        .fallback_service(ServeDir::new(dashboard_dir).append_index_html_on_directories(true));

    Ok(app)
}

async fn proxy_request(
    req: axum::extract::Request,
    mgmt_url: &str,
    token: Option<&str>,
) -> axum::response::Response {
    use axum::http::StatusCode;
    use axum::response::IntoResponse;

    let client = reqwest::Client::new();
    let path = req.uri().path();
    let query = req
        .uri()
        .query()
        .map(|q| format!("?{}", q))
        .unwrap_or_default();
    let url = format!("{}{}{}", mgmt_url.trim_end_matches('/'), path, query);
    let method = req.method().clone();

    let mut builder = client.request(
        reqwest::Method::from_bytes(method.as_str().as_bytes()).unwrap_or(reqwest::Method::GET),
        &url,
    );

    // Forward auth token
    if let Some(t) = token {
        builder = builder.header("Authorization", format!("Bearer {}", t));
    }

    // Forward request headers (except host)
    for (key, value) in req.headers() {
        if key != "host" {
            if let Ok(v) = value.to_str() {
                builder = builder.header(key.as_str(), v);
            }
        }
    }

    // Forward body
    let body_bytes = axum::body::to_bytes(req.into_body(), 10 * 1024 * 1024)
        .await
        .unwrap_or_default();
    if !body_bytes.is_empty() {
        builder = builder.body(body_bytes);
    }

    match builder.send().await {
        Ok(resp) => {
            let status = StatusCode::from_u16(resp.status().as_u16())
                .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
            let mut response = axum::response::Response::builder().status(status);

            for (key, value) in resp.headers() {
                response = response.header(key.as_str(), value.as_bytes());
            }

            let body = resp.bytes().await.unwrap_or_default();
            response
                .body(axum::body::Body::from(body))
                .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
        }
        Err(e) => {
            eprintln!("Proxy error: {}", e);
            StatusCode::BAD_GATEWAY.into_response()
        }
    }
}

fn is_headless() -> bool {
    // Check for SSH session
    if std::env::var("SSH_TTY").is_ok() || std::env::var("SSH_CONNECTION").is_ok() {
        return true;
    }
    // On Linux, check for DISPLAY
    if cfg!(target_os = "linux")
        && std::env::var("DISPLAY").is_err()
        && std::env::var("WAYLAND_DISPLAY").is_err()
    {
        return true;
    }
    false
}

fn get_server_ip() -> Option<String> {
    // Simple heuristic: try to get a non-loopback IP
    // This is best-effort for display purposes
    std::env::var("HOSTNAME").ok()
}
