const DOT_ICON = `
  <svg viewBox="0 0 24 24" width="16" height="16" aria-hidden="true">
    <circle cx="12" cy="12" r="6" fill="currentColor"></circle>
  </svg>
`;

const PIN_ICON = `
  <svg viewBox="0 0 24 24" width="14" height="14" aria-hidden="true">
    <path fill="currentColor" d="M16 12V4h1V2H7v2h1v8l-2 2v2h5.2v6h1.6v-6H18v-2l-2-2z"></path>
  </svg>
`;

const SEARCH_ICON = `
  <svg viewBox="0 0 24 24" width="18" height="18" aria-hidden="true">
    <path fill="currentColor" d="M15.5 14h-.79l-.28-.27A6.47 6.47 0 0 0 16 9.5 6.5 6.5 0 1 0 9.5 16a6.47 6.47 0 0 0 4.23-1.57l.27.28v.79L20 21.5 21.5 20l-6-6zm-6 0A4.5 4.5 0 1 1 14 9.5 4.5 4.5 0 0 1 9.5 14z"></path>
  </svg>
`;

const APPEARANCE_ICON = `
  <svg viewBox="0 0 24 24" width="18" height="18" aria-hidden="true">
    <path fill="currentColor" d="M12 3a9 9 0 1 0 9 9c0-.34-.02-.68-.06-1.01a1 1 0 0 0-1.34-.8A6.5 6.5 0 1 1 13.81 4.4 1 1 0 0 0 13 3.06 9.26 9.26 0 0 0 12 3z"></path>
  </svg>
`;

const LOGOUT_ICON = `
  <svg viewBox="0 0 24 24" width="18" height="18" aria-hidden="true">
    <path fill="currentColor" d="M16 17v-3H9v-4h7V7l5 5-5 5M14 2a2 2 0 0 1 2 2v2h-2V4H5v16h9v-2h2v2a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9z"></path>
  </svg>
`;

const SEND_ICON = `
  <svg viewBox="0 0 24 24" width="20" height="20" aria-hidden="true">
    <path fill="currentColor" d="M1.101 21.757L23.8 12.028 1.101 2.3l.011 7.912 13.623 1.816-13.623 1.817-.011 7.912z"></path>
  </svg>
`;

const CLOSE_ICON = `
  <svg viewBox="0 0 24 24" width="20" height="20" aria-hidden="true">
    <path fill="currentColor" d="M19 6.41L17.59 5 12 10.59 6.41 5 5 6.41 10.59 12 5 17.59 6.41 19 12 13.41 17.59 19 19 17.59 13.41 12z"></path>
  </svg>
`;

const EMOJI_ICON = `
  <svg viewBox="0 0 24 24" width="24" height="24" aria-hidden="true">
    <path fill="currentColor" d="M12 2C6.48 2 2 6.48 2 12s4.48 10 10 10 10-4.48 10-10S17.52 2 12 2zm0 18c-4.41 0-8-3.59-8-8s3.59-8 8-8 8 3.59 8 8-3.59 8-8 8zm-5-6c.78 2.34 2.72 4 5 4s4.22-1.66 5-4H7zm2-3c.55 0 1-.45 1-1s-.45-1-1-1-1 .45-1 1 .45 1 1 1zm6 0c.55 0 1-.45 1-1s-.45-1-1-1-1 .45-1 1 .45 1 1 1z"></path>
  </svg>
`;

const ATTACH_ICON = `
  <svg viewBox="0 0 24 24" width="24" height="24" aria-hidden="true">
    <path fill="currentColor" d="M16.5 6.5v8.25a4.25 4.25 0 1 1-8.5 0V5.75a2.75 2.75 0 1 1 5.5 0v8.5a1.25 1.25 0 1 1-2.5 0V7h-1.5v7.25a2.75 2.75 0 0 0 5.5 0v-8.5a4.25 4.25 0 1 0-8.5 0v9a5.75 5.75 0 1 0 11.5 0V6.5z"></path>
  </svg>
`;

const WORKSPACE_ICON = `
  <svg viewBox="0 0 24 24" width="20" height="20" aria-hidden="true">
    <path fill="currentColor" d="M3 5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V5zm6 0H5v14h4V5zm2 14h8V5h-8v14z"></path>
  </svg>
`;

