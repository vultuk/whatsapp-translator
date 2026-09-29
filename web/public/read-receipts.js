// Only viewport samples can create receipt work. Fetching, translating and live events cannot.
export class ViewedMessageTracker {
  constructor({send, save = () => {}, pending = {}, dwell = 700}) {
    this.send = send;
    this.save = save;
    this.dwell = dwell;
    this.since = new Map();
    this.completed = new Set();
    this.pending = new Map(Object.entries(pending).map(([chat, ids]) => [chat, new Set(ids)]));
    this.sending = false;
  }

  sample(messages, {active, now}) {
    const visible = new Set();
    if (active) for (const message of messages) {
      if (!canSendReadReceipt(message)) continue;
      const key = `${message.contactId}:${message.id}`;
      visible.add(key);
      if (!this.since.has(key)) this.since.set(key, now);
      if (now - this.since.get(key) >= this.dwell && !this.completed.has(key)) {
        if (!this.pending.has(message.contactId)) this.pending.set(message.contactId, new Set());
        this.pending.get(message.contactId).add(message.id);
        this.completed.add(key);
        this.persist();
      }
    }
    for (const key of this.since.keys()) if (!visible.has(key)) this.since.delete(key);
  }

  persist() { this.save(Object.fromEntries([...this.pending].map(([chat, ids]) => [chat, [...ids]]))); }

  async flush() {
    if (this.sending) return;
    this.sending = true;
    try {
      for (const [chat, ids] of this.pending) {
        const batch = [...ids].slice(0, 200);
        await this.send(chat, batch);
        for (const id of batch) ids.delete(id);
        if (!ids.size) this.pending.delete(chat);
        this.persist();
      }
    } finally { this.sending = false; }
  }
}

export function canSendReadReceipt(message) {
  const type = String(message.content?.type || message.contentType || message.content_type || '').toLowerCase();
  return Boolean(message.id && message.contactId && !(message.isFromMe || message.is_from_me)
    && !message.contactId.endsWith('@broadcast') && !message.contactId.endsWith('@newsletter')
    && !['reaction', 'revoked', 'protocol', 'unknown'].includes(type));
}

export function isMessageVisible(rect, viewport) {
  const height = Math.max(0, Math.min(rect.bottom, viewport.bottom) - Math.max(rect.top, viewport.top));
  const width = Math.max(0, Math.min(rect.right, viewport.right) - Math.max(rect.left, viewport.left));
  return rect.height > 0 && rect.width > 0 && height >= Math.min(rect.height * 0.5, 120) && width >= rect.width * 0.5;
}
