use std::path::Path;

use anyhow::{Context, Result};

pub struct ApiClient {
    url: String,
    token: String,
    json: bool,
    agent: ureq::Agent,
}

impl ApiClient {
    /// Resolve API client from flags > env vars > server.toml auto-detect.
    pub fn resolve(flag_url: Option<&str>, flag_token: Option<&str>, json: bool) -> Result<Self> {
        let (url, token) = match (flag_url, flag_token) {
            (Some(u), Some(t)) => (u.to_string(), t.to_string()),
            (Some(u), None) => {
                let t = std::env::var("PRISMA_MGMT_TOKEN").unwrap_or_default();
                (u.to_string(), t)
            }
            (None, Some(t)) => {
                let u = std::env::var("PRISMA_MGMT_URL")
                    .unwrap_or_else(|_| "https://127.0.0.1:9090".to_string());
                (u, t.to_string())
            }
            (None, None) => {
                // Try env vars first
                let env_url = std::env::var("PRISMA_MGMT_URL").ok();
                let env_token = std::env::var("PRISMA_MGMT_TOKEN").ok();

                if let Some(u) = env_url {
                    (u, env_token.unwrap_or_default())
                } else {
                    // Try auto-detect from server.toml
                    Self::auto_detect().unwrap_or_else(|| {
                        (
                            "https://127.0.0.1:9090".to_string(),
                            env_token.unwrap_or_default(),
                        )
                    })
                }
            }
        };

        let tls = ureq::tls::TlsConfig::builder()
            .disable_verification(true)
            .build();

        let agent = ureq::Agent::config_builder()
            .tls_config(tls)
            .build()
            .new_agent();

        Ok(Self {
            url: url.trim_end_matches('/').to_string(),
            token,
            json,
            agent,
        })
    }

    fn auto_detect() -> Option<(String, String)> {
        let candidates = if cfg!(windows) {
            let mut v = Vec::new();
            v.push(std::path::PathBuf::from("server.toml"));
            if let Ok(pd) = std::env::var("PROGRAMDATA") {
                v.push(
                    std::path::PathBuf::from(pd)
                        .join("prisma")
                        .join("server.toml"),
                );
            }
            if let Ok(home) = std::env::var("USERPROFILE") {
                v.push(
                    std::path::PathBuf::from(home)
                        .join(".config")
                        .join("prisma")
                        .join("server.toml"),
                );
            }
            v
        } else {
            let mut v = vec![
                std::path::PathBuf::from("server.toml"),
                std::path::PathBuf::from("/etc/prisma/server.toml"),
            ];
            if let Ok(home) = std::env::var("HOME") {
                v.push(
                    std::path::PathBuf::from(home)
                        .join(".config")
                        .join("prisma")
                        .join("server.toml"),
                );
            }
            v
        };

        for path in candidates {
            if let Some(result) = Self::parse_server_toml(&path) {
                return Some(result);
            }
        }
        None
    }

    fn parse_server_toml(path: &Path) -> Option<(String, String)> {
        let content = std::fs::read_to_string(path).ok()?;
        let table: toml::Table = content.parse().ok()?;
        let mgmt = table.get("management_api")?.as_table()?;
        let listen = mgmt.get("listen_addr")?.as_str()?;
        let token = mgmt
            .get("auth_token")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        // Default to https since most setups use TLS
        Some((format!("https://{}", listen), token))
    }

    pub fn is_json(&self) -> bool {
        self.json
    }

    pub fn ws_url(&self, path: &str) -> String {
        let base = self
            .url
            .replace("https://", "wss://")
            .replace("http://", "ws://");
        if self.token.is_empty() {
            format!("{}{}", base, path)
        } else {
            let encoded = urlencoded(&self.token);
            format!("{}{}?token={}", base, path, encoded)
        }
    }

    pub fn get(&self, path: &str) -> Result<serde_json::Value> {
        let url = format!("{}{}", self.url, path);
        let mut resp = self
            .agent
            .get(&url)
            .header("Authorization", &format!("Bearer {}", self.token))
            .call()
            .with_context(|| format!("GET {}", path))?;
        let body: serde_json::Value = resp.body_mut().read_json()?;
        Ok(body)
    }

    pub fn get_query(&self, path: &str, query: &[(&str, &str)]) -> Result<serde_json::Value> {
        let mut url = format!("{}{}", self.url, path);
        if !query.is_empty() {
            url.push('?');
            for (i, (k, v)) in query.iter().enumerate() {
                if i > 0 {
                    url.push('&');
                }
                url.push_str(&format!("{}={}", k, urlencoded(v)));
            }
        }
        let mut resp = self
            .agent
            .get(&url)
            .header("Authorization", &format!("Bearer {}", self.token))
            .call()
            .with_context(|| format!("GET {}", path))?;
        let body: serde_json::Value = resp.body_mut().read_json()?;
        Ok(body)
    }