const MENU_ICON = `
  <svg viewBox="0 0 24 24" width="20" height="20" aria-hidden="true">
    <path fill="currentColor" d="M12 7a2 2 0 1 0 0-4 2 2 0 0 0 0 4zm0 7a2 2 0 1 0 0-4 2 2 0 0 0 0 4zm0 7a2 2 0 1 0 0-4 2 2 0 0 0 0 4z"></path>
  </svg>
`;

const TRANSLATE_ICON = `
  <svg viewBox="0 0 24 24" width="16" height="16" aria-hidden="true">
    <path fill="currentColor" d="M12.87 15.07l-2.54-2.51.03-.03c1.74-1.94 2.98-4.17 3.71-6.53H17V4h-7V2H8v2H1v1.99h11.17C11.5 7.92 10.44 9.75 9 11.35 8.07 10.32 7.3 9.19 6.69 8h-2c.73 1.63 1.73 3.17 2.98 4.56l-5.09 5.02L4 19l5-5 3.11 3.11.76-2.04zM18.5 10h-2L12 22h2l1.12-3h4.75L21 22h2l-4.5-12zm-2.62 7l1.62-4.33L19.12 17h-3.24z"></path>
  </svg>
`;

const REPLY_ICON = `
  <svg viewBox="0 0 24 24" width="16" height="16" aria-hidden="true">
    <path fill="currentColor" d="M10 9V5l-7 7 7 7v-4.1c5 0 8.5 1.6 11 5.1-1-5-4-10-11-11z"></path>
  </svg>
`;

const AI_ICON = `
  <svg viewBox="0 0 24 24" width="18" height="18" aria-hidden="true">
    <path fill="currentColor" d="M21 11.5a8.38 8.38 0 0 1-.9 3.8 8.5 8.5 0 0 1-7.6 4.7 8.38 8.38 0 0 1-3.8-.9L3 21l1.9-5.7a8.38 8.38 0 0 1-.9-3.8 8.5 8.5 0 0 1 4.7-7.6 8.38 8.38 0 0 1 3.8-.9h.5a8.48 8.48 0 0 1 8 8v.5z"></path>
  </svg>
`;

const STAR_ICON = `
  <svg viewBox="0 0 24 24" width="16" height="16" aria-hidden="true">
    <path fill="currentColor" d="m12 17.27 6.18 3.73-1.64-7.03L22 9.24l-7.19-.61L12 2 9.19 8.63 2 9.24l5.46 4.73L5.82 21z"></path>
  </svg>
`;

const REACTION_ICON = `
  <svg viewBox="0 0 24 24" width="16" height="16" aria-hidden="true">
    <path fill="currentColor" d="M12 2C6.48 2 2 6.48 2 12s4.48 10 10 10 10-4.48 10-10S17.52 2 12 2zm0 18c-4.41 0-8-3.59-8-8s3.59-8 8-8 8 3.59 8 8-3.59 8-8 8zm-5-6c.78 2.34 2.72 4 5 4s4.22-1.66 5-4H7zm2-3c.55 0 1-.45 1-1s-.45-1-1-1-1 .45-1 1 .45 1 1 1zm6 0c.55 0 1-.45 1-1s-.45-1-1-1-1 .45-1 1 .45 1 1 1z"></path>
  </svg>
`;

const CHECK_ICON = `
  <svg viewBox="0 0 24 24" width="20" height="20" aria-hidden="true">
    <path fill="currentColor" d="M9.55 18.55 3.5 12.5l1.4-1.4 4.65 4.65 9.55-9.55 1.4 1.4z"></path>
  </svg>
`;

const CHAT_GLYPH = `
  <svg viewBox="0 0 24 24" width="80" height="80" aria-hidden="true">
    <path fill="currentColor" d="M19.005 3.175H4.674C3.642 3.175 3 3.789 3 4.821V21.02l3.544-3.514h12.461c1.033 0 2.064-1.06 2.064-2.093V4.821c-.001-1.032-1.032-1.646-2.064-1.646zm-4.989 9.869H7.041V11.1h6.975v1.944zm3-4H7.041V7.1h9.975v1.944z"></path>
  </svg>
`;

