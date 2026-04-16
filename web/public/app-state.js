const DEFAULT_FILTERS = {
  unreadOnly: false,
  groupsOnly: false,
  draftsOnly: false,
  pinnedOnly: false,
  notesOnly: false,
  dueRemindersOnly: false,
  labelsOnly: false,
  snoozedOnly: false,
  needsReplyOnly: false,
};

function normalizeText(value) {
  return String(value || '').trim().toLowerCase();
}

function cloneObject(value) {
  return { ...(value || {}) };
}

function normalizeTimestamp(value) {
  if (value === null || value === undefined || value === '') return null;
  const timestamp = Number(value);
  return Number.isFinite(timestamp) ? timestamp : null;
}

export function getContactDisplayName(contact = {}, metadata = {}) {
  const alias = String(metadata?.alias || '').trim();
  if (alias) return alias;

  if (contact?.name) return contact.name;
  if (contact?.phone) return `+${contact.phone}`;
  if (contact?.type === 'group') return 'Group Chat';

  const phoneFromJid = String(contact?.id || '').split('@')[0];
  return phoneFromJid ? `+${phoneFromJid}` : 'Unknown';
}

export function parseLabelsInput(value) {
  const seen = new Set();
  return String(value || '')
    .split(/[\n,]/)
    .map(label => label.trim())
    .filter(Boolean)
    .filter((label) => {
      const key = label.toLowerCase();
      if (seen.has(key)) return false;
      seen.add(key);
      return true;
    });
}

export function getContactLabels(metadata = {}) {
  if (Array.isArray(metadata?.labels)) {
    return parseLabelsInput(metadata.labels.join(','));
  }
  return parseLabelsInput(metadata?.labelsText || metadata?.labels || '');
}

export function isContactSnoozed(metadata = {}, now = Date.now()) {
  const snoozedUntil = normalizeTimestamp(metadata?.snoozedUntil);
  return Boolean(snoozedUntil && snoozedUntil > now);
}

export function getReminderStatus(metadata = {}, now = Date.now()) {
  const reminderAt = normalizeTimestamp(metadata?.reminderAt);
  const reminderText = normalizeText(metadata?.reminderText);
  if (!reminderAt || !reminderText) return 'none';
  return reminderAt <= now ? 'due' : 'upcoming';
}

export function getDraftText(drafts, contactId) {
  return drafts?.[contactId]?.text || '';
}

export function upsertDraft(drafts, contactId, text, updatedAt = Date.now()) {
  const nextDrafts = cloneObject(drafts);
  const trimmed = String(text || '').trim();

  if (!contactId) {
    return nextDrafts;
  }

  if (!trimmed) {
    delete nextDrafts[contactId];
    return nextDrafts;
  }

  nextDrafts[contactId] = {
    text: trimmed,
    updatedAt,
  };

  return nextDrafts;
}

export function getDraftPreview(drafts, contactId, maxLength = 48) {
  const draft = getDraftText(drafts, contactId);
  if (!draft) return '';

  const preview = draft.length > maxLength ? `${draft.slice(0, maxLength - 1)}…` : draft;
  return `Draft: ${preview}`;
}

export function getReplyState({
  contact = {},
  messages = [],
  drafts = {},
  metadata = {},
  contactId = contact?.id,
  now = Date.now(),
} = {}) {
  if (isContactSnoozed(metadata, now)) {
    return 'snoozed';
  }

  if (getDraftText(drafts, contactId)) {
    return 'drafting';
  }

  const latestMessage = Array.isArray(messages) && messages.length > 0
    ? messages[messages.length - 1]
    : null;

  if (latestMessage) {
    return latestMessage.isFromMe || latestMessage.is_from_me ? 'waiting' : 'needs-reply';
  }

  if ((contact?.unreadCount || 0) > 0) {
    return 'needs-reply';
  }

  return 'idle';
}

function createQuickReplyId(text, updatedAt) {
  const slug = String(text || '')
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/^-+|-+$/g, '')
    .slice(0, 24) || 'quick-reply';
  return `${slug}-${updatedAt}`;
}

export function upsertQuickReply(quickReplies, text, updatedAt = Date.now(), maxItems = 8) {
  const trimmed = String(text || '').trim();
  if (!trimmed) return Array.isArray(quickReplies) ? [...quickReplies] : [];

  const normalized = trimmed.toLowerCase();
  const nextReplies = (Array.isArray(quickReplies) ? quickReplies : [])
    .filter(reply => normalizeText(reply?.text) !== normalized)
    .map(reply => ({ ...reply }));

  nextReplies.unshift({
    id: createQuickReplyId(trimmed, updatedAt),
    text: trimmed,
    updatedAt,
  });

  return nextReplies.slice(0, Math.max(1, maxItems));
}

export function removeQuickReply(quickReplies, quickReplyId) {
  return (Array.isArray(quickReplies) ? quickReplies : [])
    .filter(reply => reply?.id !== quickReplyId)
    .map(reply => ({ ...reply }));
}

