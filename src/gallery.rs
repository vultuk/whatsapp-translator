//! Paginated chat media and small previews; originals remain lazy-loaded.
use crate::web::{present_message_page, AppState};
use anyhow::{bail, Context, Result};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use serde::Deserialize;
use std::{collections::VecDeque, path::PathBuf, sync::Arc, time::Duration};
use tokio::{
    process::Command,
    sync::{Mutex, Semaphore},
};

pub struct ThumbnailCache {
    entries: Mutex<VecDeque<(String, String)>>,
    workers: Semaphore,
}

impl Default for ThumbnailCache {
    fn default() -> Self {
        Self {
            entries: Mutex::new(VecDeque::new()),
            workers: Semaphore::new(2),
        }
    }
}

#[derive(Deserialize)]
pub struct GalleryQuery {
    limit: Option<u32>,
    before: Option<i64>,
    #[serde(alias = "beforeId")]
    before_id: Option<String>,
}

pub async fn messages(
    State(state): State<Arc<AppState>>,
    Path(contact_id): Path<String>,
    Query(query): Query<GalleryQuery>,
) -> Response {
    let limit = query.limit.unwrap_or(60).clamp(1, 100);
    if query.before_id.is_some() && query.before.is_none() {
        return (
            StatusCode::BAD_REQUEST,
            "A gallery cursor needs a timestamp",
        )
            .into_response();
    }
    match state.store.get_gallery_messages(
        &contact_id,
        limit + 1,
        query.before,
        query.before_id.as_deref(),
    ) {
        Ok(mut messages) => {
            let has_more = messages.len() > limit as usize;
            if has_more {
                messages.remove(0);
            }
            present_message_page(&state.store, messages, has_more)
        }
        Err(error) => {
            tracing::error!("Could not load chat gallery: {error}");
            (StatusCode::INTERNAL_SERVER_ERROR, "Could not load gallery").into_response()
        }
    }
}

pub async fn thumbnail(State(state): State<Arc<AppState>>, Path(id): Path<String>) -> Response {
    // Recheck eligibility before serving cached previews, including after revocation.
    match state.store.message_is_visual_media(&id) {
        Ok(true) => {}
        Ok(false) => return (StatusCode::NOT_FOUND, "Visual media not found").into_response(),
        Err(_) => {
            return (StatusCode::INTERNAL_SERVER_ERROR, "Could not load media").into_response()
        }
    }
    let result = async {
        let _permit = state.gallery_thumbnails.workers.acquire().await?;
        {
            let mut cache = state.gallery_thumbnails.entries.lock().await;
            if let Some(index) = cache.iter().position(|(key, _)| key == &id) {
                let entry = cache.remove(index).unwrap();
                let encoded = entry.1.clone();
                cache.push_back(entry);
                return Ok::<_, anyhow::Error>(encoded);
            }
        }
        let (source, _) = state
            .store
            .get_message_media(&id)?
            .context("Media is unavailable")?;
        let preview = make_thumbnail(&source).await?;
        let encoded = B64.encode(preview);
        let mut cache = state.gallery_thumbnails.entries.lock().await;
        while cache.len() >= 192 {
            cache.pop_front();
        }
        cache.push_back((id, encoded.clone()));
        Ok(encoded)
    }
    .await;
    match result {
        Ok(encoded) => (
            [("Cache-Control", "private, no-store")],
            Json(serde_json::json!({
                "media_data": encoded, "mime_type": "image/jpeg"
            })),
        )
            .into_response(),
        Err(error) => {
            tracing::warn!("Gallery thumbnail unavailable: {error}");
            (
                StatusCode::UNPROCESSABLE_ENTITY,
                "Preview unavailable; open the original media",
            )
                .into_response()
        }
    }
}

struct TemporaryMedia(PathBuf);
impl Drop for TemporaryMedia {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

async fn make_thumbnail(source: &str) -> Result<Vec<u8>> {
    if source.len() > 128 * 1024 * 1024 {
        bail!("Media exceeds preview size limit");
    }
    let data = B64.decode(source).context("Invalid media encoding")?;
    let directory = TemporaryMedia(
        std::env::temp_dir().join(format!("babel-gallery-{}", uuid::Uuid::new_v4())),
    );
    tokio::fs::create_dir(&directory.0).await?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        tokio::fs::set_permissions(&directory.0, std::fs::Permissions::from_mode(0o700)).await?;
    }
    let input = directory.0.join("media");
    tokio::fs::write(&input, data).await?;
    let output = tokio::time::timeout(
        Duration::from_secs(15),
        Command::new("ffmpeg")
            .kill_on_drop(true)
            .args([
                "-nostdin",
                "-v",
                "error",
                "-threads",
                "1",
                "-filter_threads",
                "1",
                "-protocol_whitelist",
                "file,pipe",
                "-format_whitelist",
                "mov,matroska,avi,mpegts,mpegvideo,mjpeg,jpeg_pipe,png_pipe,webp_pipe,gif",
                "-i",
            ])
            .arg(&input)
            .args([
                "-map",
                "0:v:0",
                "-frames:v",
                "1",
                "-an",
                "-sn",
                "-vf",
                "scale=384:384:force_original_aspect_ratio=decrease",
                "-q:v",
                "5",
                "-threads",
                "1",
                "-f",
                "image2pipe",
                "-c:v",
                "mjpeg",
                "pipe:1",
            ])
            .output(),
    )
    .await
    .context("Preview timed out")??;
    if !output.status.success()
        || output.stdout.len() > 256 * 1024
        || !output.stdout.starts_with(&[0xff, 0xd8])
    {
        bail!("Could not decode visual media");
    }
    Ok(output.stdout)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn thumbnails_decode_real_photo_and_video_as_small_jpegs() {
        let dir = TemporaryMedia(
            std::env::temp_dir().join(format!("gallery-test-{}", uuid::Uuid::new_v4())),
        );
        std::fs::create_dir(&dir.0).unwrap();
        let photo = dir.0.join("photo.png");
        assert!(Command::new("ffmpeg")
            .args([
                "-v",
                "error",
                "-f",
                "lavfi",
                "-i",
                "color=c=teal:s=800x600",
                "-frames:v",
                "1"
            ])
            .arg(&photo)
            .status()
            .await
            .unwrap()
            .success());
        let video = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("ios/WhatsAppTranslator/Resources/video-playback-fixture.mp4");
        // CI may build only the backend source; use a generated video fixture there.
        let video = if video.exists() {
            video
        } else {
            let path = dir.0.join("video.mp4");
            assert!(Command::new("ffmpeg")
                .args([
                    "-v",
                    "error",
                    "-f",
                    "lavfi",
                    "-i",
                    "color=c=blue:s=160x120:d=1",
                    "-c:v",
                    "mpeg4"
                ])
                .arg(&path)
                .status()
                .await
                .unwrap()
                .success());
            path
        };
        for path in [photo, video] {
            let bytes = make_thumbnail(&B64.encode(std::fs::read(path).unwrap()))
                .await
                .unwrap();
            assert!(bytes.starts_with(&[0xff, 0xd8]));
            assert!(bytes.len() < 256 * 1024);
        }
        assert!(make_thumbnail(&B64.encode(b"not a media file"))
            .await
            .is_err());
    }
}
