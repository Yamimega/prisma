use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use serde::Deserialize;
use uuid::Uuid;

use prisma_core::config::server::{RoutingRule, RuleAction, RuleCondition};

use crate::MgmtState;

#[derive(Deserialize)]
pub struct CreateRouteRequest {
    pub name: String,
    pub priority: u32,
    pub condition: RuleCondition,
    pub action: RuleAction,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_true() -> bool {
    true
}

pub async fn list(State(state): State<MgmtState>) -> Json<Vec<RoutingRule>> {
    let rules = state.routing_rules.read().await;
    Json(rules.clone())
}

pub async fn create(
    State(state): State<MgmtState>,
    Json(req): Json<CreateRouteRequest>,
) -> Result<Json<RoutingRule>, StatusCode> {
    let rule = RoutingRule {
        id: Uuid::new_v4(),
        name: req.name,
        priority: req.priority,
        condition: req.condition,
        action: req.action,
        enabled: req.enabled,
    };

    let json = Json(rule.clone());
    state.routing_rules.write().await.push(rule);
    Ok(json)
}

pub async fn update(
    State(state): State<MgmtState>,
    Path(id): Path<Uuid>,
    Json(req): Json<CreateRouteRequest>,
) -> StatusCode {
    let mut rules = state.routing_rules.write().await;
    if let Some(rule) = rules.iter_mut().find(|r| r.id == id) {
        rule.name = req.name;
        rule.priority = req.priority;
        rule.condition = req.condition;
        rule.action = req.action;
        rule.enabled = req.enabled;
        StatusCode::OK
    } else {
        StatusCode::NOT_FOUND
    }
}

pub async fn remove(State(state): State<MgmtState>, Path(id): Path<Uuid>) -> StatusCode {
    let mut rules = state.routing_rules.write().await;
    let len_before = rules.len();
    rules.retain(|r| r.id != id);
    if rules.len() < len_before {
        StatusCode::OK
    } else {
        StatusCode::NOT_FOUND
    }
}
