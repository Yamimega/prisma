---
sidebar_position: 6
---

# Routing Rules

The routing rules engine controls which destinations clients can connect to. Rules are evaluated at connection time before the outbound connection is established.

## Overview

Rules are managed at runtime via the [Management API](/docs/features/management-api) or the [Dashboard](/docs/features/dashboard). No server restart is required.

- Rules are evaluated in **priority order** (lowest number first)
- The **first matching rule** determines the action
- If **no rule matches**, traffic is **allowed** by default
- Rules can be **enabled or disabled** without removing them

## Rule Conditions

| Type | Value | Matches |
|------|-------|---------|
| `DomainMatch` | Glob pattern (e.g. `*.google.com`) | Domain destinations matching the glob |
| `DomainExact` | Exact domain (e.g. `example.com`) | Exact domain match (case-insensitive) |
| `IpCidr` | CIDR notation (e.g. `192.168.0.0/16`) | IPv4 destinations in the CIDR range |
| `PortRange` | Two numbers (e.g. `[80, 443]`) | Destinations with port in the range |
| `All` | — | All traffic |

## Rule Actions

- **Allow** — permit the connection
- **Block** — reject the connection (client receives an error)

## Examples

### Block all traffic to a domain

```json
{
  "name": "Block ads",
  "priority": 10,
  "condition": { "type": "DomainMatch", "value": "*.doubleclick.net" },
  "action": "Block",
  "enabled": true
}
```

### Allow only HTTPS traffic

```json
{
  "name": "Allow HTTPS",
  "priority": 1,
  "condition": { "type": "PortRange", "value": [443, 443] },
  "action": "Allow",
  "enabled": true
}
```

```json
{
  "name": "Block everything else",
  "priority": 100,
  "condition": { "type": "All", "value": null },
  "action": "Block",
  "enabled": true
}
```

### Block internal network access

```json
{
  "name": "Block RFC1918",
  "priority": 5,
  "condition": { "type": "IpCidr", "value": "10.0.0.0/8" },
  "action": "Block",
  "enabled": true
}
```

## Managing Rules

### Via the Management API

```bash
# List rules
curl -H "Authorization: Bearer $TOKEN" http://127.0.0.1:9090/api/routes

# Create a rule
curl -X POST -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "Block ads",
    "priority": 10,
    "condition": {"type": "DomainMatch", "value": "*.ads.example.com"},
    "action": "Block",
    "enabled": true
  }' \
  http://127.0.0.1:9090/api/routes

# Delete a rule
curl -X DELETE -H "Authorization: Bearer $TOKEN" \
  http://127.0.0.1:9090/api/routes/<rule-id>
```

### Via the Dashboard

Navigate to the **Routing** page in the dashboard to visually manage rules. You can create, edit, toggle, reorder, and delete rules without touching the API directly.

## Behavior Notes

- Domain matching only applies to `Connect` commands with domain-type addresses. IP addresses are not reverse-resolved.
- `DomainMatch` uses simple glob matching: `*.example.com` matches `sub.example.com` and `example.com`.
- `IpCidr` currently supports IPv4 only.
- Rules are stored in memory. They are cleared on server restart. Persistence to a file is planned for a future release.