    pub fn post(&self, path: &str, body: &serde_json::Value) -> Result<serde_json::Value> {
        let url = format!("{}{}", self.url, path);
        let mut resp = self
            .agent
            .post(&url)
            .header("Authorization", &format!("Bearer {}", self.token))
            .send_json(body)
            .with_context(|| format!("POST {}", path))?;
        let text = resp.body_mut().read_to_string()?;
        if text.is_empty() {
            Ok(serde_json::json!({"status": "ok"}))
        } else {
            serde_json::from_str(&text)
                .with_context(|| format!("Failed to parse response from POST {}", path))
        }
    }

    pub fn post_empty(&self, path: &str) -> Result<serde_json::Value> {
        let url = format!("{}{}", self.url, path);
        let mut resp = self
            .agent
            .post(&url)
            .header("Authorization", &format!("Bearer {}", self.token))
            .send(&[] as &[u8])
            .with_context(|| format!("POST {}", path))?;
        let text = resp.body_mut().read_to_string()?;
        if text.is_empty() {
            Ok(serde_json::json!({"status": "ok"}))
        } else {
            serde_json::from_str(&text)
                .with_context(|| format!("Failed to parse response from POST {}", path))
        }
    }

    pub fn put(&self, path: &str, body: &serde_json::Value) -> Result<serde_json::Value> {
        let url = format!("{}{}", self.url, path);
        let mut resp = self
            .agent
            .put(&url)
            .header("Authorization", &format!("Bearer {}", self.token))
            .send_json(body)
            .with_context(|| format!("PUT {}", path))?;
        let text = resp.body_mut().read_to_string()?;
        if text.is_empty() {
            Ok(serde_json::json!({"status": "ok"}))
        } else {
            serde_json::from_str(&text)
                .with_context(|| format!("Failed to parse response from PUT {}", path))
        }
    }

    pub fn patch(&self, path: &str, body: &serde_json::Value) -> Result<serde_json::Value> {
        let url = format!("{}{}", self.url, path);
        let mut resp = self
            .agent
            .patch(&url)
            .header("Authorization", &format!("Bearer {}", self.token))
            .send_json(body)
            .with_context(|| format!("PATCH {}", path))?;
        let text = resp.body_mut().read_to_string()?;
        if text.is_empty() {
            Ok(serde_json::json!({"status": "ok"}))
        } else {
            serde_json::from_str(&text)
                .with_context(|| format!("Failed to parse response from PATCH {}", path))
        }
    }

    pub fn delete(&self, path: &str) -> Result<serde_json::Value> {
        let url = format!("{}{}", self.url, path);
        let mut resp = self
            .agent
            .delete(&url)
            .header("Authorization", &format!("Bearer {}", self.token))
            .call()
            .with_context(|| format!("DELETE {}", path))?;
        let text = resp.body_mut().read_to_string()?;
        if text.is_empty() {
            Ok(serde_json::json!({"status": "ok"}))
        } else {
            serde_json::from_str(&text)
                .with_context(|| format!("Failed to parse response from DELETE {}", path))
        }
    }
}

// --- Output helpers ---

pub fn print_table(headers: &[&str], rows: &[Vec<String>]) {
    let mut table = comfy_table::Table::new();
    table.set_header(headers);
    for row in rows {
        table.add_row(row);
    }
    println!("{table}");
}

pub fn format_bytes(bytes: u64) -> String {
    if bytes >= 1_073_741_824 {
        format!("{:.2} GiB", bytes as f64 / 1_073_741_824.0)
    } else if bytes >= 1_048_576 {
        format!("{:.2} MiB", bytes as f64 / 1_048_576.0)
    } else if bytes >= 1024 {
        format!("{:.2} KiB", bytes as f64 / 1024.0)
    } else {
        format!("{} B", bytes)
    }
}

pub fn format_duration(secs: u64) -> String {
    if secs >= 86400 {
        let days = secs / 86400;
        let hours = (secs % 86400) / 3600;
        format!("{}d {}h", days, hours)
    } else if secs >= 3600 {
        let hours = secs / 3600;
        let mins = (secs % 3600) / 60;
        format!("{}h {}m", hours, mins)
    } else if secs >= 60 {
        let mins = secs / 60;
        let s = secs % 60;
        format!("{}m {}s", mins, s)
    } else {
        format!("{}s", secs)
    }
}

pub fn format_bps(bps: u64) -> String {
    if bps == 0 {
        "unlimited".to_string()
    } else if bps >= 1_000_000_000 {
        format!("{:.1} Gbps", bps as f64 / 1_000_000_000.0)
    } else if bps >= 1_000_000 {
        format!("{:.1} Mbps", bps as f64 / 1_000_000.0)
    } else if bps >= 1_000 {
        format!("{:.1} Kbps", bps as f64 / 1_000.0)
    } else {
        format!("{} bps", bps)
    }
}

fn urlencoded(s: &str) -> String {
    let mut result = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                result.push(b as char);
            }
            _ => {
                result.push_str(&format!("%{:02X}", b));
            }
        }
    }
    result
}
