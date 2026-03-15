use anyhow::Result;

use crate::api_client::{self, ApiClient};

pub fn list(client: &ApiClient) -> Result<()> {
    let data = client.get("/api/routes")?;

    if client.is_json() {
        println!("{}", serde_json::to_string_pretty(&data)?);
        return Ok(());
    }

    let empty = vec![];
    let arr = data.as_array().unwrap_or(&empty);
    if arr.is_empty() {
        println!("No routing rules configured.");
        return Ok(());
    }

    let rows: Vec<Vec<String>> = arr
        .iter()
        .map(|r| {
            vec![
                r["id"].as_str().unwrap_or("-").chars().take(8).collect(),
                r["name"].as_str().unwrap_or("-").to_string(),
                r["priority"]
                    .as_u64()
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| "-".to_string()),
                format_condition(&r["condition"]),
                r["action"].as_str().unwrap_or("-").to_string(),
                r["enabled"]
                    .as_bool()
                    .map(|b| if b { "yes" } else { "no" })
                    .unwrap_or("-")
                    .to_string(),
            ]
        })
        .collect();

    api_client::print_table(
        &["ID", "Name", "Priority", "Condition", "Action", "Enabled"],
        &rows,
    );
    Ok(())
}

pub fn create(
    client: &ApiClient,
    name: &str,
    condition: &str,
    action: &str,
    priority: u32,
) -> Result<()> {
    let cond = parse_condition(condition)?;

    let body = serde_json::json!({
        "name": name,
        "priority": priority,
        "condition": cond,
        "action": action,
        "enabled": true,
    });

    let resp = client.post("/api/routes", &body)?;

    if client.is_json() {
        println!("{}", serde_json::to_string_pretty(&resp)?);
        return Ok(());
    }

    println!(
        "Route '{}' created (ID: {}).",
        name,
        resp["id"].as_str().unwrap_or("?")
    );
    Ok(())
}

pub fn update(
    client: &ApiClient,
    id: &str,
    condition: Option<&str>,
    action: Option<&str>,
    priority: Option<u32>,
    name: Option<&str>,
) -> Result<()> {
    // First get existing route to merge
    let existing = client.get("/api/routes")?;
    let empty = vec![];
    let arr = existing.as_array().unwrap_or(&empty);
    let route = arr
        .iter()
        .find(|r| r["id"].as_str() == Some(id))
        .ok_or_else(|| anyhow::anyhow!("Route '{}' not found", id))?;

    let cond = if let Some(c) = condition {
        parse_condition(c)?
    } else {
        route["condition"].clone()
    };

    let body = serde_json::json!({
        "name": name.unwrap_or_else(|| route["name"].as_str().unwrap_or("")),
        "priority": priority.unwrap_or_else(|| route["priority"].as_u64().unwrap_or(0) as u32),
        "condition": cond,
        "action": action.unwrap_or_else(|| route["action"].as_str().unwrap_or("Allow")),
        "enabled": route["enabled"].as_bool().unwrap_or(true),
    });

    client.put(&format!("/api/routes/{}", id), &body)?;

    if !client.is_json() {
        println!("Route '{}' updated.", id);
    }
    Ok(())
}

pub fn delete(client: &ApiClient, id: &str) -> Result<()> {
    client.delete(&format!("/api/routes/{}", id))?;

    if !client.is_json() {
        println!("Route '{}' deleted.", id);
    }
    Ok(())
}

pub fn setup(client: &ApiClient, preset: &str, clear: bool) -> Result<()> {
    let rules: &[(&str, &str, &str, u32)] = match preset {
        "block-ads" => &PRESET_BLOCK_ADS,
        "privacy" => &PRESET_PRIVACY,
        "allow-all" => &[("allow-all", "All", "Allow", 1000)],
        "block-all" => &[("block-all", "All", "Block", 1000)],
        _ => anyhow::bail!(
            "Unknown preset '{preset}'. Available: block-ads, privacy, allow-all, block-all"
        ),
    };

    if clear {
        let existing = client.get("/api/routes")?;
        let empty = vec![];
        let arr = existing.as_array().unwrap_or(&empty);
        let ids: Vec<String> = arr
            .iter()
            .filter_map(|r| r["id"].as_str().map(str::to_string))
            .collect();
        let count = ids.len();
        for id in &ids {
            client.delete(&format!("/api/routes/{}", id))?;
        }
        if !client.is_json() && count > 0 {
            println!("Cleared {} existing rule(s).", count);
        }
    }

    let mut created = 0usize;
    for (name, condition, action, priority) in rules {
        let cond = parse_condition(condition)?;
        let body = serde_json::json!({
            "name": name,
            "priority": priority,
            "condition": cond,
            "action": action,
            "enabled": true,
        });
        client.post("/api/routes", &body)?;
        created += 1;
    }

    if client.is_json() {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "preset": preset,
                "created": created,
            }))?
        );
    } else {
        println!("Applied preset '{}': {} rule(s) created.", preset, created);
        println!();
        println!(
            "  {:<5}  {:<32}  {:<16}  {}",
            "Pri", "Name", "Condition", "Action"
        );
        println!("  {}", "-".repeat(65));
        for (name, condition, action, priority) in rules {
            println!(
                "  {:<5}  {:<32}  {:<16}  {}",
                priority, name, condition, action
            );
        }
    }
    Ok(())
}

