use anyhow::Result;

use crate::api_client::{self, ApiClient};

pub fn run_status(client: &ApiClient) -> Result<()> {
    // Fetch health
    match client.get("/api/health") {
        Ok(body) => {
            if client.is_json() {
                // Merge health + metrics into one JSON response
                let metrics = client.get("/api/metrics").ok();
                let combined = serde_json::json!({
                    "health": body,
                    "metrics": metrics,
                });
                println!("{}", serde_json::to_string_pretty(&combined)?);
                return Ok(());
            }

            println!("Server Status: UP");
            if let Some(version) = body.get("version").and_then(|v| v.as_str()) {
                println!("  Version:  {}", version);
            }
        }
        Err(e) => {
            if client.is_json() {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&serde_json::json!({
                        "status": "down",
                        "error": e.to_string()
                    }))?
                );
            } else {
                println!("Server Status: DOWN ({})", e);
            }
            return Ok(());
        }
    }

    // Fetch metrics
    match client.get("/api/metrics") {
        Ok(body) => {
            if let Some(uptime) = body.get("uptime_secs").and_then(|v| v.as_u64()) {
                println!("  Uptime:   {}", api_client::format_duration(uptime));
            }
            if let Some(active) = body.get("active_connections").and_then(|v| v.as_u64()) {
                println!("  Active:   {} connections", active);
            }
            if let Some(total) = body.get("total_connections").and_then(|v| v.as_u64()) {
                println!("  Total:    {} connections", total);
            }
            if let Some(up) = body.get("total_bytes_up").and_then(|v| v.as_u64()) {
                println!("  Bytes Up: {}", api_client::format_bytes(up));
            }
            if let Some(down) = body.get("total_bytes_down").and_then(|v| v.as_u64()) {
                println!("  Bytes Dn: {}", api_client::format_bytes(down));
            }
            if let Some(failures) = body.get("handshake_failures").and_then(|v| v.as_u64()) {
                if failures > 0 {
                    println!("  HS Fails: {}", failures);
                }
            }
        }
        Err(e) => {
            println!("  Metrics:  unavailable ({})", e);
        }
    }

    Ok(())
}
