use anyhow::Result;

use crate::api_client::{self, ApiClient};

pub fn list(client: &ApiClient) -> Result<()> {
    let data = client.get("/api/connections")?;

    if client.is_json() {
        println!("{}", serde_json::to_string_pretty(&data)?);
        return Ok(());
    }

    print_connections(&data);
    Ok(())
}

pub fn disconnect(client: &ApiClient, id: &str) -> Result<()> {
    client.delete(&format!("/api/connections/{}", id))?;

    if !client.is_json() {
        println!("Connection '{}' disconnected.", id);
    }
    Ok(())
}

pub fn watch(client: &ApiClient, interval: u64) -> Result<()> {
    loop {
        // Clear screen
        print!("\x1b[2J\x1b[H");

        let data = client.get("/api/connections")?;

        if client.is_json() {
            println!("{}", serde_json::to_string_pretty(&data)?);
        } else {
            let count = data.as_array().map(|a| a.len()).unwrap_or(0);
            println!(
                "Connections (refreshing every {}s, {} active)\n",
                interval, count
            );
            print_connections(&data);
        }

        std::thread::sleep(std::time::Duration::from_secs(interval));
    }
}

fn print_connections(data: &serde_json::Value) {
    let empty = vec![];
    let arr = data.as_array().unwrap_or(&empty);
    if arr.is_empty() {
        println!("No active connections.");
        return;
    }

    let rows: Vec<Vec<String>> = arr
        .iter()
        .map(|c| {
            let session = c["session_id"]
                .as_str()
                .unwrap_or("-")
                .chars()
                .take(8)
                .collect::<String>();
            let client_name = c["client_name"].as_str().unwrap_or("-").to_string();
            let transport = c["transport"].as_str().unwrap_or("-").to_string();
            let mode = c["mode"].as_str().unwrap_or("-").to_string();
            let peer = c["peer_addr"].as_str().unwrap_or("-").to_string();

            let uptime = c["connected_at"]
                .as_str()
                .and_then(|t| chrono::DateTime::parse_from_rfc3339(t).ok())
                .map(|dt| {
                    let elapsed = chrono::Utc::now().signed_duration_since(dt);
                    api_client::format_duration(elapsed.num_seconds().max(0) as u64)
                })
                .unwrap_or_else(|| "-".to_string());

            let up = c["bytes_up"]
                .as_u64()
                .map(api_client::format_bytes)
                .unwrap_or_else(|| "-".to_string());
            let down = c["bytes_down"]
                .as_u64()
                .map(api_client::format_bytes)
                .unwrap_or_else(|| "-".to_string());

            vec![
                session,
                client_name,
                transport,
                mode,
                peer,
                uptime,
                up,
                down,
            ]
        })
        .collect();

    api_client::print_table(
        &[
            "Session",
            "Client",
            "Transport",
            "Mode",
            "Peer",
            "Uptime",
            "Upload",
            "Download",
        ],
        &rows,
    );
}
