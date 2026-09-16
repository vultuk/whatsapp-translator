//! Optional, persisted topic views. A single background worker never delays message delivery.
use crate::{
    storage::StoredMessage,
    translation::{OpenAiApiFailure, OpenAiOutputFailure},
    web::{AppState, WebSocketEvent},
};
use anyhow::{ensure, Context, Result};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{collections::HashSet, sync::Arc, time::Duration};

pub const INITIAL_MESSAGES: usize = 200;
// Keep both tasks small enough to leave room for reasoning and complete JSON output.
pub const BATCH_SIZE: usize = 8;
pub const LABEL_BATCH_SIZE: usize = 8;
pub const IMPORT_DAYS: i64 = 7;

#[derive(Debug)]
struct TopicStage(&'static str);
impl std::fmt::Display for TopicStage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0)
    }
}
impl std::error::Error for TopicStage {}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TopicImportSummary {
    pub days: i64,
    pub chat_count: i64,
    pub message_count: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatTopic {
    pub id: String,
    pub category_id: String,
    pub contact_id: String,
    pub contact_name: String,
    pub title: String,
    pub message_count: i64,
    pub last_message_time: i64,
}

pub fn category_id(name_key: &str) -> String {
    format!("category:{}", URL_SAFE_NO_PAD.encode(name_key))
}

pub fn category_key(id: &str) -> Option<String> {
    String::from_utf8(URL_SAFE_NO_PAD.decode(id.strip_prefix("category:")?).ok()?).ok()
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TopicSetting {
    pub contact_id: String,
    pub enabled: bool,
    pub pending_count: i64,
    pub failed_count: i64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MessageTopic {
    pub message_id: String,
    pub contact_id: String,
    pub revision: i64,
    pub title: Option<String>,
    pub state: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MessageTopicsRequest {
    message_ids: Vec<String>,
}

pub async fn message_topics(
    State(state): State<Arc<AppState>>,
    Json(request): Json<MessageTopicsRequest>,
) -> Response {
    if request.message_ids.len() > 200 || request.message_ids.iter().any(|id| id.len() > 512) {
        return (
            StatusCode::BAD_REQUEST,
            "Request at most 200 message topics",
        )
            .into_response();
    }
    match state.store.message_topics(&request.message_ids) {
        Ok(topics) => Json(json!({"topics":topics})).into_response(),
        Err(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Could not load message topics",
        )
            .into_response(),
    }
}

#[derive(Clone, Debug)]
pub struct TopicBatch {
    pub contact_id: String,
    pub epoch: String,
    pub messages: Vec<StoredMessage>,
}

#[derive(Clone, Debug)]
pub struct TopicLabel {
    pub id: String,
    pub contact_id: String,
    pub epoch: String,
    pub title: String,
}

pub fn valid_title(title: &str) -> bool {
    !title.trim().is_empty()
        && title.chars().count() <= 32
        && title.split_whitespace().count() <= 3
        && !title.chars().any(char::is_control)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TopicAssignment {
    pub message_id: String,
    pub topic: String,
}

pub fn parse_assignments(text: &str, batch: &TopicBatch) -> Result<Vec<TopicAssignment>> {
    parse_title_assignments(text, batch.messages.iter().map(|m| m.id.as_str()))
}

pub fn parse_title_assignments<'a>(
    text: &str,
    ids: impl Iterator<Item = &'a str>,
) -> Result<Vec<TopicAssignment>> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Output {
        assignments: Vec<TopicAssignment>,
    }
    let result: Output = serde_json::from_str(text).context("Invalid topic response")?;
    let expected: HashSet<&str> = ids.collect();
    ensure!(
        result.assignments.len() == expected.len(),
        "Incomplete topic response"
    );
    let mut seen = HashSet::new();
    for assignment in &result.assignments {
        ensure!(
            expected.contains(assignment.message_id.as_str())
                && seen.insert(&assignment.message_id),
            "Invalid topic message ID"
        );
        ensure!(valid_title(&assignment.topic), "Invalid topic title");
    }
    Ok(result.assignments)
}

pub fn revision(message: &StoredMessage) -> i64 {
    serde_json::from_str::<Value>(&message.content_json)
        .ok()
        .and_then(|v| v.get("edited_at_ms").and_then(Value::as_i64))
        .unwrap_or(0)
}

pub fn start(state: Arc<AppState>) {
    if state.store.log_topic_queue_health().is_err() {
        tracing::warn!("Could not read topic queue health");
    }
    if state.translator.is_none() {
        return;
    }
    tokio::spawn(async move {
        loop {
            // Also batches bursts of messages into a single request.
            tokio::time::sleep(Duration::from_secs(5)).await;
            // Simplify a bounded batch of existing labels, retaining all assignments until
            // a complete response is validated. New messages still get a turn each cycle.
            match state.store.next_topic_labels() {
                Ok(labels) if !labels.is_empty() => {
                    match tokio::time::timeout(
                        Duration::from_secs(120),
                        simplify_labels(&state, &labels),
                    )
                    .await
                    {
                        Ok(Ok(count)) => {
                            tracing::info!(topics = count, "Topic simplification completed")
                        }
                        failure => {
                            let reason = match &failure {
                                Ok(Err(error)) => error
                                    .downcast_ref::<OpenAiOutputFailure>()
                                    .map(|e| e.0)
                                    .or_else(|| {
                                        error.downcast_ref::<OpenAiApiFailure>().map(|e| e.reason)
                                    })
                                    .unwrap_or("operation_failed"),
                                _ => "timeout",
                            };
                            if state.store.retry_topic_labels(&labels).is_ok() {
                                tracing::warn!(
                                    topics = labels.len(),
                                    reason,
                                    "Topic simplification deferred; retry state saved"
                                );
                            } else {
                                tracing::error!("Could not save topic simplification retry state");
                            }
                        }
                    }
                    let _ = state.broadcast_tx.send(WebSocketEvent::TopicsUpdated);
                }
                Ok(_) => {}
                Err(_) => tracing::warn!("Could not read topic simplification queue"),
            }
            match state.store.next_topic_batch() {
                Ok(Some(batch)) => {
                    let result =
                        tokio::time::timeout(Duration::from_secs(120), classify(&state, &batch))
                            .await;
                    match result {
                        Ok(Ok(assigned)) => {
                            tracing::info!(messages = assigned, "Topic classification completed")
                        }
                        failure => {
                            // Only fixed labels and counts are logged, never source text or model output.
                            let (stage, reason, http_status) = match &failure {
                                Ok(Err(error)) => {
                                    let stage = error
                                        .downcast_ref::<TopicStage>()
                                        .map(|s| s.0)
                                        .unwrap_or("internal");
                                    let upstream = error.downcast_ref::<OpenAiApiFailure>();
                                    let transport = error.downcast_ref::<reqwest::Error>();
                                    let output = error.downcast_ref::<OpenAiOutputFailure>();
                                    (
                                        stage,
                                        upstream.map(|e| e.reason).unwrap_or_else(|| {
                                            if let Some(output) = output {
                                                output.0
                                            } else if transport
                                                .is_some_and(reqwest::Error::is_timeout)
                                            {
                                                "request_timeout"
                                            } else if transport
                                                .is_some_and(reqwest::Error::is_connect)
                                            {
                                                "connection_failed"
                                            } else {
                                                "operation_failed"
                                            }
                                        }),
                                        upstream.map(|e| e.status),
                                    )
                                }
                                _ => ("ai_request", "timeout", None),
                            };
                            match state.store.retry_topic_batch(&batch) {
                                Ok(()) => tracing::warn!(
                                    stage,
                                    reason,
                                    http_status,
                                    messages = batch.messages.len(),
                                    "Topic classification deferred; retry state saved"
                                ),
                                Err(_) => tracing::error!(
                                    stage,
                                    reason,
                                    http_status,
                                    "Topic classification failed; could not save retry state"
                                ),
                            }
                        }
                    }
                    let _ = state.broadcast_tx.send(WebSocketEvent::TopicsUpdated);
                }
                Ok(None) => {}
                Err(_) => tracing::warn!("Could not read topic queue"),
            }
        }
    });
}

async fn simplify_labels(state: &AppState, labels: &[TopicLabel]) -> Result<usize> {
    let account_epoch = state.voice_epoch.load(std::sync::atomic::Ordering::SeqCst);
    let translator = state.translator.as_ref().context("AI is unavailable")?;
    let existing = state.store.topic_names("")?;
    let input: Vec<Value> = labels
        .iter()
        .map(|label| json!({"messageId":label.id,"title":label.title}))
        .collect();
    let (output, usage) = translator.simplify_topic_labels(json!({"labelLanguage":translator.default_language(),"existingTopics":existing,"labels":input})).await?;
    if account_epoch != state.voice_epoch.load(std::sync::atomic::Ordering::SeqCst) {
        return Ok(0);
    }
    state
        .store
        .record_usage(None, None, &usage, "topic_simplification")?;
    let assignments =
        parse_title_assignments(&output, labels.iter().map(|label| label.id.as_str()))?;
    state.store.finish_topic_labels(labels, &assignments)
}

async fn classify(state: &AppState, batch: &TopicBatch) -> Result<usize> {
    let account_epoch = state.voice_epoch.load(std::sync::atomic::Ordering::SeqCst);
    let translator = state.translator.as_ref().context("AI is unavailable")?;
    let existing = state
        .store
        .topic_names(&batch.contact_id)
        .context(TopicStage("read_topics"))?;
    let context = state
        .store
        .topic_context(&batch.contact_id)
        .context(TopicStage("read_context"))?;
    let messages: Vec<Value> = batch.messages.iter().map(|m| {
        let content: Value = serde_json::from_str(&m.content_json).unwrap_or(Value::Null);
        json!({"messageId":m.id,"text":m.original_text.as_deref().unwrap_or("").chars().take(2000).collect::<String>(),
               "replyTo":content.get("reply_context").or_else(|| content.get("reply_to")).or_else(|| content.get("replyTo")),"timestamp":m.timestamp})
    }).collect();
    let (output, usage) = translator.classify_topics(json!({
        "labelLanguage":translator.default_language(),"existingTopics":existing,"context":context,"messages":messages
    })).await.context(TopicStage("ai_request"))?;
    if account_epoch != state.voice_epoch.load(std::sync::atomic::Ordering::SeqCst) {
        return Ok(0);
    }
    // Record real usage even if the response fails validation or the chat was disabled in flight.
    state
        .store
        .record_usage(
            Some(&batch.contact_id),
            None,
            &usage,
            "topic_classification",
        )
        .context(TopicStage("record_usage"))?;
    let assignments = parse_assignments(&output, batch).context(TopicStage("validate_response"))?;
    state
        .store
        .finish_topic_batch(batch, &assignments)
        .context(TopicStage("save_assignments"))
}

pub async fn catalog(State(state): State<Arc<AppState>>) -> Response {
    match (state.store.list_topics(), state.store.topic_settings()) {
        (Ok(topics), Ok(settings)) => Json(
            json!({"topics":topics,"settings":settings,"available":state.translator.is_some()}),
        )
        .into_response(),
        _ => (StatusCode::INTERNAL_SERVER_ERROR, "Could not load topics").into_response(),
    }
}

pub async fn import_preview(State(state): State<Arc<AppState>>) -> Response {
    match state
        .store
        .topic_import_preview(chrono::Utc::now().timestamp_millis())
    {
        Ok(summary) => Json(summary).into_response(),
        Err(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Could not preview topic import",
        )
            .into_response(),
    }
}

pub async fn import_recent(State(state): State<Arc<AppState>>) -> Response {
    if state.translator.is_none() {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            "Configure OpenAI before importing topics",
        )
            .into_response();
    }
    match state
        .store
        .import_recent_topics(chrono::Utc::now().timestamp_millis())
    {
        Ok(summary) => {
            let _ = state.broadcast_tx.send(WebSocketEvent::TopicsUpdated);
            Json(summary).into_response()
        }
        Err(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Could not start topic import",
        )
            .into_response(),
    }
}

#[derive(Deserialize)]
pub struct UpdateSetting {
    enabled: bool,
}

pub async fn update_setting(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(input): Json<UpdateSetting>,
) -> Response {
    let id = crate::identity::canonical_chat_id(&id).into_owned();
    if !matches!(state.store.get_contact(&id), Ok(Some(_))) || id == "status@broadcast" {
        return (StatusCode::NOT_FOUND, "Conversation not found").into_response();
    }
    if input.enabled && state.translator.is_none() {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            "Configure OpenAI before enabling topics",
        )
            .into_response();
    }
    match state.store.set_topics_enabled(&id, input.enabled) {
        Ok(()) => {
            let _ = state.broadcast_tx.send(WebSocketEvent::TopicsUpdated);
            catalog(State(state)).await
        }
        Err(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Could not save topic settings",
        )
            .into_response(),
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Page {
    limit: Option<u32>,
    before: Option<i64>,
    #[serde(alias = "before_id")]
    before_id: Option<String>,
}

pub async fn messages(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Query(page): Query<Page>,
) -> Response {
    let limit = page.limit.unwrap_or(50).clamp(1, 100);
    match state
        .store
        .topic_messages(&id, limit + 1, page.before, page.before_id.as_deref())
    {
        Ok(Some(mut messages)) => {
            let has_more = messages.len() > limit as usize;
            if has_more {
                messages.remove(0);
            }
            crate::web::present_message_page(&state.store, messages, has_more)
        }
        Ok(None) => (StatusCode::NOT_FOUND, "This topic is no longer available").into_response(),
        Err(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Could not load topic messages",
        )
            .into_response(),
    }
}
