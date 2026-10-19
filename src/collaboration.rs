use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    sync::{Arc, RwLock},
    time::{SystemTime, UNIX_EPOCH},
};

use axum::{
    Json,
    extract::{Path as AxumPath, Query, State},
    http::StatusCode,
};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::{ApiResult, AppError, AppState, private_file};

const MAX_TEXT_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentRegistration {
    pub agent_id: String,
    pub name: String,
    #[serde(default)]
    pub platform: String,
    #[serde(default)]
    pub capabilities: Vec<String>,
    #[serde(default)]
    pub session_id: String,
    pub status: String,
    pub created_at: u64,
    pub updated_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Membership {
    pub agent_id: String,
    pub role: String,
    pub status: String,
    pub joined_at: u64,
    #[serde(default)]
    pub last_read_sequence: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscussionGroup {
    pub group_id: String,
    pub project_id: String,
    pub name: String,
    #[serde(default)]
    pub topic: String,
    pub owner_agent_id: String,
    pub status: String,
    pub members: Vec<Membership>,
    pub last_sequence: u64,
    pub created_at: u64,
    pub updated_at: u64,
    #[serde(default)]
    pub projection_degraded: bool,
    #[serde(default)]
    pub projection_error: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupMessage {
    pub message_id: String,
    pub group_id: String,
    pub sequence: u64,
    pub sender_agent_id: String,
    #[serde(default)]
    pub kind: String,
    pub text: String,
    #[serde(default)]
    pub reply_to: String,
    #[serde(default)]
    pub idempotency_key: String,
    pub created_at: u64,
    #[serde(default)]
    pub acknowledgements: HashMap<String, String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct CollaborationData {
    agents: HashMap<String, AgentRegistration>,
    groups: HashMap<String, DiscussionGroup>,
    messages: HashMap<String, Vec<GroupMessage>>,
}

#[derive(Clone)]
pub struct CollaborationManager {
    root: Arc<PathBuf>,
    state_file: Arc<PathBuf>,
    data: Arc<RwLock<CollaborationData>>,
}

#[derive(Debug, Deserialize)]
pub struct RegisterAgentRequest {
    pub agent_id: String,
    pub name: String,
    #[serde(default)]
    pub platform: String,
    #[serde(default)]
    pub capabilities: Vec<String>,
    #[serde(default)]
    pub session_id: String,
}
#[derive(Debug, Deserialize)]
pub struct CreateGroupRequest {
    pub project_id: String,
    pub name: String,
    #[serde(default)]
    pub topic: String,
    pub owner_agent_id: String,
}
#[derive(Debug, Deserialize)]
pub struct JoinGroupRequest {
    pub agent_id: String,
    #[serde(default)]
    pub role: String,
}
#[derive(Debug, Deserialize)]
pub struct LeaveGroupRequest {
    pub agent_id: String,
}
#[derive(Debug, Deserialize)]
pub struct SendMessageRequest {
    pub sender_agent_id: String,
    pub text: String,
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub reply_to: String,
    #[serde(default)]
    pub idempotency_key: String,
}
#[derive(Debug, Deserialize)]
pub struct AckMessageRequest {
    pub agent_id: String,
    pub status: String,
}
#[derive(Debug, Default, Deserialize)]
pub struct MessageQuery {
    #[serde(default)]
    pub after_sequence: u64,
    #[serde(default)]
    pub limit: usize,
}

impl CollaborationManager {
    pub fn new(app_dir: &Path) -> Self {
        let root = app_dir.join(".webclx-collaboration");
        let state_file = root.join("state.json");
        let data = fs::read(&state_file)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        Self {
            root: Arc::new(root),
            state_file: Arc::new(state_file),
            data: Arc::new(RwLock::new(data)),
        }
    }

    fn mutate<T>(
        &self,
        update: impl FnOnce(&mut CollaborationData) -> Result<T, AppError>,
    ) -> Result<T, AppError> {
        let mut data = crate::lock_or_recover!(self.data.write());
        let result = update(&mut data)?;
        fs::create_dir_all(self.root.as_ref()).map_err(internal)?;
        private_file::write_atomic(
            &self.state_file,
            &serde_json::to_vec_pretty(&*data).map_err(internal)?,
        )
        .map_err(internal)?;
        if let Err(error) = self.render_all(&mut data) {
            let message = error.to_string();
            for group in data.groups.values_mut() {
                group.projection_degraded = true;
                group.projection_error = message.clone();
            }
            private_file::write_atomic(
                &self.state_file,
                &serde_json::to_vec_pretty(&*data).map_err(internal)?,
            )
            .map_err(internal)?;
            return Err(error);
        }
        private_file::write_atomic(
            &self.state_file,
            &serde_json::to_vec_pretty(&*data).map_err(internal)?,
        )
        .map_err(internal)?;
        Ok(result)
    }

    fn render_all(&self, data: &mut CollaborationData) -> Result<(), AppError> {
        let ids: Vec<String> = data.groups.keys().cloned().collect();
        for id in ids {
            self.render_group(data, &id)?;
        }
        Ok(())
    }

    fn render_group(&self, data: &mut CollaborationData, group_id: &str) -> Result<(), AppError> {
        let group = data
            .groups
            .get(group_id)
            .cloned()
            .ok_or_else(|| AppError::not_found("讨论组不存在"))?;
        let dir = self
            .root
            .join("projects")
            .join(safe_segment(&group.project_id)?)
            .join("groups")
            .join(safe_segment(group_id)?);
        fs::create_dir_all(&dir).map_err(internal)?;
        let messages = data.messages.get(group_id).cloned().unwrap_or_default();
        let members = group
            .members
            .iter()
            .map(|member| {
                format!(
                    "- `{}` — {} ({})，加入于 {}",
                    member.agent_id, member.role, member.status, member.joined_at
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        let history = messages.iter().map(|message| format!("## #{} · {} · `{}`\n\n- message_id: `{}`\n- kind: `{}`\n- acknowledgements: `{}`\n\n{}\n", message.sequence, message.created_at, message.sender_agent_id, message.message_id, message.kind, serde_json::to_string(&message.acknowledgements).unwrap_or_default(), message.text)).collect::<Vec<_>>().join("\n");
        let header = format!(
            "---\nschema_version: 1\ngroup_id: {}\nproject_id: {}\nstatus: {}\nlast_sequence: {}\nupdated_at: {}\nmanaged_by: webclx-collaboration-api\n---\n\n> 此文档由 webClx collaboration API 管理，只读。\n\n",
            group.group_id, group.project_id, group.status, group.last_sequence, group.updated_at
        );
        let readme = format!(
            "{header}# {}\n\n{}\n\n- 所有者：`{}`\n- 成员数：{}\n- 历史：[history.md](history.md)\n- 成员：[members.md](members.md)\n",
            group.name,
            group.topic,
            group.owner_agent_id,
            group
                .members
                .iter()
                .filter(|m| m.status == "active")
                .count()
        );
        for (name, body) in [
            ("README.md", readme),
            ("members.md", format!("{header}# 成员\n\n{members}\n")),
            ("history.md", format!("{header}# 讨论历史\n\n{history}")),
        ] {
            private_file::write_atomic(&dir.join(name), body.as_bytes()).map_err(internal)?;
        }
        if let Some(target) = data.groups.get_mut(group_id) {
            target.projection_degraded = false;
            target.projection_error.clear();
        }
        Ok(())
    }

    pub fn root(&self) -> &Path {
        self.root.as_ref()
    }
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
fn id(prefix: &str) -> String {
    let mut bytes = [0u8; 8];
    rand::thread_rng().fill_bytes(&mut bytes);
    format!("{prefix}_{}_{:x}", now(), u64::from_le_bytes(bytes))
}
fn internal(error: impl std::fmt::Display) -> AppError {
    AppError::internal(format!("协作存储失败: {error}"))
}
fn validate_id(value: &str, field: &str) -> Result<String, AppError> {
    let value = value.trim();
    if value.is_empty()
        || value.len() > 96
        || !value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
    {
        return Err(AppError::bad_request(format!("{field} 格式无效")));
    }
    Ok(value.to_string())
}
fn safe_segment(value: &str) -> Result<String, AppError> {
    validate_id(value, "路径标识")
}
fn validate_text(value: &str, field: &str) -> Result<String, AppError> {
    let value = value.trim();
    if value.is_empty() || value.len() > MAX_TEXT_BYTES {
        return Err(AppError::bad_request(format!("{field} 为空或过长")));
    }
    Ok(value.to_string())
}
fn active_member(group: &DiscussionGroup, agent_id: &str) -> bool {
    group
        .members
        .iter()
        .any(|m| m.agent_id == agent_id && m.status == "active" && m.role != "observer")
}

pub async fn list_agents(State(state): State<AppState>) -> Json<Value> {
    let data = crate::lock_or_recover!(state.collaboration_manager.data.read());
    Json(json!({"agents": data.agents.values().collect::<Vec<_>>() }))
}
pub async fn register_agent(
    State(state): State<AppState>,
    Json(payload): Json<RegisterAgentRequest>,
) -> ApiResult<Json<AgentRegistration>> {
    let agent_id = validate_id(&payload.agent_id, "agent_id")?;
    let name = validate_text(&payload.name, "name")?;
    let manager = state.collaboration_manager.clone();
    let agent = manager.mutate(|data| {
        let created_at = data
            .agents
            .get(&agent_id)
            .map(|a| a.created_at)
            .unwrap_or_else(now);
        let agent = AgentRegistration {
            agent_id: agent_id.clone(),
            name,
            platform: payload.platform.trim().to_string(),
            capabilities: payload.capabilities,
            session_id: payload.session_id.trim().to_string(),
            status: "active".into(),
            created_at,
            updated_at: now(),
        };
        data.agents.insert(agent_id, agent.clone());
        Ok(agent)
    })?;
    Ok(Json(agent))
}
pub async fn list_groups(State(state): State<AppState>) -> Json<Value> {
    let data = crate::lock_or_recover!(state.collaboration_manager.data.read());
    Json(json!({"groups": data.groups.values().collect::<Vec<_>>() }))
}
pub async fn create_group(
    State(state): State<AppState>,
    Json(payload): Json<CreateGroupRequest>,
) -> ApiResult<(StatusCode, Json<DiscussionGroup>)> {
    let project_id = validate_id(&payload.project_id, "project_id")?;
    let owner = validate_id(&payload.owner_agent_id, "owner_agent_id")?;
    let name = validate_text(&payload.name, "name")?;
    let manager = state.collaboration_manager.clone();
    let group = manager.mutate(|data| {
        if !data.agents.contains_key(&owner) {
            return Err(AppError::bad_request("owner_agent_id 未注册"));
        }
        let timestamp = now();
        let group = DiscussionGroup {
            group_id: id("grp"),
            project_id,
            name,
            topic: payload.topic.trim().to_string(),
            owner_agent_id: owner.clone(),
            status: "active".into(),
            members: vec![Membership {
                agent_id: owner,
                role: "owner".into(),
                status: "active".into(),
                joined_at: timestamp,
                last_read_sequence: 0,
            }],
            last_sequence: 0,
            created_at: timestamp,
            updated_at: timestamp,
            projection_degraded: false,
            projection_error: String::new(),
        };
        data.messages.insert(group.group_id.clone(), Vec::new());
        data.groups.insert(group.group_id.clone(), group.clone());
        Ok(group)
    })?;
    Ok((StatusCode::CREATED, Json(group)))
}
pub async fn get_group(
    State(state): State<AppState>,
    AxumPath(group_id): AxumPath<String>,
) -> ApiResult<Json<Value>> {
    let data = crate::lock_or_recover!(state.collaboration_manager.data.read());
    let group = data
        .groups
        .get(&group_id)
        .ok_or_else(|| AppError::not_found("讨论组不存在"))?;
    Ok(Json(
        json!({"group": group, "markdown_path": state.collaboration_manager.root().join("projects").join(&group.project_id).join("groups").join(&group.group_id).join("history.md")}),
    ))
}
pub async fn join_group(
    State(state): State<AppState>,
    AxumPath(group_id): AxumPath<String>,
    Json(payload): Json<JoinGroupRequest>,
) -> ApiResult<Json<DiscussionGroup>> {
    let agent_id = validate_id(&payload.agent_id, "agent_id")?;
    let role = match payload.role.trim() {
        "observer" => "observer",
        _ => "member",
    }
    .to_string();
    let manager = state.collaboration_manager.clone();
    let group = manager.mutate(|data| {
        if !data.agents.contains_key(&agent_id) {
            return Err(AppError::bad_request("agent_id 未注册"));
        }
        let group = data
            .groups
            .get_mut(&group_id)
            .ok_or_else(|| AppError::not_found("讨论组不存在"))?;
        if let Some(member) = group.members.iter_mut().find(|m| m.agent_id == agent_id) {
            member.status = "active".into();
            member.role = role;
        } else {
            group.members.push(Membership {
                agent_id: agent_id.clone(),
                role,
                status: "active".into(),
                joined_at: now(),
                last_read_sequence: 0,
            });
        }
        group.updated_at = now();
        Ok(group.clone())
    })?;
    Ok(Json(group))
}
pub async fn leave_group(
    State(state): State<AppState>,
    AxumPath(group_id): AxumPath<String>,
    Json(payload): Json<LeaveGroupRequest>,
) -> ApiResult<Json<DiscussionGroup>> {
    let manager = state.collaboration_manager.clone();
    let group = manager.mutate(|data| {
        let group = data
            .groups
            .get_mut(&group_id)
            .ok_or_else(|| AppError::not_found("讨论组不存在"))?;
        if group.owner_agent_id == payload.agent_id {
            return Err(AppError::bad_request("所有者不能直接退出讨论组"));
        }
        let member = group
            .members
            .iter_mut()
            .find(|m| m.agent_id == payload.agent_id)
            .ok_or_else(|| AppError::not_found("成员不存在"))?;
        member.status = "left".into();
        group.updated_at = now();
        Ok(group.clone())
    })?;
    Ok(Json(group))
}
pub async fn list_messages(
    State(state): State<AppState>,
    AxumPath(group_id): AxumPath<String>,
    Query(query): Query<MessageQuery>,
) -> ApiResult<Json<Value>> {
    let data = crate::lock_or_recover!(state.collaboration_manager.data.read());
    if !data.groups.contains_key(&group_id) {
        return Err(AppError::not_found("讨论组不存在"));
    }
    let limit = if query.limit == 0 {
        200
    } else {
        query.limit.min(500)
    };
    let messages = data
        .messages
        .get(&group_id)
        .into_iter()
        .flatten()
        .filter(|m| m.sequence > query.after_sequence)
        .take(limit)
        .cloned()
        .collect::<Vec<_>>();
    Ok(Json(json!({"messages": messages})))
}
pub async fn send_message(
    State(state): State<AppState>,
    AxumPath(group_id): AxumPath<String>,
    Json(payload): Json<SendMessageRequest>,
) -> ApiResult<(StatusCode, Json<GroupMessage>)> {
    let sender = validate_id(&payload.sender_agent_id, "sender_agent_id")?;
    let text = validate_text(&payload.text, "text")?;
    let manager = state.collaboration_manager.clone();
    let message = manager.mutate(|data| {
        let group = data
            .groups
            .get_mut(&group_id)
            .ok_or_else(|| AppError::not_found("讨论组不存在"))?;
        if !active_member(group, &sender) {
            return Err(AppError::bad_request("发送者不是可发言的 active member"));
        }
        let messages = data.messages.entry(group_id.clone()).or_default();
        if !payload.idempotency_key.trim().is_empty() {
            if let Some(existing) = messages.iter().find(|m| {
                m.sender_agent_id == sender && m.idempotency_key == payload.idempotency_key.trim()
            }) {
                return Ok(existing.clone());
            }
        }
        group.last_sequence += 1;
        group.updated_at = now();
        let message = GroupMessage {
            message_id: id("msg"),
            group_id: group_id.clone(),
            sequence: group.last_sequence,
            sender_agent_id: sender,
            kind: match payload.kind.trim() {
                "task" | "reply" | "status" | "handoff" | "error" => {
                    payload.kind.trim().to_string()
                }
                _ => "speech".into(),
            },
            text,
            reply_to: payload.reply_to.trim().to_string(),
            idempotency_key: payload.idempotency_key.trim().to_string(),
            created_at: now(),
            acknowledgements: HashMap::new(),
        };
        messages.push(message.clone());
        Ok(message)
    })?;
    Ok((StatusCode::CREATED, Json(message)))
}
pub async fn ack_message(
    State(state): State<AppState>,
    AxumPath(message_id): AxumPath<String>,
    Json(payload): Json<AckMessageRequest>,
) -> ApiResult<Json<GroupMessage>> {
    let allowed = ["accepted", "completed", "rejected", "failed", "read"];
    if !allowed.contains(&payload.status.as_str()) {
        return Err(AppError::bad_request("ack status 无效"));
    }
    let manager = state.collaboration_manager.clone();
    let message = manager.mutate(|data| {
        for messages in data.messages.values_mut() {
            if let Some(message) = messages.iter_mut().find(|m| m.message_id == message_id) {
                let group = data
                    .groups
                    .get(&message.group_id)
                    .ok_or_else(|| AppError::not_found("讨论组不存在"))?;
                if !group
                    .members
                    .iter()
                    .any(|m| m.agent_id == payload.agent_id && m.status == "active")
                {
                    return Err(AppError::bad_request("agent 不是 active member"));
                }
                message
                    .acknowledgements
                    .insert(payload.agent_id.clone(), payload.status.clone());
                return Ok(message.clone());
            }
        }
        Err(AppError::not_found("消息不存在"))
    })?;
    Ok(Json(message))
}
pub async fn markdown_history(
    State(state): State<AppState>,
    AxumPath(group_id): AxumPath<String>,
) -> ApiResult<String> {
    let data = crate::lock_or_recover!(state.collaboration_manager.data.read());
    let group = data
        .groups
        .get(&group_id)
        .ok_or_else(|| AppError::not_found("讨论组不存在"))?;
    let path = state
        .collaboration_manager
        .root()
        .join("projects")
        .join(&group.project_id)
        .join("groups")
        .join(group_id)
        .join("history.md");
    fs::read_to_string(path).map_err(internal)
}
pub async fn reconcile(
    State(state): State<AppState>,
    AxumPath(group_id): AxumPath<String>,
) -> ApiResult<Json<Value>> {
    let manager = state.collaboration_manager.clone();
    manager.mutate(|data| {
        if !data.groups.contains_key(&group_id) {
            return Err(AppError::not_found("讨论组不存在"));
        }
        Ok(())
    })?;
    Ok(Json(json!({"ok": true, "group_id": group_id})))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn identifiers_reject_paths() {
        assert!(validate_id("../bad", "id").is_err());
        assert!(validate_id("agent-one", "id").is_ok());
    }
    #[test]
    fn active_observer_cannot_speak() {
        let group = DiscussionGroup {
            group_id: "g".into(),
            project_id: "p".into(),
            name: "n".into(),
            topic: String::new(),
            owner_agent_id: "a".into(),
            status: "active".into(),
            members: vec![Membership {
                agent_id: "o".into(),
                role: "observer".into(),
                status: "active".into(),
                joined_at: 0,
                last_read_sequence: 0,
            }],
            last_sequence: 0,
            created_at: 0,
            updated_at: 0,
            projection_degraded: false,
            projection_error: String::new(),
        };
        assert!(!active_member(&group, "o"));
    }
}