const SAMPLE_CONTACTS = [
  {
    id: 'sofia',
    name: 'Sofia at Casa Azul',
    initial: 'S',
    preview: 'Check-in is ready for 18:30. Please send the taxi plate when you have it.',
    time: '09:18',
    unread: 2,
    pinned: true,
    badges: [
      { type: 'priority urgent', label: 'Urgent' },
      { type: 'reply', label: 'Reply' },
      { type: 'label', label: 'Travel' },
    ],
  },
  {
    id: 'jules',
    name: 'Jules (French tutor)',
    initial: 'J',
    preview: 'Draft saved: send the corrected deck after lunch.',
    previewClass: 'preview-text checklist-preview',
    time: '08:42',
    active: true,
    badges: [
      { type: 'drafting', label: 'Drafting' },
      { type: 'tasks', label: '2 tasks' },
      { type: '', label: 'Note' },
    ],
  },
  {
    id: 'osaka-group',
    name: 'Osaka arrival group',
    initial: 'O',
    preview: 'Waiting on the venue confirmation from the host.',
    time: 'Yesterday',
    group: true,
    badges: [
      { type: 'waiting', label: 'Waiting' },
      { type: 'reminder', label: 'Reminder' },
    ],
  },
  {
    id: 'theo',
    name: 'Theo the plumber',
    initial: 'T',
    preview: 'Snoozed until Thursday 11:00.',
    previewClass: 'preview-text snooze-preview',
    time: 'Tue',
    due: true,
    badges: [
      { type: 'snoozed', label: 'Snoozed' },
      { type: 'priority high', label: 'High' },
    ],
  },
];

const SAMPLE_FOCUS_ITEMS = [
  {
    name: 'Sofia at Casa Azul',
    summary: 'Awaiting your reply about check-in timing and taxi details.',
    time: '09:18',
  },
  {
    name: 'Jules (French tutor)',
    summary: 'Two open tasks and a draft follow-up already in progress.',
    time: '08:42',
  },
];

const SAMPLE_REMINDERS = [
  {
    name: 'Osaka arrival group',
    summary: 'Confirm the venue pin and share the local train exit.',
    time: 'Today, 16:00',
  },
  {
    name: 'Theo the plumber',
    summary: 'Re-open the chat once the boiler quote lands.',
    time: 'Thu, 11:00',
  },
];

const SAMPLE_MESSAGES = [
  {
    sender: 'Sofia',
    body: 'The keys are in the lockbox by the blue gate.',
    time: '09:21',
    translated: true,
    translationText: 'Les clefs sont dans la boite a code pres du portail bleu.',
    translationLanguage: 'French',
    reactions: [{ emoji: '🙏', count: 1 }],
  },
  {
    outgoing: true,
    quoted: {
      sender: 'Sofia',
      text: 'Can I arrive after 18:00?',
    },
    body: '18:30 works on our side. Please send the taxi plate when you have it.',
    time: '09:24',
    starred: true,
  },
  {
    body: 'Perfect, I will send it as soon as we leave the station.',
    time: '09:26',
    translated: true,
    translationText: 'Parfait, je vous l envoie des que nous quittons la gare.',
    translationLanguage: 'French',
    reactions: [
      { emoji: '👍', count: 1 },
      { emoji: '✅', count: 1 },
    ],
  },
];

export function escapeHtml(value) {
  return String(value ?? '').replace(/[&<>"']/g, (char) => {
    const entities = {
      '&': '&amp;',
      '<': '&lt;',
      '>': '&gt;',
      '"': '&quot;',
      "'": '&#39;',
    };

    return entities[char];
  });
}

function joinClasses(...values) {
  return values.filter(Boolean).join(' ');
}

function renderIconButton(className, label, icon = DOT_ICON) {
  return `
    <button class="${className}" type="button" title="${escapeHtml(label)}" aria-label="${escapeHtml(label)}">
      ${icon}
    </button>
  `;
}

function renderBadge(badge) {
  const badgeType = badge.type ? ` ${badge.type}` : '';
  return `<span class="contact-meta-chip${badgeType}">${escapeHtml(badge.label)}</span>`;
}

function renderStatCard(stat) {
  return `
    <button class="visitor-stat-card ${stat.active ? 'active' : ''}" type="button">
      <span class="visitor-stat-value">${escapeHtml(stat.value)}</span>
      <span class="visitor-stat-label">${escapeHtml(stat.label)}</span>
      <span class="visitor-stat-description">${escapeHtml(stat.description)}</span>
    </button>
  `;
}

