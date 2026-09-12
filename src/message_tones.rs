//! Shared message sounds: persisted on the server and resolved before push delivery.
use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Deserializer, Serialize};
use std::sync::Arc;

use crate::web::AppState;

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum MessageTone {
    #[default]
    Default,
    Silent,
    Aurora,
    Bamboo,
    Bloom,
    Droplet,
    Glass,
    Orbit,
}

impl MessageTone {
    pub fn sound_name(self) -> Option<&'static str> {
        match self {
            Self::Default => Some("default"),
            Self::Silent => None,
            Self::Aurora => Some("bb-aurora.wav"),
            Self::Bamboo => Some("bb-bamboo.wav"),
            Self::Bloom => Some("bb-bloom.wav"),
            Self::Droplet => Some("bb-droplet.wav"),
            Self::Glass => Some("bb-glass.wav"),
            Self::Orbit => Some("bb-orbit.wav"),
        }
    }
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MessageToneSettings {
    /// None means this conversation follows the global setting.
    pub tone: Option<MessageTone>,
    pub global_tone: MessageTone,
    pub effective_tone: MessageTone,
}

#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MessageToneQuery {
    pub contact_id: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpdateMessageTone {
    // Require the field even though an explicit null is a valid inheritance choice.
    #[serde(deserialize_with = "required_tone")]
    pub tone: Option<MessageTone>,
}

fn required_tone<'de, D: Deserializer<'de>>(decoder: D) -> Result<Option<MessageTone>, D::Error> {
    Option::<MessageTone>::deserialize(decoder)
}

fn validate_contact(
    state: &AppState,
    contact_id: Option<&str>,
) -> Result<(), (StatusCode, &'static str)> {
    if let Some(id) = contact_id {
        if id.is_empty() || id.len() > 256 {
            return Err((StatusCode::BAD_REQUEST, "Invalid conversation."));
        }
        match state.store.get_contact(id) {
            Ok(Some(_)) => {}
            Ok(None) => return Err((StatusCode::NOT_FOUND, "Conversation not found.")),
            Err(error) => {
                tracing::warn!("Message ringtone conversation lookup failed: {error}");
                return Err((
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Couldn’t load the conversation. Please try again.",
                ));
            }
        }
    }
    Ok(())
}

fn failure(error: anyhow::Error) -> Response {
    tracing::warn!("Message ringtone settings failed: {error}");
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        "Couldn’t load or save the message ringtone. Please try again.",
    )
        .into_response()
}

pub async fn get_settings(
    State(state): State<Arc<AppState>>,
    Query(query): Query<MessageToneQuery>,
) -> Response {
    if let Err(response) = validate_contact(&state, query.contact_id.as_deref()) {
        return response.into_response();
    }
    match state
        .store
        .message_tone_settings(query.contact_id.as_deref())
    {
        Ok(settings) => Json(settings).into_response(),
        Err(error) => failure(error),
    }
}

