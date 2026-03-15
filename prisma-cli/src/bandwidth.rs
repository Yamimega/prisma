use anyhow::Result;

use crate::api_client::{self, ApiClient};

pub fn summary(client: &ApiClient) -> Result<()> {
    let data = client.get("/api/bandwidth/summary")?;

    if client.is_json() {
        println!("{}", serde_json::to_string_pretty(&data)?);
        return Ok(());
    }

    let empty = vec![];
    let clients = data["clients"].as_array().unwrap_or(&empty);
    if clients.is_empty() {
        println!("No bandwidth data available.");
        return Ok(());
    }

    let rows: Vec<Vec<String>> = clients
        .iter()
        .map(|c| {
            vec![
                c["client_id"]
                    .as_str()
                    .unwrap_or("-")
                    .chars()
                    .take(8)
                    .collect(),
                c["client_name"].as_str().unwrap_or("-").to_string(),
                c["upload_bps"]
                    .as_u64()
                    .map(api_client::format_bps)
                    .unwrap_or_else(|| "-".to_string()),
                c["download_bps"]
                    .as_u64()
                    .map(api_client::format_bps)
                    .unwrap_or_else(|| "-".to_string()),
                c["quota_bytes"]
                    .as_u64()
                    .map(api_client::format_bytes)
                    .unwrap_or_else(|| "-".to_string()),
                c["quota_used"]
                    .as_u64()
                    .map(api_client::format_bytes)
                    .unwrap_or_else(|| "-".to_string()),
            ]
        })
        .collect();

    api_client::print_table(
        &["Client", "Name", "Upload", "Download", "Quota", "Used"],
        &rows,
    );
    Ok(())
}

pub fn get(client: &ApiClient, id: &str) -> Result<()> {
    let bw = client.get(&format!("/api/clients/{}/bandwidth", id))?;
    let quota = client.get(&format!("/api/clients/{}/quota", id)).ok();

    if client.is_json() {
        let combined = serde_json::json!({
            "bandwidth": bw,
            "quota": quota,
        });
        println!("{}", serde_json::to_string_pretty(&combined)?);
        return Ok(());
    }

    println!("Client: {}", id);
    println!(
        "  Upload:   {}",
        bw["upload_bps"]
            .as_u64()
            .map(api_client::format_bps)
            .unwrap_or_else(|| "-".to_string())
    );
    println!(
        "  Download: {}",
        bw["download_bps"]
            .as_u64()
            .map(api_client::format_bps)
            .unwrap_or_else(|| "-".to_string())
    );

    if let Some(q) = quota {
        println!(
            "  Quota:    {} (used: {}, remaining: {})",
            q["quota_bytes"]
                .as_u64()
                .map(api_client::format_bytes)
                .unwrap_or_else(|| "-".to_string()),
            q["used_bytes"]
                .as_u64()
                .map(api_client::format_bytes)
                .unwrap_or_else(|| "-".to_string()),
            q["remaining_bytes"]
                .as_u64()
                .map(api_client::format_bytes)
                .unwrap_or_else(|| "-".to_string()),
        );
    }

    Ok(())
}

pub fn set(client: &ApiClient, id: &str, upload: Option<u64>, download: Option<u64>) -> Result<()> {
    let body = serde_json::json!({
        "upload_bps": upload,
        "download_bps": download,
    });
    let resp = client.put(&format!("/api/clients/{}/bandwidth", id), &body)?;

    if client.is_json() {
        println!("{}", serde_json::to_string_pretty(&resp)?);
        return Ok(());
    }

    println!("Bandwidth updated for client '{}'.", id);
    if let Some(up) = resp["upload_bps"].as_u64() {
        println!("  Upload:   {}", api_client::format_bps(up));
    }
    if let Some(down) = resp["download_bps"].as_u64() {
        println!("  Download: {}", api_client::format_bps(down));
    }
    Ok(())
}

pub fn quota(client: &ApiClient, id: &str, limit: Option<u64>) -> Result<()> {
    if let Some(limit_bytes) = limit {
        // Set quota
        let body = serde_json::json!({ "quota_bytes": limit_bytes });
        client.put(&format!("/api/clients/{}/quota", id), &body)?;

        if !client.is_json() {
            println!(
                "Quota set to {} for client '{}'.",
                api_client::format_bytes(limit_bytes),
                id
            );
        }
    } else {
        // Get quota
        let data = client.get(&format!("/api/clients/{}/quota", id))?;

        if client.is_json() {
            println!("{}", serde_json::to_string_pretty(&data)?);
            return Ok(());
        }

        println!("Client: {}", id);
        println!(
            "  Quota:     {}",
            data["quota_bytes"]
                .as_u64()
                .map(api_client::format_bytes)
                .unwrap_or_else(|| "-".to_string())
        );
        println!(
            "  Used:      {}",
            data["used_bytes"]
                .as_u64()
                .map(api_client::format_bytes)
                .unwrap_or_else(|| "-".to_string())
        );
        println!(
            "  Remaining: {}",
            data["remaining_bytes"]
                .as_u64()
                .map(api_client::format_bytes)
                .unwrap_or_else(|| "-".to_string())
        );
    }
    Ok(())
}