function renderDashboardListItem(item) {
  return `
    <button class="visitor-focus-item" type="button">
      <div class="visitor-focus-copy">
        <span class="visitor-focus-name">${escapeHtml(item.name)}</span>
        <span class="visitor-focus-summary">${escapeHtml(item.summary)}</span>
      </div>
      <div class="visitor-focus-meta">
        <span class="visitor-focus-time">${escapeHtml(item.time)}</span>
      </div>
    </button>
  `;
}

function renderActionButton(className, label, icon = DOT_ICON) {
  return `
    <button class="${className}" type="button" title="${escapeHtml(label)}" aria-label="${escapeHtml(label)}">
      ${icon}
    </button>
  `;
}

function renderTranslationIndicator({ language, text, open = false, header = 'Original message' }) {
  return `
    <span class="translation-indicator ${open ? 'show-tooltip' : ''}">
      <span class="info-icon">i</span>
      <span>Translated</span>
      <div class="original-tooltip">
        <div class="tooltip-header">${escapeHtml(header)} (${escapeHtml(language)})</div>
        <div class="tooltip-text">${escapeHtml(text)}</div>
      </div>
    </span>
  `;
}

function renderMessageReactions(reactions = []) {
  if (!reactions.length) return '';

  return `
    <div class="message-reactions">
      ${reactions.map((reaction) => `
        <span class="reaction-item">
          ${escapeHtml(reaction.emoji)}
          <span class="reaction-count">${escapeHtml(reaction.count)}</span>
        </span>
      `).join('')}
    </div>
  `;
}

function renderMessage(message) {
  const messageClasses = joinClasses('message', message.outgoing ? 'outgoing' : 'incoming');
  const sender = !message.outgoing && message.sender
    ? `<div class="message-sender">${escapeHtml(message.sender)}</div>`
    : '';
  const quoted = message.quoted
    ? `
      <div class="quoted-message">
        <div class="quoted-sender">${escapeHtml(message.quoted.sender)}</div>
        <div class="quoted-text">${escapeHtml(message.quoted.text)}</div>
      </div>
    `
    : '';
  const translationIndicator = message.translated
    ? renderTranslationIndicator({
        language: message.translationLanguage || 'French',
        text: message.translationText || '',
        open: Boolean(message.translationOpen),
        header: message.outgoing ? 'Sent as' : 'Original message',
      })
    : '';
  const star = message.starred ? '<span class="message-starred-badge" title="Starred">★</span>' : '';
  const translateButtonClasses = joinClasses(
    'message-action-btn',
    'active',
    'translate-button',
    !message.outgoing && 'can-translate',
  );

  return `
    <div class="${messageClasses}">
      ${sender}
      ${quoted}
      <div class="message-text">${escapeHtml(message.body)}</div>
      ${renderMessageReactions(message.reactions)}
      <div class="message-footer">
        <span class="message-time">${escapeHtml(message.time)}</span>
        ${star}
        ${translationIndicator}
        <div class="message-actions">
          ${renderActionButton(translateButtonClasses, 'Translate', TRANSLATE_ICON)}
          ${renderActionButton('message-action-btn active', 'Reply', REPLY_ICON)}
          ${renderActionButton('message-action-btn active ai-reply-btn', 'Draft with AI', AI_ICON)}
          ${renderActionButton('message-action-btn active star-toggle', 'Star', STAR_ICON)}
          <div class="reaction-button-container">
            ${renderActionButton('message-action-btn active', 'React', REACTION_ICON)}
          </div>
        </div>
      </div>
    </div>
  `;
}

export function renderAppFrame(content, { page = false } = {}) {
  const appClass = page ? 'sb-app--page' : 'sb-app--component';

  return `
    <div id="app" class="${appClass}">
      ${content}
    </div>
  `;
}

export function renderComponentPreview(content, variant = 'chat') {
  return renderAppFrame(`
    <div class="sb-component-card sb-component-card--${escapeHtml(variant)}">
      ${content}
    </div>
  `);
}

export function renderAuthOverlay({ showError = false } = {}) {
  return renderAppFrame(`
    <div class="overlay">
      <div class="overlay-content overlay-card auth-card">
        <div class="overlay-badge" aria-hidden="true">WT</div>
        <span class="overlay-kicker">Protected workspace</span>
        <h2>Unlock WhatsApp Translator</h2>
        <p>Sign in to open your inbox, follow-ups, and translation tools.</p>
        <form class="password-form">
          <input type="password" id="password-input" placeholder="Enter password" autocomplete="current-password">
          <button class="password-button" type="submit">Continue</button>
        </form>
        <p class="password-error ${showError ? '' : 'hidden'}">Incorrect password. Try again.</p>
      </div>
    </div>
  `);
}

