//! Voice-note translation. All network writes require a reviewed, durable preparation.
use crate::{bridge::BridgeCommand, storage::StoredMessage, web::AppState};
use anyhow::{bail, Context, Result};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    path::{Path as FsPath, PathBuf},
    sync::Arc,
    time::Duration,
};
use tokio::process::Command;

const MAX_BYTES: usize = 16 * 1024 * 1024;
const MAX_SECONDS: usize = 180;
const RATE: usize = 16000;

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VoiceSettings {
    pub voice: String,
}
fn valid_voice(value: &str) -> bool {
    matches!(value, "auto" | "masculine" | "feminine" | "neutral")
}
fn failure(error: anyhow::Error) -> Response {
    tracing::warn!("Voice note operation failed: {error}");
    (
        StatusCode::BAD_REQUEST,
        Json(json!({"error": error.to_string()})),
    )
        .into_response()
}
fn result_response(result: Result<Value>) -> Response {
    match result {
        Ok(value) => Json(value).into_response(),
        Err(error) => failure(error),
    }
}
pub async fn get_settings(
    State(state): State<Arc<AppState>>,
    Path(scope): Path<String>,
) -> Response {
    result_response(
        state
            .store
            .voice_setting(&scope)
            .map(|voice| json!({"voice": voice})),
    )
}
pub async fn put_settings(
    State(state): State<Arc<AppState>>,
    Path(scope): Path<String>,
    Json(settings): Json<VoiceSettings>,
) -> Response {
    if !valid_voice(&settings.voice) {
        return failure(anyhow::anyhow!(
            "Choose automatic, masculine, feminine or neutral."
        ));
    }
    result_response(
        state
            .store
            .set_voice_setting(&scope, &settings.voice)
            .map(|_| json!({"voice": settings.voice})),
    )
}

