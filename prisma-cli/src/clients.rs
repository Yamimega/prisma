use anyhow::Result;

use crate::api_client::{self, ApiClient};

pub fn list(client: &ApiClient) -> Result<()> {
    let data = client.get("/api/clients")?;

    if client.is_json() {
        println!("{}", serde_json::to_string_pretty(&data)?);
        return Ok(());
    }

    let empty = vec![];
    let arr = data.as_array().unwrap_or(&empty);
    if arr.is_empty() {
        println!("No clients configured.");
        return Ok(());
    }

    let rows: Vec<Vec<String>> = arr
        .iter()
        .map(|c| {
            vec![
                c["id"].as_str().unwrap_or("-").to_string(),
                c["name"].as_str().unwrap_or("-").to_string(),
                c["enabled"]
                    .as_bool()
                    .map(|b| if b { "yes" } else { "no" })
                    .unwrap_or("-")
                    .to_string(),
            ]
        })
        .collect();

    api_client::print_table(&["ID", "Name", "Enabled"], &rows);
    Ok(())
}

pub fn show(client: &ApiClient, id: &str) -> Result<()> {
    let data = client.get("/api/clients")?;
    let empty = vec![];
    let arr = data.as_array().unwrap_or(&empty);

    let found = arr.iter().find(|c| c["id"].as_str() == Some(id));

    match found {
        Some(c) => {
            if client.is_json() {
                println!("{}", serde_json::to_string_pretty(c)?);
            } else {
                println!("Client ID: {}", c["id"].as_str().unwrap_or("-"));
                println!("Name:      {}", c["name"].as_str().unwrap_or("-"));
                println!(
                    "Enabled:   {}",
                    c["enabled"]
                        .as_bool()
                        .map(|b| if b { "yes" } else { "no" })
                        .unwrap_or("-")
                );
            }
        }
        None => {
            anyhow::bail!("Client '{}' not found", id);
        }
    }
    Ok(())
}

pub fn create(client: &ApiClient, name: Option<&str>) -> Result<()> {
    let body = serde_json::json!({ "name": name });
    let resp = client.post("/api/clients", &body)?;

    if client.is_json() {
        println!("{}", serde_json::to_string_pretty(&resp)?);
        return Ok(());
    }

    let id = resp["id"].as_str().unwrap_or("?");
    let secret = resp["auth_secret_hex"].as_str().unwrap_or("?");
    let cname = resp["name"].as_str().unwrap_or("-");

    println!("Client created successfully!");
    println!();
    println!("Client ID:   {}", id);
    println!("Name:        {}", cname);
    println!("Auth Secret: {}", secret);
    println!();
    println!("# Add to server.toml:");
    println!("[[authorized_clients]]");
    println!("id = \"{}\"", id);
    println!("auth_secret = \"{}\"", secret);
    println!("name = \"{}\"", cname);
    println!();
    println!("# Add to client.toml:");
    println!("[identity]");
    println!("client_id = \"{}\"", id);
    println!("auth_secret = \"{}\"", secret);

    Ok(())
}

pub fn delete(client: &ApiClient, id: &str) -> Result<()> {
    client.delete(&format!("/api/clients/{}", id))?;

    if !client.is_json() {
        println!("Client '{}' deleted.", id);
    }
    Ok(())
}

pub fn enable(client: &ApiClient, id: &str) -> Result<()> {
    let body = serde_json::json!({ "enabled": true });
    client.put(&format!("/api/clients/{}", id), &body)?;

    if !client.is_json() {
        println!("Client '{}' enabled.", id);
    }
    Ok(())
}

pub fn disable(client: &ApiClient, id: &str) -> Result<()> {
    let body = serde_json::json!({ "enabled": false });
    client.put(&format!("/api/clients/{}", id), &body)?;

    if !client.is_json() {
        println!("Client '{}' disabled.", id);
    }
    Ok(())
}
