use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use serde::Serialize;
use std::fs;
use std::path::PathBuf;

use crate::MgmtState;

#[derive(Serialize)]
pub struct BackupInfo {
    pub name: String,
    pub timestamp: String,
    pub size: u64,
}

#[derive(Serialize)]
pub struct BackupDiff {
    pub changes: Vec<DiffChange>,
}

#[derive(Serialize)]
pub struct DiffChange {
    pub tag: String, // "equal", "insert", "delete"
    pub old_value: Option<String>,
    pub new_value: Option<String>,
}

fn backup_dir(state: &MgmtState) -> Result<PathBuf, StatusCode> {
    let config_path = state
        .config_path
        .as_ref()
        .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
    let dir = config_path
        .parent()
        .unwrap_or(std::path::Path::new("."))
        .join("backups");
    fs::create_dir_all(&dir).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(dir)
}

fn validated_backup_path(dir: &std::path::Path, name: &str) -> Result<PathBuf, StatusCode> {
    let name = std::path::Path::new(name)
        .file_name()
        .ok_or(StatusCode::BAD_REQUEST)?
        .to_str()
        .ok_or(StatusCode::BAD_REQUEST)?;
    let path = dir.join(name);
    let canonical_dir = dir
        .canonicalize()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let canonical_path = path.canonicalize().map_err(|_| StatusCode::NOT_FOUND)?;
    if !canonical_path.starts_with(&canonical_dir) {
        return Err(StatusCode::BAD_REQUEST);
    }
    Ok(path)
}

pub async fn auto_backup(state: &MgmtState) -> Result<(), StatusCode> {
    let config_path = state
        .config_path
        .as_ref()
        .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
    let dir = backup_dir(state)?;
    let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S");
    let backup_name = format!("auto_{}.toml", timestamp);
    let dest = dir.join(&backup_name);
    fs::copy(config_path, dest).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Keep only last 50 auto-backups
    let mut entries: Vec<_> = fs::read_dir(&dir)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().starts_with("auto_"))
        .collect();
    entries.sort_by_key(|e| e.file_name());
    if entries.len() > 50 {
        for entry in &entries[..entries.len() - 50] {
            let _ = fs::remove_file(entry.path());
        }
    }

    Ok(())
}

/// GET /api/config/backups
pub async fn list_backups(
    State(state): State<MgmtState>,
) -> Result<Json<Vec<BackupInfo>>, StatusCode> {
    let dir = backup_dir(&state)?;
    let mut backups = Vec::new();

    let entries = fs::read_dir(&dir).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    for entry in entries.filter_map(|e| e.ok()) {
        let meta = entry
            .metadata()
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        if meta.is_file() {
            let name = entry.file_name().to_string_lossy().to_string();
            let modified = meta
                .modified()
                .map(|t| chrono::DateTime::<chrono::Utc>::from(t).to_rfc3339())
                .unwrap_or_default();
            backups.push(BackupInfo {
                name,
                timestamp: modified,
                size: meta.len(),
            });
        }
    }

    backups.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
    Ok(Json(backups))
}

/// POST /api/config/backup
pub async fn create_backup(State(state): State<MgmtState>) -> Result<Json<BackupInfo>, StatusCode> {
    let config_path = state
        .config_path
        .as_ref()
        .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
    let dir = backup_dir(&state)?;
    let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S");
    let backup_name = format!("manual_{}.toml", timestamp);
    let dest = dir.join(&backup_name);
    fs::copy(config_path, &dest).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let meta = fs::metadata(&dest).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(BackupInfo {
        name: backup_name,
        timestamp: chrono::Utc::now().to_rfc3339(),
        size: meta.len(),
    }))
}

/// GET /api/config/backups/{name}
pub async fn get_backup(
    State(state): State<MgmtState>,
    Path(name): Path<String>,
) -> Result<String, StatusCode> {
    let dir = backup_dir(&state)?;
    let path = validated_backup_path(&dir, &name)?;
    fs::read_to_string(path).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

/// POST /api/config/backups/{name}/restore
pub async fn restore_backup(
    State(state): State<MgmtState>,
    Path(name): Path<String>,
) -> Result<StatusCode, StatusCode> {
    let dir = backup_dir(&state)?;
    let backup_path = validated_backup_path(&dir, &name)?;
    let config_path = state
        .config_path
        .as_ref()
        .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;

    // Auto-backup current before restore
    auto_backup(&state).await?;

    fs::copy(&backup_path, config_path).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Reload config
    let content = fs::read_to_string(config_path).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let new_config: prisma_core::config::server::ServerConfig =
        toml::from_str(&content).map_err(|_| StatusCode::BAD_REQUEST)?;
    *state.config.write().await = new_config;

    Ok(StatusCode::OK)
}

/// DELETE /api/config/backups/{name}
pub async fn delete_backup(
    State(state): State<MgmtState>,
    Path(name): Path<String>,
) -> Result<StatusCode, StatusCode> {
    let dir = backup_dir(&state)?;
    let path = validated_backup_path(&dir, &name)?;
    fs::remove_file(path).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(StatusCode::OK)
}

/// GET /api/config/backups/{name}/diff
pub async fn diff_backup(
    State(state): State<MgmtState>,
    Path(name): Path<String>,
) -> Result<Json<BackupDiff>, StatusCode> {
    let dir = backup_dir(&state)?;
    let backup_path = validated_backup_path(&dir, &name)?;

    let config_path = state
        .config_path
        .as_ref()
        .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
    let current = fs::read_to_string(config_path).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let backup = fs::read_to_string(&backup_path).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    use similar::{ChangeTag, TextDiff};
    let diff = TextDiff::from_lines(&backup, &current);
    let changes = diff
        .iter_all_changes()
        .map(|change| {
            let value = change.value().to_string();
            let (tag, old_value, new_value) = match change.tag() {
                ChangeTag::Equal => ("equal", None, None),
                ChangeTag::Delete => ("delete", Some(value), None),
                ChangeTag::Insert => ("insert", None, Some(value)),
            };
            DiffChange {
                tag: tag.to_string(),
                old_value,
                new_value,
            }
        })
        .collect();

    Ok(Json(BackupDiff { changes }))
}
