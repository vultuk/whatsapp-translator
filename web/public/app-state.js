const DEFAULT_FILTERS = {
  unreadOnly: false,
  groupsOnly: false,
  draftsOnly: false,
};

function normalizeText(value) {
  return String(value || '').trim().toLowerCase();
}

function cloneObject(value) {
  return { ...(value || {}) };
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

function contactMatchesSearch(contact, draftPreview, messagePreview, normalizedQuery) {
  if (!normalizedQuery) return true;

  const haystack = [
    contact?.name,
    contact?.phone,
    contact?.id,
    draftPreview,
    messagePreview,
  ]
    .filter(Boolean)
    .join(' ')
    .toLowerCase();

  return haystack.includes(normalizedQuery);
}

export function getVisibleContacts({
  contacts,
  drafts = {},
  searchQuery = '',
  filters = DEFAULT_FILTERS,
  messagePreviewByContact = {},
}) {
  const normalizedQuery = normalizeText(searchQuery);
  const mergedFilters = { ...DEFAULT_FILTERS, ...(filters || {}) };

  return (contacts || []).filter((contact) => {
    const draftPreview = getDraftPreview(drafts, contact.id);
    const hasDraft = Boolean(draftPreview);

    if (mergedFilters.unreadOnly && !(contact.unreadCount > 0)) {
      return false;
    }

    if (mergedFilters.groupsOnly && contact.type !== 'group') {
      return false;
    }

    if (mergedFilters.draftsOnly && !hasDraft) {
      return false;
    }

    return contactMatchesSearch(
      contact,
      draftPreview,
      messagePreviewByContact?.[contact.id] || '',
      normalizedQuery,
    );
  });
}

export function countMatchingMessages(messages, query, options = {}) {
  return filterMessagesByQuery(messages, query, options).length;
}
