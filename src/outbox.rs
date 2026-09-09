//! Durable request identities: retries inspect the original operation and never repeat an uncertain write.
use crate::{storage::OutboxEntry, web::AppState};
use axum::{
    body::{to_bytes, Body},
    extract::{Path, Request, State},
    http::{HeaderValue, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use sha2::{Digest, Sha256};
use std::{sync::Arc, time::Duration};

tokio::task_local! { pub static OPERATION_ID: String; }

pub fn is_send(path: &str) -> bool {
    matches!(
        path,
        "/api/send"
            | "/api/send-media"
            | "/api/send-image"
            | "/api/send-images"
            | "/api/react"
            | "/api/voice/send"
    ) || (path.starts_with("/api/photo-albums/") && path.ends_with("/send"))
}

fn render(entry: &OutboxEntry) -> Response {
    let mut response = if matches!(entry.state.as_str(), "sending" | "uncertain") {
        (StatusCode::CONFLICT,Json(serde_json::json!({"error":"Delivery is uncertain or still awaiting confirmation. Retry this same action to check its result; it will not send twice.","deliveryState":entry.state,"operationId":entry.id}))).into_response()
    } else {
        (
            StatusCode::from_u16(entry.status_code.unwrap_or(500))
                .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
            [("content-type", "application/json")],
            entry.response.clone().unwrap_or_else(|| "{}".into()),
        )
            .into_response()
    };
    response
        .headers_mut()
        .insert("x-operation-id", HeaderValue::from_str(&entry.id).unwrap());
    response.headers_mut().insert(
        "x-delivery-state",
        HeaderValue::from_str(&entry.state).unwrap(),
    );
    response
}

pub async fn status(State(state): State<Arc<AppState>>, Path(id): Path<String>) -> Response {
    match state.store.outbox_entry(&id) {
        Ok(Some(entry)) => render(&entry),
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

pub async fn dispatch(state: Arc<AppState>, req: Request, next: Next) -> Response {
    if req.method() != "POST" || !is_send(req.uri().path()) {
        return next.run(req).await;
    }
    let id = match req.headers().get("idempotency-key") {
        Some(value) => match value.to_str().ok().filter(|v| {
            !v.is_empty()
                && v.len() <= 128
                && v.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        }) {
            Some(value) => value.to_owned(),
            None => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({"error":"Invalid Idempotency-Key"})),
                )
                    .into_response()
            }
        },
        // Preserve compatibility with existing installed clients; current clients always provide a durable key.
        None => uuid::Uuid::new_v4().to_string(),
    };
    let path = req.uri().path().to_owned();
    let (parts, body) = req.into_parts();
    let bytes = match to_bytes(body, 90 * 1024 * 1024).await {
        Ok(bytes) => bytes,
        Err(_) => return StatusCode::PAYLOAD_TOO_LARGE.into_response(),
    };
    let mut digest = Sha256::new();
    digest.update(path.as_bytes());
    digest.update([0]);
    let canonical = serde_json::from_slice::<serde_json::Value>(&bytes)
        .ok()
        .and_then(|value| serde_json::to_vec(&value).ok());
    digest.update(canonical.as_deref().unwrap_or(&bytes));
    let fingerprint = format!("{:x}", digest.finalize());
    match state.store.claim_outbox(&id,&fingerprint,&path) {
        Ok(false) => return match state.store.outbox_entry(&id) {
            Ok(Some(entry)) if entry.fingerprint==fingerprint => render(&entry),
            Ok(Some(_)) => (StatusCode::CONFLICT,Json(serde_json::json!({"error":"That retry identifier belongs to different content."}))).into_response(),
            _ => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
        },
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
        Ok(true) => {},
    }
    let request = Request::from_parts(parts, Body::from(bytes));
    let task_state = state.clone();
    let task_id = id.clone();
    // The server owns execution even if the HTTP client disconnects.
    let task = tokio::spawn(OPERATION_ID.scope(id.clone(), async move {
        let response = next.run(request).await;
        let code = response.status().as_u16();
        let bytes = to_bytes(response.into_body(), 1024 * 1024).await?;
        task_state
            .store
            .finish_outbox(&task_id, code, &String::from_utf8_lossy(&bytes))?;
        Ok::<(), anyhow::Error>(())
    }));
    // Album handlers have their own longer timeout. A lost HTTP response is recoverable by key.
    let _ = tokio::time::timeout(Duration::from_secs(16 * 60), task).await;
    match state.store.outbox_entry(&id) {
        Ok(Some(entry)) => render(&entry),
        _ => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

pub fn confirm_late(state: &AppState, id: &str) -> anyhow::Result<()> {
    let results = state.store.outbox_results(id)?;
    let Some(primary) = results.first().filter(|result| result["success"] == true) else {
        return Ok(());
    };
    let message_id = primary["message_id"].as_str().unwrap_or_default();
    let message = state.store.get_message_by_id(message_id)?;
    let original_expected = message
        .as_ref()
        .and_then(|message| message.content.as_ref())
        .and_then(|content| content.get("showTranslatedPrimary"))
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    let response = serde_json::json!({
        "success":true,"messageId":message_id,"timestamp":primary["timestamp"],
        "messageIds":primary["message_ids"],"timestamps":primary["timestamps"],
        "isTranslated":message.as_ref().is_some_and(|m|m.is_translated),
        "translatedText":message.as_ref().and_then(|m|m.translated_text.clone()),
        "sourceLanguage":message.as_ref().and_then(|m|m.source_language.clone()),
        "originalFollowUpSent":results.get(1).is_some_and(|r|r["success"]==true),
        "originalMessageId":results.get(1).and_then(|r|r["message_id"].as_str()),
        "originalTimestamp":results.get(1).map(|r|r["timestamp"].clone()),
        "originalFollowUpError":if original_expected && !results.get(1).is_some_and(|r|r["success"]==true) {Some("Delivery was confirmed after a delay. Check the conversation for any original follow-up.")} else {None},
        "warning":"Delivery was confirmed after a delay. Check the conversation before sending another recording."
    });
    state.store.confirm_late_outbox(id, &response.to_string())
}

pub fn recover_receipts(state: &AppState) -> anyhow::Result<()> {
    for (temporary, payload) in state.store.recoverable_send_receipts()? {
        let receipt: crate::web::BridgeSendResult = serde_json::from_str(&payload)?;
        if let (Some(temporary), Some(confirmed)) = (temporary, receipt.message_id.as_deref()) {
            state
                .store
                .replace_message_id(&temporary, confirmed, receipt.timestamp)?;
            state.store.set_delivery_status(confirmed, "sent")?;
        }
        crate::voice::recover_confirmation(state, receipt)?;
    }
    for id in state.store.unfinished_outbox_ids()? {
        confirm_late(state, &id)?;
    }
    Ok(())
}
