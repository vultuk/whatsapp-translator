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
  importantOnly: false,
  tasksOnly: false,
};

const PRIORITY_CONFIG = {
  urgent: { value: 'urgent', label: 'Urgent', rank: 0, isImportant: true },
  high: { value: 'high', label: 'High', rank: 1, isImportant: true },
  normal: { value: 'normal', label: 'Normal', rank: 2, isImportant: false },
  low: { value: 'low', label: 'Low', rank: 3, isImportant: false },
};

const APPEARANCE_THEME_CONFIG = {
  whatsapp: {
    label: 'WhatsApp',
    light: {
      label: 'WhatsApp',
      themeColor: '#f7f5f2',
      accentColor: '#128c7e',
      surfaceColor: '#ffffff',
      textColor: '#111b21',
    },
    dark: {
      label: 'WhatsApp',
      themeColor: '#111b21',
      accentColor: '#00a884',
      surfaceColor: '#202c33',
      textColor: '#e9edef',
    },
  },
  ocean: {
    label: 'Ocean',
    light: {
      label: 'Ocean',
      themeColor: '#f4f7fb',
      accentColor: '#0f6cbd',
      surfaceColor: '#ffffff',
      textColor: '#102a43',
    },
    dark: {
      label: 'Ocean',
      themeColor: '#0f172a',
      accentColor: '#38bdf8',
      surfaceColor: '#162033',
      textColor: '#e0f2fe',
    },
  },
  sunset: {
    label: 'Sunset',
    light: {
      label: 'Sunset',
      themeColor: '#fff6ef',
      accentColor: '#dd6b20',
      surfaceColor: '#ffffff',
      textColor: '#4a2c1d',
    },
    dark: {
      label: 'Sunset',
      themeColor: '#1f1720',
      accentColor: '#f97316',
      surfaceColor: '#2a1f2d',
      textColor: '#fde7d8',
    },
  },
  github: {
    label: 'GitHub',
    light: {
      label: 'GitHub',
      themeColor: '#f6f8fa',
      accentColor: '#0969da',
      surfaceColor: '#ffffff',
      textColor: '#1f2328',
    },
    dark: {
      label: 'GitHub',
      themeColor: '#0d1117',
      accentColor: '#2f81f7',
      surfaceColor: '#161b22',
      textColor: '#e6edf3',
    },
  },
  dracula: {
    label: 'Dracula',
    light: {
      label: 'Dracula',
      themeColor: '#f7f4ff',
      accentColor: '#7c3aed',
      surfaceColor: '#ffffff',
      textColor: '#2d2140',
    },
    dark: {
      label: 'Dracula',
      themeColor: '#282a36',
      accentColor: '#bd93f9',
      surfaceColor: '#303341',
      textColor: '#f8f8f2',
    },
  },
  nord: {
    label: 'Nord',
    light: {
      label: 'Nord',
      themeColor: '#eceff4',
      accentColor: '#5e81ac',
      surfaceColor: '#ffffff',
      textColor: '#2e3440',
    },
    dark: {
      label: 'Nord',
      themeColor: '#2e3440',
      accentColor: '#88c0d0',
      surfaceColor: '#3b4252',
      textColor: '#eceff4',
    },
  },
  linear: {
    label: 'Linear',
    light: {
      label: 'Linear',
      themeColor: '#f7f8f8',
      accentColor: '#5e6ad2',
      surfaceColor: '#ffffff',
      textColor: '#171717',
    },
    dark: {
      label: 'Linear',
      themeColor: '#08090a',
      accentColor: '#7170ff',
      surfaceColor: '#191a1b',
      textColor: '#f7f8f8',
    },
  },
  vercel: {
    label: 'Vercel',
    light: {
      label: 'Vercel',
      themeColor: '#ffffff',
      accentColor: '#0070f3',
      surfaceColor: '#ffffff',
      textColor: '#171717',
    },
    dark: {
      label: 'Vercel',
      themeColor: '#000000',
      accentColor: '#3291ff',
      surfaceColor: '#111111',
      textColor: '#fafafa',
    },
  },
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

function normalizePriority(value) {
  const normalized = normalizeText(value);
  return PRIORITY_CONFIG[normalized] ? normalized : 'normal';
}

function normalizeHexColor(value) {
  const color = String(value || '').trim();
  const shortMatch = color.match(/^#([\da-f]{3})$/i);
  if (shortMatch) {
    return `#${shortMatch[1].split('').map(char => char + char).join('')}`.toLowerCase();
  }
  const longMatch = color.match(/^#([\da-f]{6})$/i);
  return longMatch ? `#${longMatch[1].toLowerCase()}` : null;
}

function hexToRgb(color) {
  const normalized = normalizeHexColor(color);
  if (!normalized) return null;
  return {
    r: Number.parseInt(normalized.slice(1, 3), 16),
    g: Number.parseInt(normalized.slice(3, 5), 16),
    b: Number.parseInt(normalized.slice(5, 7), 16),
  };
}

function getRelativeLuminance(color) {
  const rgb = hexToRgb(color);
  if (!rgb) return null;
  const toLinear = channel => {
    const value = channel / 255;
    return value <= 0.03928 ? value / 12.92 : ((value + 0.055) / 1.055) ** 2.4;
  };
  return (0.2126 * toLinear(rgb.r)) + (0.7152 * toLinear(rgb.g)) + (0.0722 * toLinear(rgb.b));
}

export function getContrastRatio(foreground, background) {
  const fg = getRelativeLuminance(foreground);
  const bg = getRelativeLuminance(background);
  if (fg == null || bg == null) return 1;
  const lighter = Math.max(fg, bg);
  const darker = Math.min(fg, bg);
  return Number(((lighter + 0.05) / (darker + 0.05)).toFixed(2));
}

export function getAppearanceThemeCatalog() {
  return Object.entries(APPEARANCE_THEME_CONFIG).map(([id, theme]) => ({
    id,
    label: theme.label,
    light: { ...theme.light },
    dark: { ...theme.dark },
  }));
}

export function resolveAppearanceTheme(preferences = {}, options = {}) {
  const requestedTheme = normalizeText(preferences?.theme);
  const theme = APPEARANCE_THEME_CONFIG[requestedTheme] ? requestedTheme : 'whatsapp';
  const requestedMode = normalizeText(preferences?.mode);
  const mode = ['light', 'dark', 'system'].includes(requestedMode) ? requestedMode : 'system';
  const systemMode = normalizeText(options?.systemMode) === 'dark' ? 'dark' : 'light';
  const resolvedMode = mode === 'system' ? systemMode : mode;
  const definition = { ...APPEARANCE_THEME_CONFIG[theme][resolvedMode] };

  return {
    theme,
    mode,
    resolvedMode,
    definition,
    dataTheme: `${theme}-${resolvedMode}`,
  };
}

function createChecklistId(text, updatedAt, index = 0) {
  const slug = String(text || '')
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/^-+|-+$/g, '')
    .slice(0, 32) || 'task';
  return `${slug}-${updatedAt}-${index}`;
}

function normalizeChecklistText(value) {
  return String(value || '')
    .replace(/^[\-•*]\s*/, '')
    .trim();
}

function parseChecklistLine(line) {
  const raw = String(line || '').trim();
  if (!raw) return null;

  const match = raw.match(/^(?:[-*•]\s*)?\[( |x|X)\]\s*(.+)$/);
  if (match) {
    return {
      text: normalizeChecklistText(match[2]),
      done: match[1].toLowerCase() === 'x',
    };
  }

  const plainText = normalizeChecklistText(raw);
  if (!plainText) return null;
  return { text: plainText, done: false };
}

function getChecklistItems(metadata = {}) {
  return Array.isArray(metadata?.checklist) ? metadata.checklist : [];
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

export function getPriorityInfo(metadata = {}) {
  const value = normalizePriority(metadata?.priority);
  return { ...PRIORITY_CONFIG[value] };
}

export function parseChecklistInput(value) {
  const seen = new Set();
  return String(value || '')
    .split(/\n+/)
    .map(parseChecklistLine)
    .filter(Boolean)
    .filter((item) => {
      const key = normalizeText(item.text);
      if (!key || seen.has(key)) return false;
      seen.add(key);
      return true;
    });
}

export function upsertChecklistItems(existingItems, text, updatedAt = Date.now()) {
  const previous = Array.isArray(existingItems) ? existingItems : [];
  const existingByText = new Map(
    previous.map(item => [normalizeText(item?.text), { ...item }]).filter(([key]) => Boolean(key)),
  );

  return parseChecklistInput(text).map((item, index) => {
    const key = normalizeText(item.text);
    const previousItem = existingByText.get(key);
    return {
      id: previousItem?.id || createChecklistId(item.text, updatedAt, index),
      text: item.text,
      done: item.done,
      updatedAt: previousItem?.updatedAt || updatedAt,
    };
  });
}

export function toggleChecklistItem(items, itemId, updatedAt = Date.now()) {
  return (Array.isArray(items) ? items : []).map((item) => {
    if (item?.id !== itemId) {
      return { ...item };
    }

    return {
      ...item,
      done: !item.done,
      updatedAt,
    };
  });
}

export function getChecklistSummary(metadata = {}) {
  const checklist = getChecklistItems(metadata);
  const total = checklist.length;
  const completed = checklist.filter(item => item?.done).length;
  const open = Math.max(0, total - completed);
  let label = 'No tasks';
  if (open > 0) {
    label = `${open} open task${open === 1 ? '' : 's'}`;
  } else if (total > 0) {
    label = 'All tasks done';
  }

  return { total, completed, open, label };
}

export function getTimezoneInfo(metadata = {}, now = Date.now()) {
  const timezone = String(metadata?.timezone || '').trim();
  if (!timezone) return null;

  try {
    const parts = new Intl.DateTimeFormat('en-GB', {
      timeZone: timezone,
      hour: '2-digit',
      minute: '2-digit',
      hour12: false,
    }).formatToParts(new Date(now));
    const hour = Number(parts.find(part => part.type === 'hour')?.value ?? '0');
    const minute = parts.find(part => part.type === 'minute')?.value ?? '00';
    const localTime = `${String(hour).padStart(2, '0')}:${minute}`;

    let status = 'daytime';
    let statusLabel = 'Daytime';
    if (hour >= 9 && hour < 18) {
      status = 'working-hours';
      statusLabel = 'Working hours';
    } else if (hour >= 6 && hour < 9) {
      status = 'morning';
      statusLabel = 'Morning';
    } else if (hour >= 18 && hour < 22) {
      status = 'evening';
      statusLabel = 'Evening';
    } else {
      status = 'quiet-hours';
      statusLabel = 'Quiet hours';
    }

    return {
      timezone,
      localTime,
      label: `${localTime} local time`,
      status,
      statusLabel,
    };
  } catch {
    return null;
  }
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

  const priority = getPriorityInfo(metadata);
  const checklistSummary = getChecklistSummary(metadata);
  const timezoneInfo = getTimezoneInfo(metadata);
  const checklistSearchText = getChecklistItems(metadata)
    .map(item => item?.text)
    .filter(Boolean)
    .join(' ');

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
    priority.label,
    checklistSummary.label,
    checklistSearchText,
    timezoneInfo?.timezone,
    timezoneInfo?.label,
    timezoneInfo?.statusLabel,
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
    const priority = getPriorityInfo(metadata);
    const checklistSummary = getChecklistSummary(metadata);
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

    if (mergedFilters.importantOnly && !priority.isImportant) {
      return false;
    }

    if (mergedFilters.tasksOnly && checklistSummary.open === 0) {
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
