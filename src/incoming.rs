//! Two workers consume a persisted, bounded queue. The bridge loop never waits on AI.
use crate::web::{AppState, WebSocketEvent};
use std::{
    sync::{atomic::Ordering, Arc},
    time::Duration,
};

pub fn start(state: Arc<AppState>) -> anyhow::Result<()> {
    state.store.recover_translations()?;
    if state.translator.is_none() {
        return Ok(());
    }
    for _ in 0..2 {
        let state = state.clone();
        tokio::spawn(async move {
            loop {
                match state.store.claim_translation() {
                    Ok(Some(id)) => {
                        if let Err(error) = translate(&state, &id).await {
                            tracing::warn!("Background translation deferred: {error}");
                            let _ = state.store.retry_translation(&id);
                        }
                    }
                    Ok(None) => tokio::time::sleep(Duration::from_millis(500)).await,
                    Err(error) => {
                        tracing::error!("Translation queue failed: {error}");
                        tokio::time::sleep(Duration::from_secs(5)).await;
                    }
                }
            }
        });
    }
    Ok(())
}

async fn translate(state: &AppState, id: &str) -> anyhow::Result<()> {
    use anyhow::Context;
    let epoch = state.voice_epoch.load(Ordering::SeqCst);
    let Some(message) = state.store.get_message_by_id(id)? else {
        return state.store.finish_translation(id, None, "", false);
    };
    let text = message
        .original_text
        .as_deref()
        .context("No text to translate")?;
    let settings = state.store.get_conversation_settings(&message.contact_id)?;
    let translator = state
        .translator
        .as_ref()
        .context("Translation unavailable")?;
    let result = tokio::time::timeout(
        Duration::from_secs(90),
        translator.process_text(
            text,
            settings.language_override.as_deref(),
            settings.translation_style.as_deref(),
        ),
    )
    .await??;
    if epoch != state.voice_epoch.load(Ordering::SeqCst) {
        return Ok(());
    }
    state.store.finish_translation(
        id,
        result.translated_text.as_deref(),
        &result.source_language,
        result.needs_translation,
    )?;
    if result.usage.input_tokens > 0 {
        state.store.record_usage(
            Some(&message.contact_id),
            Some(id),
            &result.usage,
            if result.needs_translation {
                "translate_incoming"
            } else {
                "detect_language"
            },
        )?;
    }
    if let Some(message) = state.store.get_message_by_id(id)? {
        let _ = state
            .broadcast_tx
            .send(WebSocketEvent::MessageUpdated { message });
    }
    Ok(())
}