/// A short built-in voice sample; this never contacts WhatsApp.
pub async fn sample(
    State(state): State<Arc<AppState>>,
    Path(preference): Path<String>,
) -> Response {
    result_response(async {
        if !valid_voice(&preference) { bail!("Unknown voice preference."); }
        let key = format!("sample-v1-{preference}");
        if let Some((payload,_,_,_)) = state.store.voice_note(&key)? { return Ok(serde_json::from_str(&payload)?); }
        let _guard = state.voice_lock.try_lock().map_err(|_| anyhow::anyhow!("Another recording is processing. Try the preview again shortly."))?;
        let translator = state.translator.as_ref().context("Voice translation is not configured on this server.")?;
        let epoch = state.voice_epoch.load(std::sync::atomic::Ordering::SeqCst);
        let voice = select_voice(&preference, &[]);
        let response = reqwest::Client::builder().timeout(Duration::from_secs(45)).build()?
            .post(translator.audio_api_url("speech")).bearer_auth(translator.get_api_key())
            .json(&json!({"model":"gpt-4o-mini-tts","voice":voice,"input":"This is the voice I will use for translated messages.","response_format":"mp3"})).send().await?;
        let bytes = checked_bytes(response, 1024*1024).await?;
        let value = json!({"audioData":B64.encode(bytes),"mimeType":"audio/mpeg"});
        if epoch != state.voice_epoch.load(std::sync::atomic::Ordering::SeqCst) { bail!("Session changed. Please reconnect."); }
        state.store.save_voice_note(&key, &value.to_string())?;
        Ok(value)
    }.await)
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct VoiceNote {
    id: String,
    contact_id: String,
    transcript: String,
    translation: String,
    target_language: String,
    source_language: String,
    voice: String,
    audio_data: String,
    original_data: String,
    duration_seconds: u32,
    original_duration_seconds: u32,
    send_audio: String,
    send_original: String,
    original_follow_up: bool,
    reply_to: Option<String>,
    reply_to_sender: Option<String>,
    reply_to_text: Option<String>,
}
impl VoiceNote {
    fn public(&self) -> Value {
        json!({"id": self.id, "contactId": self.contact_id, "transcript": self.transcript,
        "translation": self.translation, "targetLanguage": self.target_language, "voice": self.voice,
        "audioData": self.audio_data, "originalData": self.original_data, "mimeType": "audio/mpeg",
        "durationSeconds": self.duration_seconds, "originalFollowUp": self.original_follow_up})
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrepareRequest {
    contact_id: String,
    media_data: String,
    reply_to: Option<String>,
    reply_to_sender: Option<String>,
    reply_to_text: Option<String>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SendRequest {
    preparation_id: String,
}

// No filenames or network URLs supplied by a client are passed to ffmpeg.
struct Scratch(PathBuf);
impl Scratch {
    fn new(root: &FsPath) -> Result<Self> {
        let path = root.join(format!("voice-tmp-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&path)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700))?;
        }
        Ok(Self(path))
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
async fn convert(dir: &FsPath, input: &str, output: &str, options: &[&str]) -> Result<Vec<u8>> {
    let mut command = Command::new("ffmpeg");
    command
        .kill_on_drop(true)
        .args([
            "-v",
            "error",
            "-nostdin",
            "-y",
            "-protocol_whitelist",
            "file,pipe",
            "-i",
        ])
        .arg(dir.join(input))
        .args([
            "-vn",
            "-map_metadata",
            "-1",
            "-threads",
            "1",
            "-t",
            "241",
            "-fs",
            "16000000",
        ])
        .args(options)
        .arg(dir.join(output));
    let output_result = tokio::time::timeout(Duration::from_secs(45), command.output())
        .await
        .context("Audio conversion timed out")??;
    if !output_result.status.success() {
        bail!("Could not read this recording. Please record again.");
    }
    let bytes = tokio::fs::read(dir.join(output)).await?;
    if bytes.is_empty() || bytes.len() >= 16_000_000 {
        bail!("Recording is empty or too large.");
    }
    Ok(bytes)
}
fn decode_audio(encoded: &str) -> Result<Vec<u8>> {
    if encoded.len() > MAX_BYTES.div_ceil(3) * 4 {
        bail!("Voice notes must be smaller than 16 MB.");
    }
    let bytes = B64.decode(encoded).context("Invalid audio encoding")?;
    if bytes.len() < 32 || bytes.len() > MAX_BYTES {
        bail!("Recording is empty or too large.");
    }
    Ok(bytes)
}

/// Acoustic pitch matching only: this does not identify a speaker's gender.
fn select_voice(preference: &str, pcm: &[u8]) -> &'static str {
    match preference {
        "masculine" => return "onyx",
        "feminine" => return "shimmer",
        "neutral" => return "alloy",
        _ => {}
    }
    let samples: Vec<f64> = pcm
        .chunks_exact(2)
        .map(|x| i16::from_le_bytes([x[0], x[1]]) as f64 / 32768.0)
        .collect();
    let mut pitches = Vec::new();
    for frame in samples.chunks(640).step_by(5).take(200) {
        if frame.len() < 640 {
            continue;
        }
        let mean = frame.iter().sum::<f64>() / frame.len() as f64;
        let centered: Vec<_> = frame.iter().map(|s| s - mean).collect();
        let energy: f64 = centered.iter().map(|s| s * s).sum();
        if energy / (frame.len() as f64) < 0.0001 {
            continue;
        }
        let mut best = (0.0, 0);
        for lag in 40..=228 {
            // 70–400 Hz
            let a = &centered[..centered.len() - lag];
            let b = &centered[lag..];
            let dot: f64 = a.iter().zip(b).map(|(x, y)| x * y).sum();
            let norm = (a.iter().map(|x| x * x).sum::<f64>()
                * b.iter().map(|x| x * x).sum::<f64>())
            .sqrt();
            let correlation = dot / norm.max(1e-12);
            if correlation > best.0 {
                best = (correlation, lag);
            }
            // First strong peak avoids selecting a lower octave of the same pitch.
            if best.0 > 0.9 && correlation < best.0 - 0.02 {
                break;
            }
        }
        if best.0 > 0.75 && best.1 > 0 {
            pitches.push(RATE as f64 / best.1 as f64);
        }
    }
    if pitches.len() < 4 {
        return "alloy";
    }
    pitches.sort_by(f64::total_cmp);
    let median = pitches[pitches.len() / 2];
    if median < 165.0 {
        "onyx"
    } else if median > 195.0 {
        "shimmer"
    } else {
        "alloy"
    }
}

async fn checked_bytes(response: reqwest::Response, limit: usize) -> Result<Vec<u8>> {
    use futures::StreamExt;
    if !response.status().is_success() {
        bail!(
            "Voice service returned {}. Please try again later.",
            response.status()
        );
    }
    let mut bytes = Vec::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        if bytes.len() + chunk.len() > limit {
            bail!("Voice service response is too large.");
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}
async fn build_note(
    state: &AppState,
    req: PrepareRequest,
    incoming: bool,
    id: String,
) -> Result<VoiceNote> {
    let epoch = state.voice_epoch.load(std::sync::atomic::Ordering::SeqCst);
    let translator = state
        .translator
        .as_ref()
        .context("Translation is unavailable. Configure the OpenAI API key on the server.")?;
    state
        .store
        .get_contact(&req.contact_id)?
        .context("Conversation not found")?;
    if let Some(reply_id) = &req.reply_to {
        let reply = state
            .store
            .get_message_by_id(reply_id)?
            .context("Reply message not found")?;
        if reply.contact_id != req.contact_id {
            bail!("Reply belongs to another conversation.");
        }
    }
    let settings = state.store.get_conversation_settings(&req.contact_id)?;
    let target = if incoming {
        translator.default_language().to_string()
    } else {
        settings
            .language_override
            .clone()
            .or(state.store.get_conversation_language(&req.contact_id, 10)?)
            .context("Set this conversation’s language before sending a translated voice note.")?
    };
    let preference = state.store.voice_setting(if incoming {
        &req.contact_id
    } else {
        "outgoing"
    })?;
    let scratch = Scratch::new(&state.data_dir)?;
    tokio::fs::write(scratch.0.join("input"), decode_audio(&req.media_data)?).await?;
    let pcm = convert(
        &scratch.0,
        "input",
        "source.pcm",
        &["-ac", "1", "-ar", "16000", "-f", "s16le"],
    )
    .await?;
    if pcm.len() > MAX_SECONDS * RATE * 2 {
        bail!("Voice notes can be up to three minutes long.");
    }
    if pcm.len() < RATE / 2 {
        bail!("Record at least a quarter second of audio.");
    }
    if pcm
        .chunks_exact(2)
        .all(|s| i16::from_le_bytes([s[0], s[1]]).unsigned_abs() < 128)
    {
        bail!("No audible speech was detected. Please record again.");
    }
    let original_seconds = (pcm.len() as f64 / 2.0 / RATE as f64).ceil() as u32;
    let voice = select_voice(&preference, &pcm).to_string();
    let wav = convert(
        &scratch.0,
        "input",
        "source.wav",
        &["-ac", "1", "-ar", "16000", "-c:a", "pcm_s16le"],
    )
    .await?;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(90))
        .build()?;
    let part = reqwest::multipart::Part::bytes(wav)
        .file_name("voice.wav")
        .mime_str("audio/wav")?;
    let response = client
        .post(translator.audio_api_url("transcriptions"))
        .bearer_auth(translator.get_api_key())
        .multipart(
            reqwest::multipart::Form::new()
                .text("model", "gpt-4o-mini-transcribe")
                .part("file", part),
        )
        .send()
        .await?;
    let transcription: Value = serde_json::from_slice(&checked_bytes(response, 256 * 1024).await?)?;
    let transcript = transcription["text"]
        .as_str()
        .unwrap_or("")
        .trim()
        .to_string();
    if transcript.is_empty() {
        bail!("No speech was detected. Please record again.");
    }
    let translated = translator
        .translate_voice(&transcript, &target, settings.translation_style.as_deref())
        .await?;
    let translation = translated
        .translated_text
        .context("Voice translation returned no text")?;
    if translation.trim().is_empty() || translation.chars().count() > 4096 {
        bail!("The translated recording is empty or too long. Please use a shorter note.");
    }
    state.store.record_usage(
        Some(&req.contact_id),
        None,
        &translated.usage,
        "voice_translation",
    )?;
    let response = client.post(translator.audio_api_url("speech")).bearer_auth(translator.get_api_key())
        .json(&json!({"model":"gpt-4o-mini-tts", "voice":voice, "input":translation, "response_format":"mp3", "instructions":format!("Speak naturally in {target}. Read the provided text faithfully without additions.")})).send().await?;
    let audio = checked_bytes(response, MAX_BYTES).await?;
    tokio::fs::write(scratch.0.join("translated.mp3"), &audio).await?;
    let translated_pcm = convert(
        &scratch.0,
        "translated.mp3",
        "translated.pcm",
        &["-ac", "1", "-ar", "16000", "-f", "s16le"],
    )
    .await?;
    let seconds = (translated_pcm.len() as f64 / 2.0 / RATE as f64).ceil() as u32;
    if seconds > 240 {
        bail!("The translated recording is too long. Please use a shorter note.");
    }
    let original = convert(
        &scratch.0,
        "input",
        "original.mp3",
        &["-ac", "1", "-c:a", "libmp3lame", "-b:a", "64k"],
    )
    .await?;
    let send_audio = if incoming {
        Vec::new()
    } else {
        convert(
            &scratch.0,
            "translated.mp3",
            "translated.ogg",
            &["-ac", "1", "-c:a", "libopus", "-b:a", "32k"],
        )
        .await?
    };
    let follow_up = !incoming && settings.send_original_follow_up && transcript != translation;
    let send_original = if follow_up {
        convert(
            &scratch.0,
            "input",
            "original.ogg",
            &["-ac", "1", "-c:a", "libopus", "-b:a", "32k"],
        )
        .await?
    } else {
        Vec::new()
    };
    if epoch != state.voice_epoch.load(std::sync::atomic::Ordering::SeqCst) {
        bail!("Session changed while translating. Please reconnect.");
    }
    Ok(VoiceNote {
        id,
        contact_id: req.contact_id,
        source_language: translated.source_language,
        transcript,
        translation,
        target_language: target,
        voice,
        audio_data: B64.encode(audio),
        original_data: B64.encode(original),
        duration_seconds: seconds,
        original_duration_seconds: original_seconds,
        send_audio: B64.encode(send_audio),
        send_original: B64.encode(send_original),
        original_follow_up: follow_up,
        reply_to: req.reply_to,
        reply_to_sender: req.reply_to_sender,
        reply_to_text: req.reply_to_text,
    })
}

pub async fn prepare(
    State(state): State<Arc<AppState>>,
    Json(req): Json<PrepareRequest>,
) -> Response {
    result_response(
        async {
            let _guard = state.voice_lock.try_lock().map_err(|_| {
                anyhow::anyhow!("Another voice note is processing. Please try again shortly.")
            })?;
            let note = build_note(&state, req, false, uuid::Uuid::new_v4().to_string()).await?;
            state
                .store
                .save_voice_note(&note.id, &serde_json::to_string(&note)?)?;
            Ok(note.public())
        }
        .await,
    )
}
pub async fn translate_received(
    State(state): State<Arc<AppState>>,
    Path(message_id): Path<String>,
) -> Response {
    result_response(
        async {
            if let Some(note) = cached_received(&state, &message_id)? {
                return Ok(note.public());
            }
            let _guard = state.voice_lock.try_lock().map_err(|_| {
                anyhow::anyhow!("Another voice note is processing. Please try again shortly.")
            })?;
            Ok(translate_received_inner(&state, &message_id)
                .await?
                .public())
        }
        .await,
    )
}
fn received_key(state: &AppState, message_id: &str) -> Result<String> {
    let message = state
        .store
        .get_message_by_id(message_id)?
        .context("Message not found")?;
    if message.content_type != "audio" {
        bail!("This message is not audio.");
    }
    if message.is_from_me
        && state
            .store
            .voice_note(&format!("sent-{message_id}"))?
            .is_some()
    {
        return Ok(format!("sent-{message_id}"));
    }
    let translator = state
        .translator
        .as_ref()
        .context("Voice translation is not configured on this server.")?;
    let preference = state.store.voice_setting(&message.contact_id)?;
    let settings = state.store.get_conversation_settings(&message.contact_id)?;
    Ok(format!(
        "received-{:x}",
        Sha256::digest(format!(
            "v1:{message_id}:{}:{preference}:{:?}",
            translator.default_language(),
            settings.translation_style
        ))
    ))
}
fn cached_received(state: &AppState, message_id: &str) -> Result<Option<VoiceNote>> {
    let id = received_key(state, message_id)?;
    state
        .store
        .voice_note(&id)?
        .map(|(payload, _, _, _)| serde_json::from_str(&payload).map_err(Into::into))
        .transpose()
}
async fn translate_received_inner(state: &AppState, message_id: &str) -> Result<VoiceNote> {
    if let Some(note) = cached_received(state, message_id)? {
        return Ok(note);
    }
    let id = received_key(state, message_id)?;
    let message = state
        .store
        .get_message_by_id(message_id)?
        .context("Message not found")?;
    let (data, _) = state
        .store
        .get_message_media(message_id)?
        .context("Audio has not downloaded yet. Please try again.")?;
    let note = build_note(
        state,
        PrepareRequest {
            contact_id: message.contact_id,
            media_data: data,
            reply_to: None,
            reply_to_sender: None,
            reply_to_text: None,
        },
        true,
        id,
    )
    .await?;
    state
        .store
        .save_voice_note(&note.id, &serde_json::to_string(&note)?)?;
    if !message.is_from_me {
        state.store.update_voice_transcript(
            message_id,
            &note.transcript,
            &note.translation,
            &note.source_language,
        )?;
        // Signal cache readiness without rebroadcasting a message (which would increment unread counts).
        let _ = state
            .broadcast_tx
            .send(crate::web::WebSocketEvent::VoiceReady {
                message_id: message_id.to_string(),
            });
    }
    Ok(note)
}

/// Bound automatic work independently of the bridge event loop. Older notes remain available on demand.
pub fn queue_incoming(state: Arc<AppState>, message_id: String) {
    if state.translator.is_none() {
        return;
    }
    let Ok(permit) = state.voice_queue.clone().try_acquire_owned() else {
        return;
    };
    let epoch = state.voice_epoch.load(std::sync::atomic::Ordering::SeqCst);
    tokio::spawn(async move {
        let _permit = permit;
        let _guard = state.voice_lock.lock().await;
        if epoch != state.voice_epoch.load(std::sync::atomic::Ordering::SeqCst) {
            return;
        }
        if let Err(error) = translate_received_inner(&state, &message_id).await {
            tracing::warn!("Incoming voice translation deferred: {error}");
        }
    });
}

pub async fn send(State(state): State<Arc<AppState>>, Json(req): Json<SendRequest>) -> Response {
    result_response(async {
        if uuid::Uuid::parse_str(&req.preparation_id).is_err() { bail!("Incoming translations cannot be sent as prepared recordings."); }
        let (payload, status, result, created) = state.store.voice_note(&req.preparation_id)?.context("Recording preview expired. Please prepare it again.")?;
        if let Some(result) = result { return Ok(serde_json::from_str(&result)?); }
        if status != "prepared" { bail!("Delivery is awaiting confirmation. Check the conversation before recording again; this note will not be sent twice."); }
        if chrono::Utc::now().timestamp() - created > 15 * 60 { bail!("Recording preview expired. Please prepare it again."); }
        if !*state.connected.read().await { bail!("WhatsApp is disconnected. Reconnect before sending."); }
        let note: VoiceNote = serde_json::from_str(&payload)?;
        if !state.store.claim_voice_send(&note.id)? { bail!("This recording is already sending."); }
        // Sending belongs to the server, not the HTTP connection: losing a client must not cancel delivery.
        let send_state = state.clone();
        let operation = crate::outbox::OPERATION_ID.try_with(Clone::clone).unwrap_or_default();
        let outcome = tokio::spawn(crate::outbox::OPERATION_ID.scope(operation,async move {
            let primary = send_one(&send_state, &note, false).await?;
            let mut result = json!({"success":true,"messageId":primary.0,"timestamp":primary.1,"originalFollowUpSent":false});
            if note.original_follow_up {
                match send_one(&send_state, &note, true).await {
                    Ok(_) => result["originalFollowUpSent"] = json!(true),
                    Err(_) => result["warning"] = json!("Translated voice note sent, but the original follow-up could not be confirmed. Check the conversation before sending it again."),
                }
            }
            send_state.store.finish_voice_send(&note.id, &result.to_string())?;
            Ok::<_, anyhow::Error>(result)
        })).await??;
        Ok(outcome)
    }.await)
}
async fn send_one(state: &AppState, note: &VoiceNote, original: bool) -> Result<(String, i64)> {
    let request_id = state.next_request_id();
    let pending_id = format!("pending_voice_{request_id}");
    store_audio_message(
        state,
        note,
        original,
        crate::web::BridgeSendResult {
            request_id,
            success: false,
            message_id: Some(pending_id.clone()),
            timestamp: Some(chrono::Utc::now().timestamp_millis()),
            message_ids: vec![],
            timestamps: vec![],
            error: None,
        },
        "sending",
    )?;
    let rx = state
        .register_pending_send(request_id, Some(pending_id))
        .await;
    state
        .store
        .attach_voice_attempt(request_id, &note.id, original)?;
    let duration = if original {
        note.original_duration_seconds
    } else {
        note.duration_seconds
    };
    let handed_off = state
        .send_bridge_command(BridgeCommand::SendAudio {
            request_id: Some(request_id),
            to: note.contact_id.clone(),
            media_data: if original {
                note.send_original.clone()
            } else {
                note.send_audio.clone()
            },
            duration_seconds: duration,
            reply_to: note.reply_to.clone(),
            reply_to_sender: note.reply_to_sender.clone(),
            reply_to_text: note.reply_to_text.clone(),
        })
        .await;
    if let Err(error) = handed_off {
        state.cancel_pending_send(request_id).await;
        if !original {
            state.store.release_voice_send(&note.id)?;
        }
        bail!(error);
    }
    let result = state.wait_for_send_result(request_id, rx).await.map_err(|_| anyhow::anyhow!("Voice-note delivery could not be confirmed. Check the conversation; this recording is locked against duplicate sending."))?;
    if !result.success {
        bail!("WhatsApp could not confirm the voice note. Check the conversation before trying a new recording.");
    }
    store_confirmed(state, note, original, result)
}

pub fn recover_confirmation(state: &AppState, result: crate::web::BridgeSendResult) -> Result<()> {
    let Some((id, original)) = state.store.voice_attempt(result.request_id)? else {
        return Ok(());
    };
    let Some((payload, _, _, _)) = state.store.voice_note(&id)? else {
        return Ok(());
    };
    if payload == "{}" {
        return Ok(());
    } // Its durable pending message has already been reconciled.
    let note: VoiceNote = serde_json::from_str(&payload)?;
    let (message_id, timestamp) = store_confirmed(state, &note, original, result)?;
    // Preserve the uncertain preparation against resending, but make its late confirmation reviewable.
    if !original {
        state.store.finish_voice_send(&id,&json!({"success":true,"messageId":message_id,"timestamp":timestamp,"originalFollowUpSent":false,"warning":"Delivery confirmed after a delay. The original follow-up was not sent automatically."}).to_string())?;
    }
    Ok(())
}

fn store_confirmed(
    state: &AppState,
    note: &VoiceNote,
    original: bool,
    result: crate::web::BridgeSendResult,
) -> Result<(String, i64)> {
    store_audio_message(state, note, original, result, "sent")
}

fn store_audio_message(
    state: &AppState,
    note: &VoiceNote,
    original: bool,
    result: crate::web::BridgeSendResult,
    status: &str,
) -> Result<(String, i64)> {
    let duration = if original {
        note.original_duration_seconds
    } else {
        note.duration_seconds
    };
    let message_id = result.message_id.context("Missing WhatsApp confirmation")?;
    let timestamp = result
        .timestamp
        .unwrap_or_else(|| chrono::Utc::now().timestamp_millis());
    let contact = state
        .store
        .get_contact(&note.contact_id)?
        .context("Conversation not found")?;
    let content = json!({"type":"audio","isVoiceNote":true,"durationSeconds":duration,"mime_type":"audio/mpeg", "media_data":if original {&note.original_data} else {&note.audio_data},"aiGenerated":!original});
    let stored = StoredMessage {
        id: message_id.clone(),
        contact_id: note.contact_id.clone(),
        contact_name: contact.name.clone(),
        contact_phone: contact.phone.clone(),
        sender_name: None,
        sender_phone: None,
        timestamp,
        is_from_me: true,
        is_forwarded: false,
        chat_type: contact
            .contact_type
            .clone()
            .unwrap_or_else(|| "private".into()),
        content_type: "audio".into(),
        content_json: content.to_string(),
        content: Some(content),
        original_text: Some(note.transcript.clone()),
        translated_text: if original {
            None
        } else {
            Some(note.translation.clone())
        },
        source_language: Some(if original {
            note.source_language.clone()
        } else {
            note.target_language.clone()
        }),
        is_translated: !original,
        delivery_status: Some(status.into()),
    };
    state.store.add_message(&stored)?;
    if !original {
        let mut cached = note.clone();
        cached.id = format!("sent-{message_id}");
        cached.send_audio.clear();
        cached.send_original.clear();
        state
            .store
            .save_voice_note(&cached.id, &serde_json::to_string(&cached)?)?;
    }
    state.store.upsert_contact(
        &note.contact_id,
        contact.name.as_deref(),
        contact.phone.as_deref(),
        contact.contact_type.as_deref(),
        timestamp,
    )?;
    state.broadcast_message(stored);
    Ok((message_id, timestamp))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn pcm_tone(hz: f64) -> Vec<u8> {
        (0..RATE)
            .flat_map(|i| {
                (((2.0 * std::f64::consts::PI * hz * i as f64 / RATE as f64).sin() * 15000.0)
                    as i16)
                    .to_le_bytes()
            })
            .collect()
    }
    #[test]
    fn acoustic_matching_and_overrides() {
        assert_eq!(select_voice("auto", &pcm_tone(120.0)), "onyx");
        assert_eq!(select_voice("auto", &pcm_tone(240.0)), "shimmer");
        assert_eq!(select_voice("auto", &vec![0; RATE * 2]), "alloy");
        assert_eq!(select_voice("feminine", &pcm_tone(120.0)), "shimmer");
        assert_eq!(select_voice("neutral", &pcm_tone(240.0)), "alloy");
    }
    #[test]
    fn invalid_audio_is_rejected() {
        assert!(decode_audio("bad").is_err());
        assert!(decode_audio("").is_err());
        assert!(!valid_voice("arbitrary-model-voice"));
    }
}

#[cfg(test)]
mod integration_tests {
    use super::*;
    use crate::{
        storage::{ConversationSettings, MessageStore},
        translation::TranslationService,
        web::{create_router, BridgeSendResult},
    };
    use axum::{
        body::{to_bytes, Body},
        http::Request,
        routing::post,
        Router,
    };
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tower::ServiceExt;

    struct TestDirectory(PathBuf);
    impl TestDirectory {
        fn new() -> Self {
            let p = std::env::temp_dir().join(format!("voice-test-{}", uuid::Uuid::new_v4()));
            std::fs::create_dir(&p).unwrap();
            Self(p)
        }
    }
    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    fn state(root: &FsPath, translator: Option<Arc<TranslationService>>) -> Arc<AppState> {
        let store = MessageStore::new(root).unwrap();
        store
            .upsert_contact(
                "test@s.whatsapp.net",
                Some("Synthetic test"),
                None,
                Some("private"),
                1,
            )
            .unwrap();
        store
            .update_conversation_settings(
                "test@s.whatsapp.net",
                &ConversationSettings {
                    language_override: Some("Hungarian".into()),
                    translation_style: Some("friendly".into()),
                    send_original_follow_up: true,
                },
            )
            .unwrap();
        AppState::new(
            store,
            PathBuf::from("web/public"),
            root.to_path_buf(),
            translator,
            Some("test-password".into()),
            None,
        )
    }
    fn note(id: String) -> VoiceNote {
        VoiceNote {
            id,
            contact_id: "test@s.whatsapp.net".into(),
            transcript: "Good morning".into(),
            translation: "Jó reggelt".into(),
            target_language: "Hungarian".into(),
            source_language: "English".into(),
            voice: "onyx".into(),
            audio_data: B64.encode("mp3 test"),
            original_data: B64.encode("original"),
            duration_seconds: 2,
            original_duration_seconds: 2,
            send_audio: B64.encode("OggS translated fixture"),
            send_original: B64.encode("OggS original fixture"),
            original_follow_up: true,
            reply_to: None,
            reply_to_sender: None,
            reply_to_text: None,
        }
    }
    async fn json_response(response: Response) -> Value {
        serde_json::from_slice(
            &to_bytes(response.into_body(), 24 * 1024 * 1024)
                .await
                .unwrap(),
        )
        .unwrap()
    }

    #[tokio::test]
    async fn all_voice_routes_require_web_authentication() {
        let dir = TestDirectory::new();
        let state = state(&dir.0, None);
        for (method, url) in [
            ("POST", "/api/voice/prepare"),
            ("POST", "/api/voice/send"),
            ("POST", "/api/voice/translate/example"),
            ("GET", "/api/voice/settings/outgoing"),
            ("PUT", "/api/voice/settings/outgoing"),
        ] {
            let response = create_router(state.clone())
                .oneshot(
                    Request::builder()
                        .method(method)
                        .uri(url)
                        .header("Content-Type", "application/json")
                        .body(Body::from("{}"))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::UNAUTHORIZED, "{url}");
        }
    }
    #[tokio::test]
    async fn voice_preferences_and_duplicate_protection_survive_restart() {
        let dir = TestDirectory::new();
        let state = state(&dir.0, None);
        state
            .store
            .set_voice_setting("outgoing", "masculine")
            .unwrap();
        let note = note(uuid::Uuid::new_v4().to_string());
        state
            .store
            .save_voice_note(&note.id, &serde_json::to_string(&note).unwrap())
            .unwrap();
        assert!(state.store.claim_voice_send(&note.id).unwrap());
        let reopened = MessageStore::new(&dir.0).unwrap();
        assert_eq!(reopened.voice_setting("outgoing").unwrap(), "masculine");
        assert!(!reopened.claim_voice_send(&note.id).unwrap());
        assert_eq!(
            json_response(
                send(
                    State(state),
                    Json(SendRequest {
                        preparation_id: note.id
                    })
                )
                .await
            )
            .await["error"]
                .as_str()
                .unwrap()
                .contains("awaiting confirmation"),
            true
        );
    }
    #[tokio::test]
    async fn reviewed_voice_sends_translation_then_original_and_retries_do_not_resend() {
        let dir = TestDirectory::new();
        let state = state(&dir.0, None);
        *state.connected.write().await = true;
        let (tx, mut rx) = tokio::sync::mpsc::channel(4);
        state.set_command_tx(tx).await;
        let note = note(uuid::Uuid::new_v4().to_string());
        state
            .store
            .save_voice_note(&note.id, &serde_json::to_string(&note).unwrap())
            .unwrap();
        let bridge_state = state.clone();
        let bridge = tokio::spawn(async move {
            for (index, expected) in ["OggS translated fixture", "OggS original fixture"]
                .iter()
                .enumerate()
            {
                let command = tokio::time::timeout(Duration::from_secs(3), rx.recv())
                    .await
                    .unwrap()
                    .unwrap();
                if let BridgeCommand::SendAudio {
                    request_id: Some(request_id),
                    media_data,
                    to,
                    ..
                } = command
                {
                    assert_eq!(to, "test@s.whatsapp.net");
                    assert_eq!(B64.decode(media_data).unwrap(), expected.as_bytes());
                    bridge_state
                        .handle_send_result(BridgeSendResult {
                            request_id,
                            success: true,
                            message_id: Some(format!("confirmed-{index}")),
                            timestamp: Some(1700000000 + index as i64),
                            message_ids: vec![],
                            timestamps: vec![],
                            error: None,
                        })
                        .await;
                } else {
                    panic!("Expected an audio command");
                }
            }
            rx
        });
        let first = json_response(
            send(
                State(state.clone()),
                Json(SendRequest {
                    preparation_id: note.id.clone(),
                }),
            )
            .await,
        )
        .await;
        assert_eq!(first["success"], true);
        assert_eq!(first["originalFollowUpSent"], true);
        let mut rx = bridge.await.unwrap();
        let again = json_response(
            send(
                State(state.clone()),
                Json(SendRequest {
                    preparation_id: note.id.clone(),
                }),
            )
            .await,
        )
        .await;
        assert_eq!(first, again);
        assert!(rx.try_recv().is_err());
        assert_eq!(state.store.get_messages(&note.contact_id).unwrap().len(), 2);
        assert!(state
            .store
            .voice_note("sent-confirmed-0")
            .unwrap()
            .is_some());
    }
    #[tokio::test]
    async fn incoming_assets_cannot_be_used_as_send_preparations() {
        let dir = TestDirectory::new();
        let state = state(&dir.0, None);
        let result = json_response(
            send(
                State(state),
                Json(SendRequest {
                    preparation_id: "sent-known-message".into(),
                }),
            )
            .await,
        )
        .await;
        assert!(result["error"].as_str().unwrap().contains("cannot be sent"));
    }
    #[tokio::test]
    async fn voice_pipeline_converts_audio_and_translates_into_default_language() {
        let dir = TestDirectory::new();
        // Exercise the same codec conversion used in production; no microphone or external service.
        let tone = Command::new("ffmpeg")
            .args([
                "-v",
                "error",
                "-f",
                "lavfi",
                "-i",
                "sine=frequency=240:duration=1",
                "-f",
                "mp3",
                "pipe:1",
            ])
            .output()
            .await
            .expect("ffmpeg is required for voice tests");
        assert!(tone.status.success());
        let audio = tone.stdout;
        let calls = Arc::new(AtomicUsize::new(0));
        let speech_calls = calls.clone();
        let mode = Arc::new(AtomicUsize::new(0));
        let transcription_mode = mode.clone();
        let translation_mode = mode.clone();
        let speech_mode = mode.clone();
        let output_audio = audio.clone();
        let mock = Router::new()
            .route("/v1/audio/transcriptions", post(move || {let mode = transcription_mode.clone(); async move {Json(json!({"text":if mode.load(Ordering::SeqCst) == 0 {"Jó reggelt"} else {"Good morning"}}))}}))
            .route("/v1/responses", post(move |Json(body): Json<Value>| {let mode = translation_mode.clone(); async move {
                let incoming = mode.load(Ordering::SeqCst) == 0;
                let text = if body["model"] == "test-detect" {
                    if incoming {r#"{"language":"Hungarian","isTargetLanguage":false}"#} else {r#"{"language":"English","isTargetLanguage":false}"#}
                } else if incoming {"Good morning"} else {"Jó reggelt"};
                Json(json!({"output":[{"type":"message","content":[{"type":"output_text","text":text}]}],"usage":{"input_tokens":10,"output_tokens":3}}))
            }}))
            .route("/v1/audio/speech", post(move |Json(body): Json<Value>| {let audio = output_audio.clone(); let calls = speech_calls.clone(); let mode = speech_mode.clone(); async move {
                if mode.load(Ordering::SeqCst) == 2 { return StatusCode::SERVICE_UNAVAILABLE.into_response(); }
                assert_eq!(body["voice"], if mode.load(Ordering::SeqCst) == 0 {"shimmer"} else {"onyx"});
                assert_eq!(body["input"], if mode.load(Ordering::SeqCst) == 0 {"Good morning"} else {"Jó reggelt"});
                calls.fetch_add(1, Ordering::SeqCst);
                ([("content-type","audio/mpeg")],audio).into_response()
            }}));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move { axum::serve(listener, mock).await.unwrap() });
        let state = state(
            &dir.0,
            Some(Arc::new(TranslationService::new_with_api_url(format!(
                "http://{address}/v1/responses"
            )))),
        );
        state
            .store
            .set_voice_setting("test@s.whatsapp.net", "feminine")
            .unwrap();
        let translated = build_note(
            &state,
            PrepareRequest {
                contact_id: "test@s.whatsapp.net".into(),
                media_data: B64.encode(audio),
                reply_to: None,
                reply_to_sender: None,
                reply_to_text: None,
            },
            true,
            "test-note".into(),
        )
        .await
        .unwrap();
        assert_eq!(translated.target_language, "English");
        assert_eq!(translated.source_language, "Hungarian");
        assert_eq!(translated.transcript, "Jó reggelt");
        assert_eq!(translated.translation, "Good morning");
        assert_eq!(translated.voice, "shimmer");
        assert!(B64.decode(&translated.audio_data).unwrap().len() > 100);
        assert!(!translated.original_follow_up);
        assert!(translated.send_audio.is_empty());
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        mode.store(1, Ordering::SeqCst);
        state
            .store
            .set_voice_setting("outgoing", "masculine")
            .unwrap();
        let request = || PrepareRequest {
            contact_id: "test@s.whatsapp.net".into(),
            media_data: translated.original_data.clone(),
            reply_to: None,
            reply_to_sender: None,
            reply_to_text: None,
        };
        let outgoing = build_note(&state, request(), false, uuid::Uuid::new_v4().to_string())
            .await
            .unwrap();
        assert_eq!(outgoing.voice, "onyx");
        assert_eq!(outgoing.target_language, "Hungarian");
        assert!(B64
            .decode(&outgoing.send_audio)
            .unwrap()
            .starts_with(b"OggS"));
        assert!(B64
            .decode(&outgoing.send_original)
            .unwrap()
            .starts_with(b"OggS"));
        assert!(outgoing.original_follow_up);
        mode.store(2, Ordering::SeqCst);
        let (tx, mut commands) = tokio::sync::mpsc::channel(1);
        state.set_command_tx(tx).await;
        let response = json_response(prepare(State(state.clone()), Json(request())).await).await;
        assert!(response["error"].as_str().unwrap().contains("503"));
        assert!(
            commands.try_recv().is_err(),
            "A synthesis error must never send the original as fallback"
        );
        assert!(state
            .store
            .get_messages("test@s.whatsapp.net")
            .unwrap()
            .is_empty());

        assert_eq!(
            std::fs::read_dir(&dir.0)
                .unwrap()
                .filter_map(Result::ok)
                .filter(|entry| entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with("voice-tmp-"))
                .count(),
            0
        );
        server.abort();
    }
}