function messageSnippet(message, maxLength = 140) {
  const content = message?.content || {};
  const parts = [
    content.body,
    content.text,
    content.caption,
    message?.originalText,
    message?.original_text,
    message?.translatedText,
    message?.translated_text,
  ].filter(Boolean);

  const text = String(parts[0] || '').trim();
  if (!text) return '[Message]';
  return text.length > maxLength ? `${text.slice(0, maxLength - 1)}…` : text;
}

export function toggleStarredMessage(starredLookup, message, updatedAt = Date.now()) {
  const nextLookup = cloneObject(starredLookup);
  const id = message?.id;
  if (!id) return nextLookup;

  if (nextLookup[id]) {
    delete nextLookup[id];
    return nextLookup;
  }

  nextLookup[id] = {
    id,
    contactId: message.contactId || message.contact_id || '',
    timestamp: message.timestamp || Date.now(),
    updatedAt,
    snippet: messageSnippet(message),
  };

  return nextLookup;
}

export function isMessageStarred(starredLookup, messageId) {
  return Boolean(messageId && starredLookup?.[messageId]);
}

function messageSearchText(message) {
  const content = message?.content || {};
  return [
    content.body,
    content.text,
    content.caption,
    message?.originalText,
    message?.original_text,
    message?.translatedText,
    message?.translated_text,
    message?.senderName,
    message?.sender_name,
  ]
    .filter(Boolean)
    .join(' ')
    .toLowerCase();
}

export function filterMessagesByQuery(messages, query, options = {}) {
  const { starredOnly = false, starredLookup = {} } = options;
  const normalizedQuery = normalizeText(query);

  return (messages || []).filter((message) => {
    if (starredOnly && !isMessageStarred(starredLookup, message.id)) {
      return false;
    }

    if (!normalizedQuery) {
      return true;
    }

    return messageSearchText(message).includes(normalizedQuery);
  });
}

function getMetadata(metadataByContact, contactId) {
  return metadataByContact?.[contactId] || {};
}

function contactMatchesSearch(contact, draftPreview, messagePreview, metadata, normalizedQuery) {
  if (!normalizedQuery) return true;

  const haystack = [
    contact?.name,
    contact?.phone,
    contact?.id,
    metadata?.alias,
    draftPreview,
    messagePreview,
    metadata?.notes,
    metadata?.notePreview,
    metadata?.reminderText,
    getContactLabels(metadata).join(' '),
  ]
    .filter(Boolean)
    .join(' ')
    .toLowerCase();

  const tokens = normalizedQuery.split(/\s+/).filter(Boolean);
  return tokens.every(token => haystack.includes(token));
}

export function getVisibleContacts({
  contacts,
  drafts = {},
  searchQuery = '',
  filters = DEFAULT_FILTERS,
  messagePreviewByContact = {},
  metadataByContact = {},
  messagesByContact = {},
  now = Date.now(),
}) {
  const normalizedQuery = normalizeText(searchQuery);
  const mergedFilters = { ...DEFAULT_FILTERS, ...(filters || {}) };

  return (contacts || []).filter((contact) => {
    const draftPreview = getDraftPreview(drafts, contact.id);
    const hasDraft = Boolean(draftPreview);
    const metadata = getMetadata(metadataByContact, contact.id);
    const isPinned = metadata?.pinnedAt != null || contact?.pinnedAt != null;
    const hasNotes = Boolean(normalizeText(metadata?.notes || metadata?.notePreview));
    const labels = getContactLabels(metadata);
    const hasLabels = labels.length > 0;
    const reminderStatus = getReminderStatus(metadata, now);
    const isSnoozed = isContactSnoozed(metadata, now);
    const replyState = getReplyState({
      contact,
      messages: messagesByContact?.[contact.id] || [],
      drafts,
      metadata,
      contactId: contact.id,
      now,
    });

    if (isSnoozed && !mergedFilters.snoozedOnly) {
      return false;
    }

    if (mergedFilters.unreadOnly && !(contact.unreadCount > 0)) {
      return false;
    }

    if (mergedFilters.groupsOnly && contact.type !== 'group') {
      return false;
    }

    if (mergedFilters.draftsOnly && !hasDraft) {
      return false;
    }

    if (mergedFilters.pinnedOnly && !isPinned) {
      return false;
    }

    if (mergedFilters.notesOnly && !hasNotes) {
      return false;
    }

    if (mergedFilters.dueRemindersOnly && reminderStatus !== 'due') {
      return false;
    }

    if (mergedFilters.labelsOnly && !hasLabels) {
      return false;
    }

    if (mergedFilters.snoozedOnly && !isSnoozed) {
      return false;
    }

    if (mergedFilters.needsReplyOnly && !['needs-reply', 'drafting'].includes(replyState)) {
      return false;
    }

    return contactMatchesSearch(
      contact,
      draftPreview,
      messagePreviewByContact?.[contact.id] || '',
      metadata,
      normalizedQuery,
    );
  });
}

export function countMatchingMessages(messages, query, options = {}) {
  return filterMessagesByQuery(messages, query, options).length;
}
