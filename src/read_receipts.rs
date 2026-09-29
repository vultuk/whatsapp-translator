//! Read receipts originate only in explicit viewed-message requests. Translation and sync do not enqueue them.
use crate::{bridge::BridgeCommand, web::AppState};
use std::sync::Arc;

pub fn start(state: Arc<AppState>) {
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(std::time::Duration::from_secs(2));
        loop {
            tick.tick().await;
            if let Err(error) = dispatch(&state).await {
                tracing::warn!("Read receipt retry: {error}");
            }
        }
    });
}

pub async fn dispatch(state: &AppState) -> anyhow::Result<()> {
    if !*state.connected.read().await {
        return Ok(());
    }
    let now = chrono::Utc::now().timestamp();
    for receipt in state.store.pending_read_receipts(now)? {
        state
            .store
            .read_receipt_attempted(&receipt.message_id, now)?;
        if let Err(error) = state
            .send_bridge_command(BridgeCommand::MarkRead {
                to: receipt.contact_id,
                message_id: receipt.message_id,
                timestamp: receipt.viewed_at,
                sender_jid: receipt.sender_jid,
            })
            .await
        {
            tracing::warn!("Unable to queue read receipt: {error}");
            break;
        }
    }
    Ok(())
}