export function renderContactItem(contact, { activeId = null } = {}) {
  const isActive = contact.active || contact.id === activeId;
  const classes = joinClasses(
    'contact-item',
    isActive && 'active',
    contact.group && 'is-group',
    contact.pinned && 'is-pinned',
    contact.important && 'is-important',
    contact.due && 'has-due-reminder',
  );
  const previewClass = contact.previewClass || 'preview-text';

  return `
    <div class="${classes}" data-contact-id="${escapeHtml(contact.id)}">
      <div class="avatar-container">
        <div class="avatar">
          <span>${escapeHtml(contact.initial || contact.name.charAt(0))}</span>
          ${contact.group ? '<div class="group-indicator"></div>' : ''}
        </div>
        <button class="pin-button ${contact.pinned ? 'pinned' : ''}" type="button" title="${contact.pinned ? 'Unpin' : 'Pin'}" aria-label="${contact.pinned ? 'Unpin' : 'Pin'}">
          ${PIN_ICON}
        </button>
      </div>
      <div class="contact-details">
        <div class="contact-header">
          <div class="contact-title">
            <span class="contact-name">${escapeHtml(contact.name)}</span>
            ${(contact.badges || []).map(renderBadge).join('')}
          </div>
          <span class="contact-time">${escapeHtml(contact.time)}</span>
        </div>
        <div class="contact-preview">
          <span class="${previewClass}">${escapeHtml(contact.preview)}</span>
          ${contact.unread ? `<span class="unread-badge">${escapeHtml(contact.unread)}</span>` : ''}
        </div>
      </div>
    </div>
  `;
}

export function renderContactsPanel({ activeId = 'jules', contacts = SAMPLE_CONTACTS } = {}) {
  return `
    <aside id="contacts-panel">
      <header class="panel-header">
        <div class="user-info">
          <div class="avatar">
            <span>W</span>
          </div>
          <div class="user-details">
            <span id="user-name">Workspace owner</span>
            <span id="user-phone" class="phone">+44 20 5555 0199</span>
          </div>
        </div>
        <div class="header-actions">
          <div class="status-indicator">
            <span class="status-dot connected"></span>
            <span>Connected</span>
          </div>
          ${renderIconButton('header-icon-button', 'Command palette', SEARCH_ICON)}
          ${renderIconButton('header-icon-button', 'Appearance', APPEARANCE_ICON)}
          ${renderIconButton('logout-button', 'Logout', LOGOUT_ICON)}
        </div>
      </header>

      <section class="inbox-toolbar">
        <label class="inbox-search" for="storybook-inbox-search">
          ${SEARCH_ICON}
          <input id="storybook-inbox-search" type="search" value="" placeholder="Search chats, aliases, drafts, or notes">
        </label>
        <div class="inbox-filter-row">
          <button class="inbox-filter-chip active" type="button">Needs reply</button>
          <button class="inbox-filter-chip" type="button">Drafts</button>
          <button class="inbox-filter-chip" type="button">Tasks</button>
          <button class="inbox-filter-chip" type="button">Pinned</button>
          <button class="inbox-filter-clear" type="button">Clear</button>
        </div>
      </section>

      <div class="contacts-list">
        ${contacts.map((contact) => renderContactItem(contact, { activeId })).join('')}
      </div>

      <footer class="sidebar-footer">
        <div class="usage-info">
          <span class="usage-label">Translation Cost</span>
          <span class="usage-cost">$14.82</span>
        </div>
      </footer>
    </aside>
  `;
}