// --- Presets ---

static PRESET_BLOCK_ADS: &[(&str, &str, &str, u32)] = &[
    ("block-ads-wildcard", "DomainMatch:*.ads.*", "Block", 10),
    ("block-ad-wildcard", "DomainMatch:*.ad.*", "Block", 11),
    ("block-doubleclick", "DomainMatch:*.doubleclick.net", "Block", 12),
    (
        "block-googlesyndication",
        "DomainMatch:*.googlesyndication.com",
        "Block",
        13,
    ),
    ("block-adnxs", "DomainMatch:*.adnxs.com", "Block", 14),
    ("block-advertising", "DomainMatch:*.advertising.com", "Block", 15),
    ("block-adsystem", "DomainMatch:*.adsystem.com", "Block", 16),
    ("block-adservice", "DomainMatch:*.adservice.com", "Block", 17),
    ("block-adserver", "DomainMatch:*.adserver.*", "Block", 18),
    ("block-pagead", "DomainMatch:*.pagead.*", "Block", 19),
];

static PRESET_PRIVACY: &[(&str, &str, &str, u32)] = &[
    // Ads (same as block-ads)
    ("block-ads-wildcard", "DomainMatch:*.ads.*", "Block", 10),
    ("block-ad-wildcard", "DomainMatch:*.ad.*", "Block", 11),
    ("block-doubleclick", "DomainMatch:*.doubleclick.net", "Block", 12),
    (
        "block-googlesyndication",
        "DomainMatch:*.googlesyndication.com",
        "Block",
        13,
    ),
    ("block-adnxs", "DomainMatch:*.adnxs.com", "Block", 14),
    ("block-advertising", "DomainMatch:*.advertising.com", "Block", 15),
    ("block-adsystem", "DomainMatch:*.adsystem.com", "Block", 16),
    ("block-adservice", "DomainMatch:*.adservice.com", "Block", 17),
    ("block-adserver", "DomainMatch:*.adserver.*", "Block", 18),
    ("block-pagead", "DomainMatch:*.pagead.*", "Block", 19),
    // Analytics & telemetry
    (
        "block-google-analytics",
        "DomainMatch:*.google-analytics.com",
        "Block",
        20,
    ),
    ("block-hotjar", "DomainMatch:*.hotjar.com", "Block", 21),
    ("block-mixpanel", "DomainMatch:*.mixpanel.com", "Block", 22),
    ("block-segment", "DomainMatch:*.segment.io", "Block", 23),
    (
        "block-amplitude",
        "DomainMatch:*.amplitude.com",
        "Block",
        24,
    ),
    ("block-criteo", "DomainMatch:*.criteo.com", "Block", 25),
    (
        "block-scorecardresearch",
        "DomainMatch:*.scorecardresearch.com",
        "Block",
        26,
    ),
    (
        "block-quantserve",
        "DomainMatch:*.quantserve.com",
        "Block",
        27,
    ),
    ("block-newrelic", "DomainMatch:*.newrelic.com", "Block", 28),
    ("block-sentry", "DomainMatch:*.sentry.io", "Block", 29),
];

// --- Helpers ---

/// Parse condition shorthand: `TYPE:VALUE`
/// Examples: `DomainMatch:*.ads.*`, `IpCidr:10.0.0.0/8`, `PortRange:80-443`, `All`
fn parse_condition(s: &str) -> Result<serde_json::Value> {
    if s.eq_ignore_ascii_case("all") {
        return Ok(serde_json::json!({"type": "All", "value": null}));
    }

    let (ctype, value) = s.split_once(':').ok_or_else(|| {
        anyhow::anyhow!(
            "Invalid condition format. Use TYPE:VALUE (e.g., DomainMatch:*.example.com)"
        )
    })?;

    match ctype {
        "DomainMatch" | "domainmatch" => {
            Ok(serde_json::json!({"type": "DomainMatch", "value": value}))
        }
        "DomainExact" | "domainexact" => {
            Ok(serde_json::json!({"type": "DomainExact", "value": value}))
        }
        "IpCidr" | "ipcidr" => Ok(serde_json::json!({"type": "IpCidr", "value": value})),
        "PortRange" | "portrange" => {
            let (start, end) = value
                .split_once('-')
                .ok_or_else(|| anyhow::anyhow!("PortRange format: START-END (e.g., 80-443)"))?;
            let start: u16 = start.parse()?;
            let end: u16 = end.parse()?;
            Ok(serde_json::json!({"type": "PortRange", "value": [start, end]}))
        }
        _ => anyhow::bail!(
            "Unknown condition type '{}'. Use: DomainMatch, DomainExact, IpCidr, PortRange, All",
            ctype
        ),
    }
}

fn format_condition(cond: &serde_json::Value) -> String {
    let ctype = cond["type"].as_str().unwrap_or("?");
    match ctype {
        "All" => "All".to_string(),
        "PortRange" => {
            if let Some(arr) = cond["value"].as_array() {
                format!(
                    "PortRange:{}-{}",
                    arr.first().and_then(|v| v.as_u64()).unwrap_or(0),
                    arr.get(1).and_then(|v| v.as_u64()).unwrap_or(0)
                )
            } else {
                "PortRange:?".to_string()
            }
        }
        _ => {
            let value = cond["value"].as_str().unwrap_or("?");
            format!("{}:{}", ctype, value)
        }
    }
}
