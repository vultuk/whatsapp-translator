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

export function normalizeInboxFilters(filters = {}, { controlsAvailable = true } = {}) {
  if (!controlsAvailable) {
    return { ...DEFAULT_FILTERS };
  }

  return Object.fromEntries(
    Object.keys(DEFAULT_FILTERS).map(key => [key, Boolean(filters?.[key])]),
  );
}

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

const COMPOSER_LANGUAGE_PRESETS = ['Spanish', 'French', 'Japanese'];
const COMPOSER_TONE_PRESETS = ['Friendly', 'Formal', 'Concise'];
const COMPOSER_REMINDER_PRESETS = [
  { id: 'later-today', label: 'Later today', hours: 3 },
  { id: 'tomorrow', label: 'Tomorrow 9 AM', days: 1, hour: 9 },
  { id: 'next-week', label: 'Next week', days: 7, hour: 9 },
];

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

export function getMessageSnippet(message, maxLength = 140) {
  return messageSnippet(message, maxLength);
}

export function getUntranslatedIncomingMessages(messages = []) {
  return (Array.isArray(messages) ? messages : []).filter((message) => {
    const isOutgoing = Boolean(message?.isFromMe || message?.is_from_me);
    const isTranslated = Boolean(message?.isTranslated || message?.is_translated);
    const content = message?.content || {};
    const hasText = Boolean(content.body || content.caption || content.text);
    return !isOutgoing && !isTranslated && hasText;
  });
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

    // A typed search is global: inbox filters and snoozing must not hide a matching chat.
    if (normalizedQuery) {
      return contactMatchesSearch(
        contact,
        draftPreview,
        messagePreviewByContact?.[contact.id] || '',
        metadata,
        normalizedQuery,
      );
    }

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

export function getContactActionSnapshot({
  contact = {},
  drafts = {},
  metadata = {},
  messages = [],
  now = Date.now(),
} = {}) {
  const contactId = contact?.id || '';
  const priority = getPriorityInfo(metadata);
  const checklist = getChecklistSummary(metadata);
  const reminderStatus = getReminderStatus(metadata, now);
  const replyState = getReplyState({
    contact,
    messages,
    drafts,
    metadata,
    contactId,
    now,
  });
  const draft = drafts?.[contactId] || null;
  const hasDraft = Boolean(getDraftText(drafts, contactId));
  const isPinned = metadata?.pinnedAt != null || contact?.pinnedAt != null;
  const isSnoozed = isContactSnoozed(metadata, now);
  const labels = getContactLabels(metadata);
  const timezoneInfo = getTimezoneInfo(metadata, now);
  const unreadCount = Math.max(0, Number(contact?.unreadCount || 0));
  const lastMessageTime = normalizeTimestamp(contact?.lastMessageTime)
    || normalizeTimestamp(messages?.[messages.length - 1]?.timestamp)
    || 0;
  const reminderAt = normalizeTimestamp(metadata?.reminderAt);
  const draftUpdatedAt = normalizeTimestamp(draft?.updatedAt);

  const reasons = [];
  if (reminderStatus === 'due') {
    reasons.push('Reminder due');
  } else if (replyState === 'drafting') {
    reasons.push('Draft reply saved');
  } else if (replyState === 'needs-reply') {
    reasons.push('Needs your reply');
  } else if (replyState === 'waiting') {
    reasons.push('Waiting for them');
  }

  if (checklist.open > 0) {
    reasons.push(`${checklist.open} open task${checklist.open === 1 ? '' : 's'}`);
  }

  if (priority.value !== 'normal') {
    reasons.push(`${priority.label} priority`);
  }

  if (hasDraft && !reasons.includes('Draft reply saved')) {
    reasons.push('Draft saved');
  }

  if (timezoneInfo && reasons.length < 3) {
    reasons.push(`${timezoneInfo.localTime} local time`);
  }

  if (labels.length > 0 && reasons.length < 3) {
    reasons.push(labels.slice(0, 2).join(' · '));
  }

  let attentionScore = 0;
  if (reminderStatus === 'due') {
    attentionScore += 130;
  } else if (reminderStatus === 'upcoming') {
    attentionScore += 48;
  }

  if (replyState === 'drafting') {
    attentionScore += 112;
  } else if (replyState === 'needs-reply') {
    attentionScore += 96;
  } else if (replyState === 'waiting') {
    attentionScore += 18;
  }

  attentionScore += checklist.open * 14;
  attentionScore += unreadCount * 4;
  if (priority.value === 'urgent') {
    attentionScore += 52;
  } else if (priority.value === 'high') {
    attentionScore += 36;
  } else if (priority.value === 'low') {
    attentionScore -= 10;
  }

  if (isPinned) {
    attentionScore += 14;
  }
  if (hasDraft) {
    attentionScore += 10;
  }
  if (labels.length > 0) {
    attentionScore += Math.min(10, labels.length * 2);
  }
  if (isSnoozed) {
    attentionScore -= 160;
  }

  return {
    contactId,
    unreadCount,
    hasDraft,
    draftUpdatedAt,
    isPinned,
    isSnoozed,
    priority,
    checklist,
    labels,
    replyState,
    reminderStatus,
    reminderAt,
    timezoneInfo,
    lastMessageTime,
    attentionScore,
    headline: reasons[0] || 'Recent activity',
    summary: reasons.slice(0, 3).join(' • '),
  };
}

export function buildVisitorDashboard({
  contacts = [],
  drafts = {},
  metadataByContact = {},
  messagesByContact = {},
  now = Date.now(),
  maxFocusContacts = 5,
  maxUpcomingReminders = 4,
} = {}) {
  const actionSnapshots = (contacts || [])
    .map((contact) => {
      const metadata = getMetadata(metadataByContact, contact?.id);
      return {
        contact,
        metadata,
        snapshot: getContactActionSnapshot({
          contact,
          drafts,
          metadata,
          messages: messagesByContact?.[contact?.id] || [],
          now,
        }),
      };
    });

  const activeSnapshots = actionSnapshots.filter(entry => !entry.snapshot.isSnoozed);

  const focusContacts = activeSnapshots
    .filter((entry) => {
      const snapshot = entry.snapshot;
      return snapshot.attentionScore > 0
        && (
          snapshot.replyState === 'drafting'
          || snapshot.replyState === 'needs-reply'
          || snapshot.reminderStatus === 'due'
          || snapshot.checklist.open > 0
          || snapshot.priority.isImportant
          || snapshot.hasDraft
        );
    })
    .sort((a, b) => {
      if (b.snapshot.attentionScore !== a.snapshot.attentionScore) {
        return b.snapshot.attentionScore - a.snapshot.attentionScore;
      }
      return (b.snapshot.lastMessageTime || 0) - (a.snapshot.lastMessageTime || 0);
    })
    .slice(0, Math.max(1, maxFocusContacts));

  const upcomingReminders = activeSnapshots
    .filter(entry => entry.snapshot.reminderStatus === 'upcoming' && entry.snapshot.reminderAt)
    .sort((a, b) => {
      if (a.snapshot.reminderAt !== b.snapshot.reminderAt) {
        return a.snapshot.reminderAt - b.snapshot.reminderAt;
      }
      return b.snapshot.attentionScore - a.snapshot.attentionScore;
    })
    .slice(0, Math.max(1, maxUpcomingReminders));

  const stats = {
    needsReply: activeSnapshots.filter((entry) => ['needs-reply', 'drafting'].includes(entry.snapshot.replyState)).length,
    dueReminders: activeSnapshots.filter(entry => entry.snapshot.reminderStatus === 'due').length,
    drafts: activeSnapshots.filter(entry => entry.snapshot.hasDraft).length,
    openTasks: activeSnapshots.reduce((sum, entry) => sum + entry.snapshot.checklist.open, 0),
  };

  return {
    stats,
    focusContacts,
    upcomingReminders,
    totalContacts: (contacts || []).length,
    snoozedContacts: actionSnapshots.filter(entry => entry.snapshot.isSnoozed).length,
  };
}

export function createDemoWorkspace(now = Date.now()) {
  const minutesAgo = minutes => now - minutes * 60_000;
  const minutesFromNow = minutes => now + minutes * 60_000;

  const contacts = [
    {
      id: 'demo-casa-azul',
      name: 'Sofia at Casa Azul',
      phone: '34600111222',
      type: 'private',
      unreadCount: 2,
      pinnedAt: minutesAgo(50),
      lastMessageTime: minutesAgo(4),
      lastMessagePreview: 'Perfect, can you arrive after 18:00?',
    },
    {
      id: 'demo-tutor',
      name: 'Jules (French tutor)',
      phone: '33677889900',
      type: 'private',
      unreadCount: 0,
      lastMessageTime: minutesAgo(42),
      lastMessagePreview: 'Draft saved: send the corrected deck after lunch.',
    },
    {
      id: 'demo-osaka-group',
      name: 'Osaka arrival group',
      phone: null,
      type: 'group',
      unreadCount: 1,
      lastMessageTime: minutesAgo(95),
      lastMessagePreview: 'Venue pin still needs confirming.',
    },
    {
      id: 'demo-plumber',
      name: 'Theo the plumber',
      phone: '447700900123',
      type: 'private',
      unreadCount: 0,
      lastMessageTime: minutesAgo(180),
      lastMessagePreview: 'Snoozed until tomorrow morning.',
    },
  ];

  const messagesByContact = {
    'demo-casa-azul': [
      {
        id: 'demo-casa-1',
        contactId: 'demo-casa-azul',
        timestamp: minutesAgo(21),
        isFromMe: false,
        senderName: 'Sofia',
        senderJid: '34600111222@s.whatsapp.net',
        content: { type: 'text', body: 'Las llaves estan en la caja junto a la puerta azul.' },
        originalText: 'Las llaves estan en la caja junto a la puerta azul.',
        translatedText: 'The keys are in the box next to the blue door.',
        sourceLanguage: 'Spanish',
        isTranslated: true,
        reactions: { '🙏': ['me'] },
      },
      {
        id: 'demo-casa-2',
        contactId: 'demo-casa-azul',
        timestamp: minutesAgo(14),
        isFromMe: true,
        senderJid: 'demo@s.whatsapp.net',
        content: { type: 'text', body: '18:30 works for us. I will send the taxi plate when we leave the station.' },
        translatedText: '18:30 nos va bien. Te enviare la matricula del taxi cuando salgamos de la estacion.',
        sourceLanguage: 'Spanish',
        isTranslated: true,
      },
      {
        id: 'demo-casa-3',
        contactId: 'demo-casa-azul',
        timestamp: minutesAgo(4),
        isFromMe: false,
        senderName: 'Sofia',
        senderJid: '34600111222@s.whatsapp.net',
        content: { type: 'text', body: 'Perfecto, puede llegar despues de las 18:00?' },
        originalText: 'Perfecto, puede llegar despues de las 18:00?',
        translatedText: 'Perfect, can you arrive after 18:00?',
        sourceLanguage: 'Spanish',
        isTranslated: true,
      },
    ],
    'demo-tutor': [
      {
        id: 'demo-tutor-1',
        contactId: 'demo-tutor',
        timestamp: minutesAgo(52),
        isFromMe: false,
        senderName: 'Jules',
        senderJid: '33677889900@s.whatsapp.net',
        content: { type: 'text', body: 'Peux-tu envoyer la version corrigee apres le dejeuner ?' },
      },
      {
        id: 'demo-tutor-2',
        contactId: 'demo-tutor',
        timestamp: minutesAgo(42),
        isFromMe: true,
        senderJid: 'demo@s.whatsapp.net',
        content: { type: 'text', body: 'Yes, I will send the corrected version after lunch.' },
        translatedText: 'Oui, je vais envoyer la version corrigee apres le dejeuner.',
        sourceLanguage: 'French',
        isTranslated: true,
      },
    ],
    'demo-osaka-group': [
      {
        id: 'demo-osaka-1',
        contactId: 'demo-osaka-group',
        timestamp: minutesAgo(105),
        isFromMe: false,
        senderName: 'Mika',
        senderJid: '819012345678@s.whatsapp.net',
        chatType: 'group',
        content: { type: 'text', body: 'Venue pin still needs confirming before everyone heads out.' },
      },
      {
        id: 'demo-osaka-2',
        contactId: 'demo-osaka-group',
        timestamp: minutesAgo(95),
        isFromMe: false,
        senderName: 'Ken',
        senderJid: '819098765432@s.whatsapp.net',
        chatType: 'group',
        content: { type: 'text', body: '駅の出口は何番ですか?' },
        originalText: '駅の出口は何番ですか?',
        translatedText: 'Which station exit should we use?',
        sourceLanguage: 'Japanese',
        isTranslated: true,
      },
    ],
    'demo-plumber': [
      {
        id: 'demo-plumber-1',
        contactId: 'demo-plumber',
        timestamp: minutesAgo(180),
        isFromMe: false,
        senderName: 'Theo',
        senderJid: '447700900123@s.whatsapp.net',
        content: { type: 'text', body: 'I will send the boiler quote tomorrow morning.' },
      },
    ],
  };

  const metadataByContact = {
    'demo-casa-azul': {
      alias: 'Casa Azul host',
      priority: 'urgent',
      labels: ['Travel', 'VIP'],
      notes: 'Prefers concise Spanish replies. Confirm timings and transport details clearly.',
      timezone: 'Europe/Madrid',
      targetLanguage: 'Spanish',
      reminderText: 'Send the taxi plate before arrival',
      reminderAt: minutesAgo(1),
      checklist: [
        { id: 'demo-casa-task-1', text: 'Confirm check-in time', done: false, updatedAt: minutesAgo(30) },
        { id: 'demo-casa-task-2', text: 'Send taxi plate', done: false, updatedAt: minutesAgo(30) },
      ],
    },
    'demo-tutor': {
      priority: 'normal',
      labels: ['French', 'Learning'],
      notes: 'Friendly tone. He likes short context and bullet points.',
      timezone: 'Europe/Paris',
      targetLanguage: 'French',
      checklist: [
        { id: 'demo-tutor-task-1', text: 'Attach corrected deck', done: false, updatedAt: minutesAgo(60) },
        { id: 'demo-tutor-task-2', text: 'Confirm next lesson time', done: true, updatedAt: minutesAgo(60) },
      ],
    },
    'demo-osaka-group': {
      priority: 'high',
      labels: ['Japan', 'Group'],
      notes: 'Group chat for arrival logistics. Keep messages practical and avoid long explanations.',
      timezone: 'Asia/Tokyo',
      targetLanguage: 'Japanese',
      reminderText: 'Share the venue pin and train exit',
      reminderAt: minutesFromNow(75),
      checklist: [
        { id: 'demo-osaka-task-1', text: 'Confirm venue pin', done: false, updatedAt: minutesAgo(90) },
        { id: 'demo-osaka-task-2', text: 'Share local train exit', done: false, updatedAt: minutesAgo(90) },
      ],
    },
    'demo-plumber': {
      priority: 'low',
      labels: ['Home'],
      notes: 'Snoozed until the quote is expected.',
      timezone: 'Europe/London',
      snoozedUntil: minutesFromNow(16 * 60),
    },
  };

  const drafts = {
    'demo-tutor': {
      text: 'Thanks Jules, I will send the corrected deck just after lunch and confirm the next lesson time.',
      updatedAt: minutesAgo(10),
    },
  };

  const quickReplies = [
    { id: `demo-reply-1-${now}`, text: 'Thanks, I will confirm shortly.', updatedAt: minutesAgo(15) },
    { id: `demo-reply-2-${now}`, text: 'Could you send the address again?', updatedAt: minutesAgo(16) },
    { id: `demo-reply-3-${now}`, text: 'I will send the details before we leave.', updatedAt: minutesAgo(17) },
  ];

  return {
    contacts,
    messagesByContact,
    metadataByContact,
    drafts,
    quickReplies,
    globalUsage: { inputTokens: 1840, outputTokens: 720, costUsd: 0.03 },
  };
}

export function suggestDemoReply({ message = {}, metadata = {}, contact = {} } = {}) {
  const text = messageSnippet(message).toLowerCase();
  const displayName = getContactDisplayName(contact, metadata);
  const targetLanguage = String(metadata?.targetLanguage || '').trim();
  const languageNote = targetLanguage ? ` I will keep it clear in ${targetLanguage}.` : '';

  if (text.includes('18:00') || text.includes('check') || text.includes('llave') || text.includes('key')) {
    return `Thanks ${displayName.split(' ')[0]}, 18:30 still works. I will send the taxi plate before we arrive.${languageNote}`;
  }
  if (text.includes('venue') || text.includes('exit') || text.includes('出口')) {
    return `I am confirming the venue pin now and will share the correct station exit with everyone shortly.${languageNote}`;
  }
  if (text.includes('quote') || text.includes('tomorrow')) {
    return `Thanks, tomorrow morning works. I will keep the chat snoozed until the quote is ready.`;
  }

  return `Thanks, I have this. I will reply with the next clear step shortly.${languageNote}`;
}

export function getSmartReplyOptions({ message = {}, metadata = {}, contact = {} } = {}) {
  if (!message) return [];

  const displayName = getContactDisplayName(contact, metadata).split(' ')[0] || 'there';
  const targetLanguage = String(metadata?.targetLanguage || metadata?.languageOverride || '').trim();
  const languageNote = targetLanguage ? ` I will keep it clear in ${targetLanguage}.` : '';
  const primary = suggestDemoReply({
    message,
    metadata: { ...metadata, targetLanguage },
    contact,
  });

  const text = messageSnippet(message).toLowerCase();
  const detailPrompt = text.includes('address') || text.includes('venue') || text.includes('exit')
    ? 'Could you send the exact location details again so I can confirm the next step?'
    : 'Could you send the key detail again so I can confirm properly?';
  const delayReply = text.includes('tomorrow') || text.includes('quote')
    ? `Thanks ${displayName}, tomorrow works. I will confirm once I have checked it.${languageNote}`
    : `Thanks ${displayName}, I am checking this now and will confirm shortly.${languageNote}`;

  return [
    { id: 'confirm', label: 'Confirm', text: primary },
    { id: 'ask-detail', label: 'Ask detail', text: detailPrompt },
    { id: 'buy-time', label: 'Buy time', text: delayReply },
  ];
}

export function getComposerProfilePresets(metadata = {}) {
  const currentLanguage = normalizeText(
    metadata?.targetLanguage
    || metadata?.languageOverride
    || metadata?.language
    || 'Spanish',
  );
  const currentTone = normalizeText(metadata?.translationStyle || '');

  return {
    languages: COMPOSER_LANGUAGE_PRESETS.map(label => ({
      label,
      value: label,
      active: normalizeText(label) === currentLanguage,
    })),
    tones: COMPOSER_TONE_PRESETS.map(label => ({
      label,
      value: label.toLowerCase(),
      active: normalizeText(label) === currentTone,
    })),
  };
}

export function getComposerReminderPresets(now = Date.now()) {
  return COMPOSER_REMINDER_PRESETS.map((preset) => {
    const reminderAt = new Date(now);
    if (preset.days) {
      reminderAt.setDate(reminderAt.getDate() + preset.days);
      reminderAt.setHours(preset.hour, 0, 0, 0);
    } else {
      reminderAt.setHours(reminderAt.getHours() + preset.hours);
      reminderAt.setMinutes(0, 0, 0);
    }

    return {
      id: preset.id,
      label: preset.label,
      reminderAt: reminderAt.getTime(),
    };
  });
}

function getLatestDirectionalMessage(messages = [], isFromMe) {
  for (let index = (messages || []).length - 1; index >= 0; index -= 1) {
    const message = messages[index];
    if (Boolean(message?.isFromMe || message?.is_from_me) === isFromMe) {
      return message;
    }
  }
  return null;
}

function textContainsQuestion(text) {
  const normalized = normalizeText(text);
  return /\?/.test(String(text || ''))
    || /\b(can|could|would|will|when|where|what|which|who|how|is|are|do|does|did|peux|puedes|puede|何|どこ|いつ)\b/i.test(normalized);
}

function textMentionsAny(text, values = []) {
  const normalized = normalizeText(text);
  return values.some(value => {
    const token = normalizeText(value);
    return token && normalized.includes(token);
  });
}

function getOpenChecklistItems(metadata = {}) {
  return getChecklistItems(metadata).filter(item => item && !item.done);
}

export function buildConversationBrief({
  contact = {},
  metadata = {},
  messages = [],
  drafts = {},
  now = Date.now(),
} = {}) {
  const latestIncoming = getLatestDirectionalMessage(messages, false);
  const latestOutgoing = getLatestDirectionalMessage(messages, true);
  const snapshot = getContactActionSnapshot({
    contact,
    drafts,
    metadata,
    messages,
    now,
  });
  const openTasks = getOpenChecklistItems(metadata);
  const latestIncomingSnippet = latestIncoming ? messageSnippet(latestIncoming, 92) : '';
  const latestOutgoingSnippet = latestOutgoing ? messageSnippet(latestOutgoing, 92) : '';
  const hasQuestion = textContainsQuestion(latestIncomingSnippet);

  let nextAction = 'No immediate action suggested.';
  if (snapshot.reminderStatus === 'due') {
    nextAction = metadata.reminderText || 'Follow up on the due reminder.';
  } else if (snapshot.replyState === 'drafting') {
    nextAction = 'Review and send the saved draft.';
  } else if (hasQuestion) {
    nextAction = 'Answer the latest question directly.';
  } else if (openTasks.length > 0) {
    nextAction = `Handle: ${openTasks[0].text}`;
  } else if (snapshot.replyState === 'needs-reply') {
    nextAction = 'Send a short acknowledgement or next step.';
  }

  const contextLines = [
    latestIncomingSnippet ? `Incoming: ${latestIncomingSnippet}` : '',
    latestOutgoingSnippet ? `Last sent: ${latestOutgoingSnippet}` : '',
    snapshot.timezoneInfo ? `${snapshot.timezoneInfo.label} · ${snapshot.timezoneInfo.statusLabel}` : '',
  ].filter(Boolean);

  return {
    latestIncomingSnippet,
    latestOutgoingSnippet,
    hasQuestion,
    openTasks: openTasks.map(item => ({ ...item })),
    nextAction,
    contextLines,
    summary: contextLines[0] || snapshot.summary || 'Add messages or local context to build a brief.',
  };
}

export function buildConversationActionPlan({
  contact = {},
  metadata = {},
  messages = [],
  drafts = {},
  now = Date.now(),
  maxItems = 4,
} = {}) {
  const contactId = contact?.id || '';
  const brief = buildConversationBrief({ contact, metadata, messages, drafts, now });
  const snapshot = getContactActionSnapshot({ contact, drafts, metadata, messages, now });
  const openTasks = getOpenChecklistItems(metadata);
  const untranslated = getUntranslatedIncomingMessages(messages);
  const draftText = getDraftText(drafts, contactId);
  const actions = [];
  const addAction = (action) => {
    if (!action?.id || actions.some(entry => entry.id === action.id)) return;
    actions.push(action);
  };

  if (snapshot.reminderStatus === 'due') {
    addAction({
      id: 'due-reminder',
      label: 'Handle due reminder',
      detail: String(metadata?.reminderText || 'Follow up now.').trim(),
      priority: 'high',
    });
  }

  if (draftText) {
    addAction({
      id: 'review-draft',
      label: 'Review saved draft',
      detail: messageSnippet({ content: { body: draftText } }, 110),
      priority: 'high',
    });
  }

  if (brief.hasQuestion) {
    addAction({
      id: 'answer-question',
      label: 'Answer latest question',
      detail: brief.latestIncomingSnippet,
      priority: 'high',
    });
  }

  if (untranslated.length > 0) {
    addAction({
      id: 'translate-incoming',
      label: `Translate ${untranslated.length} incoming message${untranslated.length === 1 ? '' : 's'}`,
      detail: messageSnippet(untranslated[untranslated.length - 1], 110),
      priority: 'normal',
    });
  }

  openTasks.slice(0, 3).forEach((task, index) => {
    addAction({
      id: `task-${task.id || index}`,
      label: index === 0 ? 'Work next open task' : 'Keep task visible',
      detail: task.text,
      priority: index === 0 ? 'normal' : 'low',
    });
  });

  if (snapshot.timezoneInfo?.status === 'quiet-hours') {
    addAction({
      id: 'quiet-hours',
      label: 'Consider waiting',
      detail: `${snapshot.timezoneInfo.localTime} is quiet hours for this contact.`,
      priority: 'normal',
    });
  }

  if (actions.length === 0) {
    actions.push({
      id: 'no-action',
      label: 'No immediate action',
      detail: brief.summary,
      priority: 'low',
    });
  }

  return {
    nextAction: brief.nextAction,
    actions: actions.slice(0, Math.max(1, maxItems)),
    untranslatedCount: untranslated.length,
  };
}

export function buildConversationHandoffBrief({
  contact = {},
  metadata = {},
  messages = [],
  drafts = {},
  now = Date.now(),
  maxMessages = 4,
} = {}) {
  const contactId = contact?.id || '';
  const displayName = getContactDisplayName(contact, metadata);
  const brief = buildConversationBrief({ contact, metadata, messages, drafts, now });
  const snapshot = getContactActionSnapshot({ contact, drafts, metadata, messages, now });
  const labels = getContactLabels(metadata);
  const notes = String(metadata?.notes || '').trim();
  const draftText = getDraftText(drafts, contactId);
  const reminder = String(metadata?.reminderText || '').trim();
  const openTasks = getOpenChecklistItems(metadata);
  const recentMessages = (Array.isArray(messages) ? messages : [])
    .slice(-Math.max(1, maxMessages))
    .map((message) => {
      const sender = message?.isFromMe || message?.is_from_me
        ? 'Me'
        : (message?.senderName || message?.sender_name || displayName);
      return `- ${sender}: ${messageSnippet(message, 140)}`;
    });

  const sections = [
    `Conversation: ${displayName}`,
    `Next action: ${brief.nextAction}`,
    snapshot.summary ? `Status: ${snapshot.summary}` : '',
    labels.length ? `Labels: ${labels.join(', ')}` : '',
    snapshot.timezoneInfo ? `Local time: ${snapshot.timezoneInfo.localTime} (${snapshot.timezoneInfo.statusLabel})` : '',
    reminder ? `Reminder: ${reminder}` : '',
    openTasks.length ? `Open tasks:\n${openTasks.map(task => `- ${task.text}`).join('\n')}` : '',
    draftText ? `Saved draft:\n${draftText}` : '',
    notes ? `Private notes:\n${notes}` : '',
    recentMessages.length ? `Recent messages:\n${recentMessages.join('\n')}` : '',
  ].filter(Boolean);

  return sections.join('\n\n');
}

export function getContextualReminderPresets({
  draftText = '',
  latestIncomingMessage = null,
  metadata = {},
  now = Date.now(),
} = {}) {
  const presets = getComposerReminderPresets(now);
  const sourceText = `${draftText || ''} ${latestIncomingMessage ? messageSnippet(latestIncomingMessage, 120) : ''}`;
  const normalized = normalizeText(sourceText);
  const reminderText = String(metadata?.reminderText || '').trim();

  if (/\btomorrow\b|mañana|demain|明日/.test(normalized)) {
    const tomorrow = new Date(now);
    tomorrow.setDate(tomorrow.getDate() + 1);
    tomorrow.setHours(9, 0, 0, 0);
    presets.unshift({
      id: 'context-tomorrow',
      label: 'Tomorrow from chat',
      reminderAt: tomorrow.getTime(),
      reason: 'Detected tomorrow in the conversation',
    });
  } else if (/\bquote\b|\bprice\b|\bconfirm\b|\bcheck\b|\bfollow up\b/.test(normalized) || reminderText) {
    const soon = new Date(now + 2 * 60 * 60 * 1000);
    soon.setMinutes(0, 0, 0);
    presets.unshift({
      id: 'context-follow-up',
      label: 'Follow-up soon',
      reminderAt: soon.getTime(),
      reason: reminderText || 'Detected a follow-up cue',
    });
  }

  return presets.filter((preset, index, list) => (
    list.findIndex(candidate => candidate.id === preset.id) === index
  ));
}

export function getComposerSendReadiness({
  draftText = '',
  metadata = {},
  latestIncomingMessage = null,
  now = Date.now(),
} = {}) {
  const trimmedDraft = String(draftText || '').trim();
  const latestIncomingSnippet = latestIncomingMessage ? messageSnippet(latestIncomingMessage, 160) : '';
  const openTasks = getOpenChecklistItems(metadata);
  const timezoneInfo = getTimezoneInfo(metadata, now);
  const checks = [];

  if (!trimmedDraft) {
    checks.push({ id: 'draft', status: 'info', label: 'Type a draft to check reply coverage.' });
  } else {
    checks.push({ id: 'draft', status: 'ready', label: 'Draft is ready to translate.' });
  }

  if (latestIncomingSnippet && textContainsQuestion(latestIncomingSnippet)) {
    checks.push({
      id: 'question',
      status: textContainsQuestion(trimmedDraft) || trimmedDraft.length > 12 ? 'ready' : 'warning',
      label: textContainsQuestion(trimmedDraft)
        ? 'Draft asks a clarifying question.'
        : 'Latest incoming looks like a question; answer it directly.',
    });
  }

  if (openTasks.length > 0) {
    checks.push({
      id: 'tasks',
      status: textMentionsAny(trimmedDraft, openTasks.map(item => item.text)) ? 'ready' : 'warning',
      label: textMentionsAny(trimmedDraft, openTasks.map(item => item.text))
        ? 'Draft references an open task.'
        : `Open task still visible: ${openTasks[0].text}`,
    });
  }

  if (timezoneInfo?.status === 'quiet-hours') {
    checks.push({
      id: 'timezone',
      status: 'warning',
      label: `${timezoneInfo.localTime} there. Consider scheduling or waiting.`,
    });
  } else if (timezoneInfo) {
    checks.push({
      id: 'timezone',
      status: 'ready',
      label: `${timezoneInfo.localTime} there. Timing looks reasonable.`,
    });
  }

  const warnings = checks.filter(check => check.status === 'warning').length;
  return {
    checks,
    warnings,
    scoreLabel: warnings === 0 ? 'Ready' : `${warnings} check${warnings === 1 ? '' : 's'}`,
  };
}

export function getComposerAssistState({
  draftText = '',
  metadata = {},
  contact = {},
  latestIncomingMessage = null,
  messages = [],
  demoMode = false,
  now = Date.now(),
} = {}) {
  const targetLanguage = String(
    metadata?.targetLanguage
    || metadata?.languageOverride
    || metadata?.language
    || 'Spanish',
  ).trim() || 'Spanish';
  const translationStyle = String(metadata?.translationStyle || '').trim();
  const trimmedDraft = String(draftText || '').trim();
  const latestIncomingSnippet = latestIncomingMessage ? messageSnippet(latestIncomingMessage, 96) : '';
  const suggestedReply = latestIncomingMessage
    ? suggestDemoReply({ message: latestIncomingMessage, metadata: { ...metadata, targetLanguage }, contact })
    : '';
  const smartReplies = latestIncomingMessage
    ? getSmartReplyOptions({ message: latestIncomingMessage, metadata: { ...metadata, targetLanguage }, contact })
    : [];
  const translatedPreview = trimmedDraft
    ? simulateTranslation(trimmedDraft, targetLanguage)
    : '';
  const styleSummary = translationStyle ? `${translationStyle} tone` : 'standard tone';
  const profilePresets = getComposerProfilePresets({ ...metadata, targetLanguage, translationStyle });
  const readiness = getComposerSendReadiness({
    draftText: trimmedDraft,
    metadata,
    latestIncomingMessage,
    now,
  });
  const conversationBrief = buildConversationBrief({
    contact,
    metadata,
    messages,
    drafts: contact?.id ? { [contact.id]: { text: trimmedDraft, updatedAt: now } } : {},
    now,
  });

  return {
    targetLanguage,
    translationStyle,
    styleSummary,
    latestIncomingSnippet,
    suggestedReply,
    smartReplies,
    profilePresets,
    reminderPresets: getContextualReminderPresets({
      draftText: trimmedDraft,
      latestIncomingMessage,
      metadata,
      now,
    }),
    readiness,
    conversationBrief,
    translatedPreview,
    hasDraft: Boolean(trimmedDraft),
    hasIncomingContext: Boolean(latestIncomingMessage),
    canUseSuggestedReply: smartReplies.length > 0 || Boolean(suggestedReply),
    showPreview: Boolean(trimmedDraft || latestIncomingMessage || targetLanguage || translationStyle || demoMode),
    previewLabel: demoMode ? 'Demo translation preview' : 'Translation route',
  };
}

export function simulateTranslation(text, targetLanguage = 'Spanish') {
  const trimmed = String(text || '').trim();
  if (!trimmed) return '';

  const language = String(targetLanguage || 'Spanish').trim() || 'Spanish';
  const normalizedLanguage = language.toLowerCase();
  const lowerText = trimmed.toLowerCase();
  const phraseMap = {
    spanish: [
      [/18:30|taxi|plate|station/, '18:30 nos va bien. Te enviare la matricula del taxi antes de llegar.'],
      [/thank|thanks|confirm|shortly/, 'Gracias, lo confirmare en breve.'],
      [/address|send.*again/, 'Puedes enviarme la direccion otra vez?'],
    ],
    french: [
      [/deck|lesson|lunch|corrected/, 'Merci, j enverrai la version corrigee apres le dejeuner et je confirmerai le prochain cours.'],
      [/thank|thanks|confirm|shortly/, 'Merci, je confirmerai cela bientot.'],
    ],
    japanese: [
      [/venue|pin|station|exit/, '会場のピンと駅の出口をすぐに共有します。'],
      [/thank|thanks|confirm|shortly/, 'ありがとうございます。すぐに確認します。'],
    ],
  };

  const matches = phraseMap[normalizedLanguage] || phraseMap.spanish;
  const match = matches.find(([pattern]) => pattern.test(lowerText));
  if (match) return match[1];

  return `[${language} demo translation] ${trimmed}`;
}

export function countMatchingMessages(messages, query, options = {}) {
  return filterMessagesByQuery(messages, query, options).length;
}