export function renderVisitorDashboard() {
  const stats = [
    { value: 4, label: 'Needs reply', description: 'Chats waiting on you', active: true },
    { value: 2, label: 'Due reminders', description: 'Follow-ups due now' },
    { value: 3, label: 'Saved drafts', description: 'Replies already started' },
    { value: 5, label: 'Open tasks', description: 'Checklist items still open' },
  ];

  return `
    <section id="visitor-dashboard" class="visitor-dashboard">
      <div class="visitor-dashboard-hero">
        <div>
          <span class="visitor-dashboard-kicker">Today</span>
          <h3>Start with the conversations that matter most</h3>
          <p>Jump straight into replies, reminders, drafts, and open tasks without digging through the inbox.</p>
        </div>
        <button class="visitor-dashboard-command-button" type="button">Open command palette</button>
      </div>

      <div class="visitor-dashboard-stats">
        ${stats.map(renderStatCard).join('')}
      </div>

      <div class="visitor-dashboard-grid">
        <section class="visitor-dashboard-panel">
          <div class="visitor-dashboard-panel-header">
            <h3>Focus queue</h3>
            <span class="visitor-dashboard-count">${SAMPLE_FOCUS_ITEMS.length} active</span>
          </div>
          <div class="visitor-dashboard-list">
            ${SAMPLE_FOCUS_ITEMS.map(renderDashboardListItem).join('')}
          </div>
        </section>

        <section class="visitor-dashboard-panel">
          <div class="visitor-dashboard-panel-header">
            <h3>Upcoming reminders</h3>
            <span class="visitor-dashboard-count">${SAMPLE_REMINDERS.length} scheduled</span>
          </div>
          <div class="visitor-dashboard-list">
            ${SAMPLE_REMINDERS.map(renderDashboardListItem).join('')}
          </div>
        </section>
      </div>
    </section>
  `;
}

export function renderEmptyChatState() {
  return `
    <div class="empty-chat">
      <div class="empty-chat-content">
        <div class="empty-chat-intro">
          <div class="empty-icon">
            ${CHAT_GLYPH}
          </div>
          <h2>WhatsApp Translator</h2>
          <p>Select a conversation to view messages</p>
        </div>
        ${renderVisitorDashboard()}
      </div>
    </div>
  `;
}

export function renderMessagesList(messages = SAMPLE_MESSAGES) {
  return `
    <div id="messages-list" class="messages-list">
      ${messages.map(renderMessage).join('')}
    </div>
  `;
}

export function renderComposer({ showReply = true, showDraft = true } = {}) {
  return `
    <footer class="message-input-area">
      <div class="reply-preview ${showReply ? '' : 'hidden'}">
        <div class="reply-preview-content">
          <span class="reply-preview-sender">Sofia at Casa Azul</span>
          <span class="reply-preview-text">Can I arrive after 18:00?</span>
        </div>
        <button class="reply-preview-close" type="button" aria-label="Cancel reply">
          ${CLOSE_ICON}
        </button>
      </div>

      <div class="draft-banner ${showDraft ? '' : 'hidden'}">
        <span id="draft-banner-text">Draft saved for this conversation</span>
        <button class="draft-clear-button" type="button">Discard draft</button>
      </div>

      <div class="quick-replies-bar">
        <div class="quick-replies-header">
          <span class="quick-replies-label">Quick replies</span>
          <button class="quick-reply-save" type="button">Save draft</button>
        </div>
        <div class="quick-replies-list">
          <button class="quick-reply-chip" type="button">
            <span class="quick-reply-text">Happy to confirm once the taxi is booked.</span>
            <span class="quick-reply-remove">x</span>
          </button>
          <button class="quick-reply-chip" type="button">
            <span class="quick-reply-text">I will send the corrected deck before lunch.</span>
            <span class="quick-reply-remove">x</span>
          </button>
        </div>
      </div>

      <div class="input-container">
        <div class="emoji-button-container">
          ${renderIconButton('emoji-button', 'Insert emoji', EMOJI_ICON)}
        </div>
        ${renderIconButton('attach-button', 'Attach image', ATTACH_ICON)}
        <textarea id="storybook-message-input" rows="1" maxlength="65536" placeholder="Type a message">18:30 works on our side. Please send the taxi plate when you have it.</textarea>
        <div class="send-button-group">
          <button class="send-button" type="button" title="Send message" aria-label="Send message">
            ${SEND_ICON}
          </button>
          <button class="send-dropdown-toggle" type="button" title="More send options" aria-label="More send options">
            <svg viewBox="0 0 24 24" width="12" height="12" aria-hidden="true">
              <path fill="currentColor" d="M7 10l5 5 5-5z"></path>
            </svg>
          </button>
        </div>
      </div>
    </footer>
  `;
}