pub async fn put_settings(
    State(state): State<Arc<AppState>>,
    Query(query): Query<MessageToneQuery>,
    Json(update): Json<UpdateMessageTone>,
) -> Response {
    if let Err(response) = validate_contact(&state, query.contact_id.as_deref()) {
        return response.into_response();
    }
    if query.contact_id.is_none() && update.tone.is_none() {
        return (StatusCode::BAD_REQUEST, "Choose a global ringtone.").into_response();
    }
    let result = state
        .store
        .set_message_tone(query.contact_id.as_deref(), update.tone)
        .and_then(|()| {
            state
                .store
                .message_tone_settings(query.contact_id.as_deref())
        });
    match result {
        Ok(settings) => Json(settings).into_response(),
        Err(error) => failure(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::MessageStore;

    #[test]
    fn message_tones_persist_and_resolve_exact_conversation_overrides() {
        let dir = std::env::temp_dir().join(format!("message-tones-{}", uuid::Uuid::new_v4()));
        let store = MessageStore::new(&dir).unwrap();
        assert_eq!(
            store.message_tone_settings(None).unwrap().effective_tone,
            MessageTone::Default
        );
        store
            .set_message_tone(None, Some(MessageTone::Aurora))
            .unwrap();
        store
            .set_message_tone(Some("family@g.us"), Some(MessageTone::Bamboo))
            .unwrap();
        store
            .set_message_tone(Some("quiet@g.us"), Some(MessageTone::Silent))
            .unwrap();
        drop(store);
        let store = MessageStore::new(&dir).unwrap();
        assert_eq!(
            store
                .message_tone_settings(Some("family@g.us"))
                .unwrap()
                .effective_tone,
            MessageTone::Bamboo
        );
        assert_eq!(
            store
                .message_tone_settings(Some("family+other@g.us"))
                .unwrap()
                .effective_tone,
            MessageTone::Aurora
        );
        store
            .set_message_tone(None, Some(MessageTone::Glass))
            .unwrap();
        assert_eq!(
            store
                .message_tone_settings(Some("quiet@g.us"))
                .unwrap()
                .effective_tone,
            MessageTone::Silent
        );
        store.set_message_tone(Some("family@g.us"), None).unwrap();
        let inherited = store.message_tone_settings(Some("family@g.us")).unwrap();
        assert_eq!(inherited.tone, None);
        assert_eq!(inherited.effective_tone, MessageTone::Glass);
        store
            .set_message_tone(None, Some(MessageTone::Silent))
            .unwrap();
        store
            .set_message_tone(Some("audible@g.us"), Some(MessageTone::Bloom))
            .unwrap();
        assert_eq!(
            store
                .message_tone_settings(Some("audible@g.us"))
                .unwrap()
                .effective_tone,
            MessageTone::Bloom
        );
        assert_eq!(
            store
                .message_tone_settings(Some("family@g.us"))
                .unwrap()
                .effective_tone,
            MessageTone::Silent
        );
        store.clear_all().unwrap();
        assert_eq!(
            store.message_tone_settings(None).unwrap().effective_tone,
            MessageTone::Default
        );
        drop(store);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn tone_catalog_matches_push_names_and_contains_short_pcm_audio() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("web/public/sounds");
        let catalog: Vec<serde_json::Value> =
            serde_json::from_slice(&std::fs::read(root.join("message-tones.json")).unwrap())
                .unwrap();
        assert_eq!(catalog.len(), 6);
        let mut seen = std::collections::HashSet::new();
        for entry in catalog {
            let tone: MessageTone = serde_json::from_value(entry["id"].clone()).unwrap();
            assert!(seen.insert(entry["id"].as_str().unwrap().to_string()));
            let filename = entry["filename"].as_str().unwrap();
            assert_eq!(tone.sound_name(), Some(filename));
            let wav = std::fs::read(root.join(filename)).unwrap();
            assert_eq!(&wav[..4], b"RIFF");
            assert_eq!(&wav[8..12], b"WAVE");
            assert_eq!(u16::from_le_bytes(wav[20..22].try_into().unwrap()), 1); // linear PCM
            assert_eq!(u16::from_le_bytes(wav[22..24].try_into().unwrap()), 1); // mono
            assert_eq!(u16::from_le_bytes(wav[34..36].try_into().unwrap()), 16);
            let byte_rate = u32::from_le_bytes(wav[28..32].try_into().unwrap());
            let data_len = u32::from_le_bytes(wav[40..44].try_into().unwrap());
            let seconds = data_len as f64 / byte_rate as f64;
            assert!((0.2..3.0).contains(&seconds));
            let samples: Vec<i16> = wav[44..]
                .chunks_exact(2)
                .map(|pair| i16::from_le_bytes([pair[0], pair[1]]))
                .collect();
            assert!(samples.iter().any(|sample| sample.unsigned_abs() > 1000));
            assert!(samples.iter().all(|sample| sample.unsigned_abs() < 20000));
            assert_eq!(*samples.first().unwrap(), 0);
            assert!(samples.last().unwrap().unsigned_abs() < 10);
        }
    }
}
