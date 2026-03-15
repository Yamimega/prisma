use anyhow::Result;

use crate::api_client::{self, ApiClient};

pub fn snapshot(client: &ApiClient) -> Result<()> {
    let data = client.get("/api/metrics")?;

    if client.is_json() {
        println!("{}", serde_json::to_string_pretty(&data)?);
        return Ok(());
    }

    print_metrics(&data);
    Ok(())
}

pub fn watch(client: &ApiClient, interval: u64) -> Result<()> {
    let mut prev: Option<serde_json::Value> = None;

    loop {
        print!("\x1b[2J\x1b[H");
        println!("Metrics (refreshing every {}s)\n", interval);

        let data = client.get("/api/metrics")?;

        if client.is_json() {
            println!("{}", serde_json::to_string_pretty(&data)?);
        } else {
            print_metrics(&data);

            // Show deltas if we have a previous snapshot
            if let Some(ref p) = prev {
                println!();
                println!("Delta (since last refresh):");

                let d_up = data["total_bytes_up"].as_u64().unwrap_or(0)
                    - p["total_bytes_up"].as_u64().unwrap_or(0);
                let d_down = data["total_bytes_down"].as_u64().unwrap_or(0)
                    - p["total_bytes_down"].as_u64().unwrap_or(0);
                let d_conn = data["total_connections"].as_u64().unwrap_or(0)
                    - p["total_connections"].as_u64().unwrap_or(0);

                println!("  Bytes Up:     +{}", api_client::format_bytes(d_up));
                println!("  Bytes Down:   +{}", api_client::format_bytes(d_down));
                println!("  Connections:  +{}", d_conn);
            }
        }

        prev = Some(data);
        std::thread::sleep(std::time::Duration::from_secs(interval));
    }
}

pub fn history(client: &ApiClient, period: &str) -> Result<()> {
    let data = client.get_query("/api/metrics/history", &[("period", period)])?;

    if client.is_json() {
        println!("{}", serde_json::to_string_pretty(&data)?);
        return Ok(());
    }

    let empty = vec![];
    let arr = data.as_array().unwrap_or(&empty);
    if arr.is_empty() {
        println!("No history data available.");
        return Ok(());
    }

    let rows: Vec<Vec<String>> = arr
        .iter()
        .map(|m| {
            vec![
                m["timestamp"]
                    .as_str()
                    .unwrap_or("-")
                    .chars()
                    .take(19)
                    .collect(),
                m["active_connections"]
                    .as_u64()
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| "-".to_string()),
                m["total_bytes_up"]
                    .as_u64()
                    .map(api_client::format_bytes)
                    .unwrap_or_else(|| "-".to_string()),
                m["total_bytes_down"]
                    .as_u64()
                    .map(api_client::format_bytes)
                    .unwrap_or_else(|| "-".to_string()),
            ]
        })
        .collect();

    api_client::print_table(&["Timestamp", "Active", "Bytes Up", "Bytes Down"], &rows);
    Ok(())
}

pub fn system(client: &ApiClient) -> Result<()> {
    let data = client.get("/api/system/info")?;

    if client.is_json() {
        println!("{}", serde_json::to_string_pretty(&data)?);
        return Ok(());
    }

    println!("System Information:");
    println!("  Version:    {}", data["version"].as_str().unwrap_or("-"));
    println!("  Platform:   {}", data["platform"].as_str().unwrap_or("-"));
    println!(
        "  PID:        {}",
        data["pid"]
            .as_u64()
            .map(|v| v.to_string())
            .unwrap_or_else(|| "-".to_string())
    );
    println!(
        "  CPU:        {:.1}%",
        data["cpu_usage"].as_f64().unwrap_or(0.0)
    );
    println!(
        "  Memory:     {} / {} MB",
        data["memory_used_mb"]
            .as_u64()
            .map(|v| v.to_string())
            .unwrap_or_else(|| "?".to_string()),
        data["memory_total_mb"]
            .as_u64()
            .map(|v| v.to_string())
            .unwrap_or_else(|| "?".to_string()),
    );

    if let Some(days) = data["cert_expiry_days"].as_i64() {
        println!("  Cert Expiry: {} days", days);
    }

    if let Some(listeners) = data["listeners"].as_array() {
        println!("  Listeners:");
        for l in listeners {
            println!(
                "    {} ({})",
                l["addr"].as_str().unwrap_or("?"),
                l["protocol"].as_str().unwrap_or("?")
            );
        }
    }

    Ok(())
}

fn print_metrics(data: &serde_json::Value) {
    if let Some(uptime) = data["uptime_secs"].as_u64() {
        println!("  Uptime:       {}", api_client::format_duration(uptime));
    }
    if let Some(active) = data["active_connections"].as_u64() {
        println!("  Active:       {} connections", active);
    }
    if let Some(total) = data["total_connections"].as_u64() {
        println!("  Total:        {} connections", total);
    }
    if let Some(up) = data["total_bytes_up"].as_u64() {
        println!("  Bytes Up:     {}", api_client::format_bytes(up));
    }
    if let Some(down) = data["total_bytes_down"].as_u64() {
        println!("  Bytes Down:   {}", api_client::format_bytes(down));
    }
    if let Some(failures) = data["handshake_failures"].as_u64() {
        if failures > 0 {
            println!("  HS Failures:  {}", failures);
        }
    }
}