export function renderWorkspacePanel({ collapsed = false, reminderDue = true } = {}) {
  const workspaceClasses = joinClasses(
    'conversation-workspace',
    collapsed && 'collapsed',
    reminderDue && 'has-reminder-due',
  );

  return `
    <aside id="conversation-workspace" class="${workspaceClasses}">
      <div class="conversation-workspace-frame">
        <div class="workspace-header">
          <div>
            <span class="workspace-kicker">Workspace</span>
            <h3>Conversation context</h3>
          </div>
          <button class="workspace-edit-button" type="button">Edit</button>
        </div>

        <div class="workspace-summary">
          ${renderBadge({ type: 'priority urgent', label: 'Urgent' })}
          ${renderBadge({ type: 'reply', label: 'Reply' })}
          ${renderBadge({ type: 'label', label: 'Travel' })}
        </div>

        <div class="workspace-section">
          <span class="workspace-section-label">Right now</span>
          <p class="workspace-primary-status">Awaiting the final arrival time and the taxi plate before 18:30.</p>
        </div>

        <div class="workspace-section">
          <span class="workspace-section-label">Notes</span>
          <p class="workspace-notes">Guest is landing from Madrid. Keep replies short, friendly, and practical. Mention the blue gate rather than the street name.</p>
        </div>

        <div class="workspace-section">
          <div class="workspace-section-heading">
            <span class="workspace-section-label">Checklist</span>
            <span class="workspace-section-meta">1 open task</span>
          </div>
          <div class="workspace-checklist">
            <button class="workspace-checklist-item" type="button">
              <span class="workspace-checklist-mark"></span>
              <span class="workspace-checklist-text">Save the taxi plate when it arrives</span>
            </button>
            <button class="workspace-checklist-item done" type="button">
              <span class="workspace-checklist-mark">${CHECK_ICON}</span>
              <span class="workspace-checklist-text">Confirm the lockbox instructions</span>
            </button>
          </div>
        </div>

        <div class="workspace-section">
          <div class="workspace-section-heading">
            <span class="workspace-section-label">Labels</span>
            <span class="workspace-section-meta">18:07 local time</span>
          </div>
          <div class="workspace-labels">
            <span class="workspace-label-chip">Travel</span>
            <span class="workspace-label-chip">Arrival</span>
            <span class="workspace-label-chip">Host</span>
          </div>
        </div>

        <div class="workspace-section">
          <span class="workspace-section-label">Reminder</span>
          <p class="workspace-reminder ${reminderDue ? 'due' : ''}">Reminder due: follow up if the taxi plate has not arrived by 17:45.</p>
        </div>
      </div>
    </aside>
  `;
}

export function renderChatHeader() {
  return `
    <header class="chat-header">
      <button class="back-button" aria-label="Back to contacts" type="button">
        <svg viewBox="0 0 24 24" width="24" height="24" aria-hidden="true">
          <path fill="currentColor" d="M20 11H7.83l5.59-5.59L12 4l-8 8 8 8 1.41-1.41L7.83 13H20v-2z"></path>
        </svg>
      </button>
      <div class="avatar">
        <span>S</span>
      </div>
      <div class="chat-info">
        <span class="chat-name">Sofia at Casa Azul</span>
        <span class="chat-note-indicator">Travel host · arrival reminder due</span>
      </div>
      <div class="chat-actions">
        ${renderIconButton('chat-settings-button chat-workspace-button', 'Toggle workspace', WORKSPACE_ICON)}
        <span class="cost-badge chat-cost-badge">$2.38</span>
        ${renderIconButton('chat-settings-button chat-menu-button', 'Conversation actions', MENU_ICON)}
      </div>
    </header>
  `;
}

export function renderConversationLayout({ mobileWorkspaceOpen = false } = {}) {
  return renderAppFrame(`
    <div id="main-container" class="chat-open">
      ${renderContactsPanel({ activeId: 'sofia' })}
      <main id="messages-panel">
        <div id="chat-view" class="chat-view ${mobileWorkspaceOpen ? 'workspace-open' : ''}">
          ${renderChatHeader()}
          <button class="workspace-backdrop ${mobileWorkspaceOpen ? '' : 'hidden'}" type="button" aria-label="Close conversation context"></button>
          <div class="chat-shell">
            <div class="chat-main-column">
              ${renderMessagesList()}
              ${renderComposer()}
            </div>
            ${renderWorkspacePanel()}
          </div>
        </div>
      </main>
    </div>
  `, { page: true });
}

export function renderInboxLayout() {
  return renderAppFrame(`
    <div id="main-container">
      ${renderContactsPanel({ activeId: null })}
      <main id="messages-panel">
        ${renderEmptyChatState()}
      </main>
    </div>
  `, { page: true });
}
