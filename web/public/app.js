// WhatsApp Translator Web Client

import {
  buildConversationBrief,
  buildConversationActionPlan,
  buildConversationHandoffBrief,
  buildVisitorDashboard,
  countMatchingMessages,
  createDemoWorkspace,
  filterMessagesByQuery,
  getAppearanceThemeCatalog,
  getComposerAssistState,
  getComposerReminderPresets,
  getContactActionSnapshot,
  getChecklistSummary,
  getContactDisplayName,
  getContactLabels,
  getMessageSnippet,
  getUntranslatedIncomingMessages,
  getDraftPreview,
  getDraftText,
  getPriorityInfo,
  getReminderStatus,
  getReplyState,
  getTimezoneInfo,
  getVisibleContacts,
  isContactSnoozed,
  isMessageStarred,
  parseLabelsInput,
  removeQuickReply,
  resolveAppearanceTheme,
  simulateTranslation,
  suggestDemoReply,
  toggleChecklistItem,
  toggleStarredMessage,
  upsertChecklistItems,
  upsertDraft,
  upsertQuickReply,
} from './app-state.js';
import { calculateViewportLayout } from './viewport.js';

class WhatsAppClient {
  constructor() {
    this.ws = null;
    this.connected = false;
    this.demoMode = false;
    this.contacts = [];
    this.currentContactId = null;
    this.settingsContactId = null;
    this.initialMessageLimit = 30;
    this.messageCacheLimit = 200;
    this.notificationsReadyAt = 0;
    this.notificationPermissionRequested = false;
    this.notificationPromptEl = null;
    this.messages = new Map();
    this.contactsRenderTimer = null;
    this.metadataRefreshTimer = null;
    this.messagesHasMore = new Map(); // contactId -> boolean (whether more messages exist)
    this.messagesLoading = new Map(); // contactId -> boolean (whether currently loading)
    this.avatarCache = new Map(); // JID -> URL
    this.avatarFetching = new Set(); // JIDs currently being fetched
    this.globalUsage = { inputTokens: 0, outputTokens: 0, costUsd: 0 };
    this.conversationUsageCache = new Map(); // contactId -> usage
    this.conversationUsageTimer = null;
    this.linkPreviewCache = new Map(); // URL -> LinkPreview
    this.linkPreviewFetching = new Set(); // URLs currently being fetched
    this.linkPreviewLoadTimer = null;
    this.typingState = new Map(); // chatId -> { userId, state, timestamp }
    this.typingTimeouts = new Map(); // chatId -> timeoutId (auto-clear after 10s)
    this.replyingTo = null; // { messageId, senderJid, senderName, text, isFromMe }
    this.authToken = localStorage.getItem('wa_auth_token'); // Auth token for API requests
    this.recentEmojis = JSON.parse(localStorage.getItem('wa_recent_emojis') || '[]');
    this.currentEmojiCategory = 'recent';
    this.draftsStorageKey = 'wa_chat_drafts';
    this.starredStorageKey = 'wa_starred_messages';
    this.contactMetadataStorageKey = 'wa_contact_metadata';
    this.inboxPreferencesStorageKey = 'wa_inbox_preferences';
    this.quickRepliesStorageKey = 'wa_quick_replies';
    this.appearanceStorageKey = 'wa_appearance_preferences';
    this.drafts = this.loadStoredJson(this.draftsStorageKey);
    this.starredMessages = this.loadStoredJson(this.starredStorageKey);
    this.contactMetadata = this.loadStoredJson(this.contactMetadataStorageKey);
    this.quickReplies = this.loadStoredArray(this.quickRepliesStorageKey);
    this.appearancePreferences = this.loadStoredJson(this.appearanceStorageKey);
    this.appearanceState = resolveAppearanceTheme(this.appearancePreferences, {
      systemMode: this.getSystemAppearanceMode(),
    });
    this.systemAppearanceQuery = null;
    this.handleSystemAppearanceChange = this.handleSystemAppearanceChange.bind(this);
    const storedInboxPreferences = this.loadStoredJson(this.inboxPreferencesStorageKey);
    this.sidebarSearchQuery = typeof storedInboxPreferences.searchQuery === 'string' ? storedInboxPreferences.searchQuery : '';
    this.contactFilters = {
      unreadOnly: Boolean(storedInboxPreferences.unreadOnly),
      groupsOnly: Boolean(storedInboxPreferences.groupsOnly),
      draftsOnly: Boolean(storedInboxPreferences.draftsOnly),
      importantOnly: Boolean(storedInboxPreferences.importantOnly),
      tasksOnly: Boolean(storedInboxPreferences.tasksOnly),
      pinnedOnly: Boolean(storedInboxPreferences.pinnedOnly),
      notesOnly: Boolean(storedInboxPreferences.notesOnly),
      dueRemindersOnly: Boolean(storedInboxPreferences.dueRemindersOnly),
      labelsOnly: Boolean(storedInboxPreferences.labelsOnly),
      snoozedOnly: Boolean(storedInboxPreferences.snoozedOnly),
      needsReplyOnly: Boolean(storedInboxPreferences.needsReplyOnly),
    };
    this.messageSearchQuery = '';
    this.starredOnly = false;
    this.fullyLoadedContacts = new Set();
    this.commandPaletteOpen = false;
    this.commandPaletteQuery = '';
    this.commandPaletteSelectionIndex = 0;
    this.commandPaletteItems = [];
    this.workspaceExpanded = !this.isMobile();
    this.mobileLayoutActive = this.isMobile();
    this.handleResponsiveLayoutChange = this.handleResponsiveLayoutChange.bind(this);
    
    // Emoji data organized by category
    this.emojiData = {
      smileys: ['😀', '😃', '😄', '😁', '😅', '😂', '🤣', '😊', '😇', '🙂', '🙃', '😉', '😌', '😍', '🥰', '😘', '😗', '😙', '😚', '😋', '😛', '😜', '🤪', '😝', '🤑', '🤗', '🤭', '🤫', '🤔', '🤐', '🤨', '😐', '😑', '😶', '😏', '😒', '🙄', '😬', '🤥', '😔', '😪', '🤤', '😴', '😷', '🤒', '🤕', '🤢', '🤮', '🤧', '🥵', '🥶', '🥴', '😵', '🤯', '🤠', '🥳', '😎', '🤓', '🧐', '😕', '😟', '🙁', '☹️', '😮', '😯', '😲', '😳', '🥺', '😦', '😧', '😨', '😰', '😥', '😢', '😭', '😱', '😖', '😣', '😞', '😓', '😩', '😫', '🥱', '😤', '😡', '😠', '🤬', '😈', '👿', '💀', '☠️', '💩', '🤡', '👹', '👺', '👻', '👽', '👾', '🤖'],
      gestures: ['👋', '🤚', '🖐️', '✋', '🖖', '👌', '🤌', '🤏', '✌️', '🤞', '🤟', '🤘', '🤙', '👈', '👉', '👆', '🖕', '👇', '☝️', '👍', '👎', '✊', '👊', '🤛', '🤜', '👏', '🙌', '👐', '🤲', '🤝', '🙏', '✍️', '💅', '🤳', '💪', '🦾', '🦿', '🦵', '🦶', '👂', '🦻', '👃', '🧠', '🫀', '🫁', '🦷', '🦴', '👀', '👁️', '👅', '👄', '👶', '🧒', '👦', '👧', '🧑', '👱', '👨', '🧔', '👩', '🧓', '👴', '👵'],
      hearts: ['❤️', '🧡', '💛', '💚', '💙', '💜', '🖤', '🤍', '🤎', '💔', '❣️', '💕', '💞', '💓', '💗', '💖', '💘', '💝', '💟', '♥️', '💌', '💋', '👄', '🫂', '👥', '👤'],
      animals: ['🐶', '🐱', '🐭', '🐹', '🐰', '🦊', '🐻', '🐼', '🐨', '🐯', '🦁', '🐮', '🐷', '🐸', '🐵', '🙈', '🙉', '🙊', '🐒', '🐔', '🐧', '🐦', '🐤', '🦆', '🦅', '🦉', '🦇', '🐺', '🐗', '🐴', '🦄', '🐝', '🪱', '🐛', '🦋', '🐌', '🐞', '🐜', '🪳', '🦂', '🐢', '🐍', '🦎', '🦖', '🦕', '🐙', '🦑', '🦐', '🦞', '🦀', '🐡', '🐠', '🐟', '🐬', '🐳', '🐋', '🦈', '🐊', '🐅', '🐆', '🦓', '🦍', '🦧', '🐘', '🦛', '🦏', '🐪', '🐫', '🦒', '🦘', '🐃', '🐂', '🐄', '🐎', '🐖', '🐏', '🐑', '🦙', '🐐', '🦌', '🐕', '🐩', '🦮', '🐕‍🦺', '🐈', '🐓', '🦃', '🦚', '🦜', '🦢', '🦩', '🐇', '🦝', '🦨', '🦡', '🦫', '🦦', '🦥', '🐁', '🐀', '🐿️', '🦔'],
      food: ['🍕', '🍔', '🍟', '🌭', '🍿', '🧂', '🥓', '🥚', '🍳', '🧇', '🥞', '🧈', '🍞', '🥐', '🥖', '🥨', '🧀', '🥗', '🥙', '🥪', '🌮', '🌯', '🫔', '🥫', '🍝', '🍜', '🍲', '🍛', '🍣', '🍱', '🥟', '🦪', '🍤', '🍙', '🍚', '🍘', '🍥', '🥠', '🥮', '🍢', '🍡', '🍧', '🍨', '🍦', '🥧', '🧁', '🍰', '🎂', '🍮', '🍭', '🍬', '🍫', '🍩', '🍪', '🌰', '🥜', '🍯', '🥛', '🍼', '☕', '🫖', '🍵', '🧃', '🥤', '🧋', '🍶', '🍺', '🍻', '🥂', '🍷', '🥃', '🍸', '🍹', '🧉', '🍾', '🧊', '🥄', '🍴', '🍽️', '🥣', '🥡', '🥢', '🧆'],
      travel: ['✈️', '🚀', '🛸', '🚁', '🛶', '⛵', '🚤', '🛥️', '🛳️', '⛴️', '🚢', '🚂', '🚃', '🚄', '🚅', '🚆', '🚇', '🚈', '🚉', '🚊', '🚝', '🚞', '🚋', '🚌', '🚍', '🚎', '🚐', '🚑', '🚒', '🚓', '🚔', '🚕', '🚖', '🚗', '🚘', '🚙', '🛻', '🚚', '🚛', '🚜', '🏎️', '🏍️', '🛵', '🦽', '🦼', '🛺', '🚲', '🛴', '🛹', '🛼', '🚏', '🛣️', '🛤️', '🛢️', '⛽', '🚨', '🚥', '🚦', '🛑', '🚧', '⚓', '🗺️', '🗿', '🗽', '🗼', '🏰', '🏯', '🏟️', '🎡', '🎢', '🎠', '⛲', '⛱️', '🏖️', '🏝️', '🏜️', '🌋', '⛰️', '🏔️', '🗻', '🏕️', '⛺', '🛖', '🏠', '🏡', '🏘️', '🏚️', '🏗️', '🏢', '🏬', '🏣', '🏤', '🏥', '🏦', '🏨', '🏪', '🏫', '🏩', '💒', '🏛️', '⛪', '🕌', '🕍', '🛕', '🕋', '⛩️'],
      objects: ['💡', '🔦', '🏮', '🪔', '📱', '📲', '💻', '🖥️', '🖨️', '⌨️', '🖱️', '🖲️', '💽', '💾', '💿', '📀', '🧮', '🎥', '🎞️', '📽️', '🎬', '📺', '📷', '📸', '📹', '📼', '🔍', '🔎', '🕯️', '💵', '💴', '💶', '💷', '💰', '💳', '💎', '⚖️', '🪜', '🧰', '🪛', '🔧', '🔨', '⚒️', '🛠️', '⛏️', '🪚', '🔩', '⚙️', '🪤', '🧱', '⛓️', '🧲', '🔫', '💣', '🧨', '🪓', '🔪', '🗡️', '⚔️', '🛡️', '🚬', '⚰️', '🪦', '⚱️', '🏺', '🔮', '📿', '🧿', '💈', '⚗️', '🔭', '🔬', '🕳️', '🩹', '🩺', '💊', '💉', '🩸', '🧬', '🦠', '🧫', '🧪'],
      symbols: ['✨', '⭐', '🌟', '💫', '⚡', '🔥', '💥', '☄️', '🌈', '☀️', '🌤️', '⛅', '🌥️', '☁️', '🌦️', '🌧️', '⛈️', '🌩️', '🌨️', '❄️', '☃️', '⛄', '🌬️', '💨', '🌪️', '🌫️', '🌊', '💧', '💦', '☔', '🎵', '🎶', '🎼', '🎤', '🎧', '📻', '🎷', '🪗', '🎸', '🎹', '🎺', '🎻', '🪕', '🥁', '🪘', '⚽', '🏀', '🏈', '⚾', '🥎', '🎾', '🏐', '🏉', '🥏', '🎱', '🪀', '🏓', '🏸', '🏒', '🏑', '🥍', '🏏', '🪃', '🥅', '⛳', '🪁', '🏹', '🎣', '🤿', '🥊', '🥋', '🎽', '🛷', '⛸️', '🥌', '🎿', '⛷️', '🏂', '🪂', '🏋️', '🎯', '🎮', '🕹️', '🎰', '🧩', '♠️', '♥️', '♦️', '♣️', '🃏', '🀄', '🎴', '🎭', '🎨']
    };
    
    this.setupAppearanceSync();
    this.applyAppearanceTheme();
    this.init();
  }

  async init() {
    // Check if authentication is required
    const authRequired = await this.checkAuth();
    
    if (authRequired && !this.authToken) {
      this.showPasswordOverlay();
      this.bindPasswordEvents();
      return;
    }
    
    // If we have a token, verify it's still valid
    if (authRequired && this.authToken) {
      const valid = await this.verifyToken();
      if (!valid) {
        this.authToken = null;
        localStorage.removeItem('wa_auth_token');
        this.showPasswordOverlay();
        this.bindPasswordEvents();
        return;
      }
    }
    
    // Auth passed, continue with normal initialization
    this.startApp();
  }

  startApp() {
    document.getElementById('password-overlay')?.classList.add('hidden');
    this.connectWebSocket();
    this.bindEvents();
    this.updateInputPlaceholder();
    this.setupVisualViewport();
    this.updateNotificationPrompt();
    this.setupNotificationPermissionRequest();
    this.updateDraftBanner();
    this.renderQuickReplies();
    this.updateConversationMenuUI();
    this.updateStarredToggleUI();
    this.updateChatSearchUI();
    this.syncInboxControls();
    this.syncAppearanceControls();
    this.updateWorkspaceUI();
    this.renderVisitorDashboard();
  }

  loadStoredJson(key) {
    try {
      return JSON.parse(localStorage.getItem(key) || '{}');
    } catch (err) {
      console.warn(`Failed to load local state for ${key}:`, err);
      return {};
    }
  }

  loadStoredArray(key) {
    try {
      const parsed = JSON.parse(localStorage.getItem(key) || '[]');
      return Array.isArray(parsed) ? parsed : [];
    } catch (err) {
      console.warn(`Failed to load local array state for ${key}:`, err);
      return [];
    }
  }

  saveStoredJson(key, value) {
    localStorage.setItem(key, JSON.stringify(value || {}));
  }

  saveStoredArray(key, value) {
    localStorage.setItem(key, JSON.stringify(Array.isArray(value) ? value : []));
  }

  getSystemAppearanceMode() {
    return window.matchMedia?.('(prefers-color-scheme: dark)').matches ? 'dark' : 'light';
  }

  setupAppearanceSync() {
    if (!window.matchMedia) return;
    this.systemAppearanceQuery = window.matchMedia('(prefers-color-scheme: dark)');
    if (typeof this.systemAppearanceQuery.addEventListener === 'function') {
      this.systemAppearanceQuery.addEventListener('change', this.handleSystemAppearanceChange);
    } else if (typeof this.systemAppearanceQuery.addListener === 'function') {
      this.systemAppearanceQuery.addListener(this.handleSystemAppearanceChange);
    }
  }

  handleSystemAppearanceChange() {
    if ((this.appearancePreferences?.mode || 'system') === 'system') {
      this.applyAppearanceTheme();
    }
  }

  persistAppearancePreferences() {
    this.saveStoredJson(this.appearanceStorageKey, this.appearancePreferences);
  }

  syncAppearanceControls(preferences = this.appearancePreferences) {
    const themeSelect = document.getElementById('appearance-theme-select');
    const modeSelect = document.getElementById('appearance-mode-select');
    const preview = document.getElementById('appearance-preview-text');
    const previewState = resolveAppearanceTheme(preferences, {
      systemMode: this.getSystemAppearanceMode(),
    });
    const theme = previewState.theme;
    const mode = previewState.mode;
    const resolvedMode = previewState.resolvedMode;
    const label = previewState.definition.label || 'WhatsApp';

    if (themeSelect) {
      const currentOptions = Array.from(themeSelect.options).map(option => option.value);
      const expectedOptions = getAppearanceThemeCatalog().map(themeOption => themeOption.id);
      if (currentOptions.join('|') !== expectedOptions.join('|')) {
        themeSelect.innerHTML = getAppearanceThemeCatalog()
          .map(themeOption => `<option value="${themeOption.id}">${themeOption.label}</option>`)
          .join('');
      }
      themeSelect.value = theme;
    }
    if (modeSelect) modeSelect.value = mode;
    if (preview) {
      const modeLabel = resolvedMode === 'dark' ? 'Dark' : 'Light';
      const sourceLabel = mode === 'system' ? 'following system' : `${mode} mode`;
      preview.textContent = `${label} · ${modeLabel} variant (${sourceLabel})`;
    }
  }

  applyAppearanceTheme() {
    this.appearanceState = resolveAppearanceTheme(this.appearancePreferences, {
      systemMode: this.getSystemAppearanceMode(),
    });

    const root = document.documentElement;
    if (root) {
      root.dataset.theme = this.appearanceState.dataTheme;
      root.style.colorScheme = this.appearanceState.resolvedMode;
    }

    const themeMeta = document.querySelector('meta[name="theme-color"]');
    if (themeMeta) {
      themeMeta.setAttribute('content', this.appearanceState.definition.themeColor);
    }

    this.syncAppearanceControls();
  }

  openAppearanceModal() {
    const modal = document.getElementById('appearance-modal');
    if (!modal) return;
    this.syncAppearanceControls();
    modal.classList.remove('hidden');
  }

  closeAppearanceModal() {
    document.getElementById('appearance-modal')?.classList.add('hidden');
  }

  saveAppearanceSettings() {
    const theme = document.getElementById('appearance-theme-select')?.value || 'whatsapp';
    const mode = document.getElementById('appearance-mode-select')?.value || 'system';
    this.appearancePreferences = { theme, mode };
    this.persistAppearancePreferences();
    this.applyAppearanceTheme();
    this.closeAppearanceModal();
  }

  persistDrafts() {
    this.saveStoredJson(this.draftsStorageKey, this.drafts);
  }

  persistStarredMessages() {
    this.saveStoredJson(this.starredStorageKey, this.starredMessages);
  }

  persistContactMetadata() {
    this.saveStoredJson(this.contactMetadataStorageKey, this.contactMetadata);
  }

  persistQuickReplies() {
    this.saveStoredArray(this.quickRepliesStorageKey, this.quickReplies);
  }

  persistInboxPreferences() {
    this.saveStoredJson(this.inboxPreferencesStorageKey, {
      searchQuery: this.sidebarSearchQuery,
      ...this.contactFilters,
    });
  }

  getContactMetadata(contactId) {
    return this.contactMetadata?.[contactId] || {};
  }

  getContactNotePreview(contactId, maxLength = 64) {
    const notes = String(this.getContactMetadata(contactId).notes || '').trim();
    if (!notes) return '';
    return notes.length > maxLength ? `${notes.slice(0, maxLength - 1)}…` : notes;
  }

  getContactLabels(contactId) {
    return getContactLabels(this.getContactMetadata(contactId));
  }

  getContactDisplayName(contact) {
    return getContactDisplayName(contact, this.getContactMetadata(contact?.id));
  }

  getPriorityInfo(contactId) {
    return getPriorityInfo(this.getContactMetadata(contactId));
  }

  getChecklistSummary(contactId) {
    return getChecklistSummary(this.getContactMetadata(contactId));
  }

  getTimezoneInfo(contactId, now = Date.now()) {
    return getTimezoneInfo(this.getContactMetadata(contactId), now);
  }

  formatChecklistForTextarea(contactId) {
    const checklist = this.getContactMetadata(contactId).checklist || [];
    return checklist
      .map(item => `- [${item.done ? 'x' : ' '}] ${item.text}`)
      .join('\n');
  }

  getReplyState(contactId, now = Date.now()) {
    const contact = this.contacts.find(entry => entry.id === contactId) || { id: contactId };
    return getReplyState({
      contact,
      messages: this.messages.get(contactId) || [],
      drafts: this.drafts,
      metadata: this.getContactMetadata(contactId),
      contactId,
      now,
    });
  }

  getReplySummary(contactId, now = Date.now()) {
    const replyState = this.getReplyState(contactId, now);
    if (replyState === 'drafting') return 'Draft reply saved';
    if (replyState === 'needs-reply') return 'Awaiting your reply';
    if (replyState === 'waiting') return 'Waiting for their response';
    return '';
  }

  getReminderSummary(contactId, now = Date.now()) {
    const metadata = this.getContactMetadata(contactId);
    const status = getReminderStatus(metadata, now);
    if (status === 'none') return '';

    const reminderText = String(metadata.reminderText || '').trim();
    const reminderAt = Number(metadata.reminderAt || 0);
    const when = reminderAt ? this.formatMetadataTime(reminderAt) : 'soon';
    const prefix = status === 'due' ? 'Reminder due' : 'Reminder';
    return reminderText ? `${prefix}: ${reminderText} · ${when}` : `${prefix} · ${when}`;
  }

  getSnoozeSummary(contactId, now = Date.now()) {
    const metadata = this.getContactMetadata(contactId);
    if (!isContactSnoozed(metadata, now)) return '';
    return `Snoozed until ${this.formatMetadataTime(Number(metadata.snoozedUntil))}`;
  }

  getCurrentContact() {
    return this.contacts.find(contact => contact.id === this.currentContactId) || null;
  }

  getLatestIncomingMessage(contactId = this.currentContactId) {
    const messages = this.messages.get(contactId) || [];
    for (let index = messages.length - 1; index >= 0; index -= 1) {
      const message = messages[index];
      if (!message?.isFromMe && getMessageSnippet(message, 96) !== '[Message]') {
        return message;
      }
    }
    return null;
  }

  getCurrentComposerAssistState() {
    const input = document.getElementById('message-input');
    const contact = this.getCurrentContact() || { id: this.currentContactId };
    return getComposerAssistState({
      draftText: input?.value || getDraftText(this.drafts, this.currentContactId),
      metadata: this.getContactMetadata(this.currentContactId),
      contact,
      latestIncomingMessage: this.getLatestIncomingMessage(),
      messages: this.messages.get(this.currentContactId) || [],
      demoMode: this.demoMode,
    });
  }

  renderComposerAssist() {
    const container = document.getElementById('composer-assist');
    if (!container) return;

    if (!this.currentContactId) {
      container.classList.add('hidden');
      return;
    }

    const state = this.getCurrentComposerAssistState();
    const kicker = document.getElementById('composer-assist-kicker');
    const title = document.getElementById('composer-assist-title');
    const preview = document.getElementById('composer-assist-preview');
    const suggestionButton = document.getElementById('composer-use-suggestion');
    const profilePresets = document.getElementById('composer-profile-presets');
    const smartReplies = document.getElementById('composer-smart-replies');
    const reminderPresets = document.getElementById('composer-reminder-presets');
    const readiness = document.getElementById('composer-readiness');

    if (kicker) kicker.textContent = state.previewLabel;
    if (title) title.textContent = `${state.targetLanguage} · ${state.styleSummary}`;
    if (preview) {
      if (state.translatedPreview) {
        preview.textContent = state.translatedPreview;
      } else if (state.latestIncomingSnippet) {
        preview.textContent = `Latest incoming: ${state.latestIncomingSnippet}`;
      } else {
        preview.textContent = 'Set a language profile or type a draft to preview the translation route.';
      }
    }
    if (suggestionButton) {
      suggestionButton.disabled = !state.canUseSuggestedReply;
      suggestionButton.title = state.canUseSuggestedReply
        ? 'Use the first reply starter based on the latest incoming message'
        : 'No incoming message available for a smart reply yet';
    }
    if (profilePresets) {
      const languageButtons = state.profilePresets.languages.map((preset) => `
        <button
          class="composer-preset-chip ${preset.active ? 'active' : ''}"
          type="button"
          data-composer-language="${this.escapeHtml(preset.value)}"
        >${this.escapeHtml(preset.label)}</button>
      `).join('');
      const toneButtons = state.profilePresets.tones.map((preset) => `
        <button
          class="composer-preset-chip ${preset.active ? 'active' : ''}"
          type="button"
          data-composer-tone="${this.escapeHtml(preset.value)}"
        >${this.escapeHtml(preset.label)}</button>
      `).join('');
      profilePresets.innerHTML = `
        <div class="composer-preset-row"><span>Language</span>${languageButtons}</div>
        <div class="composer-preset-row"><span>Tone</span>${toneButtons}</div>
      `;
    }
    if (smartReplies) {
      smartReplies.innerHTML = state.smartReplies.length > 0
        ? state.smartReplies.map((reply) => `
          <button
            class="composer-smart-reply"
            type="button"
            data-smart-reply-id="${this.escapeHtml(reply.id)}"
            title="${this.escapeHtml(reply.text)}"
          >
            <span>${this.escapeHtml(reply.label)}</span>
            <small>${this.escapeHtml(reply.text)}</small>
          </button>
        `).join('')
        : '';
    }
    if (reminderPresets) {
      reminderPresets.innerHTML = state.reminderPresets.map((preset) => `
        <button
          class="composer-reminder-chip"
          type="button"
          data-reminder-preset="${this.escapeHtml(preset.id)}"
          title="${this.escapeHtml(preset.reason || 'Set follow-up reminder')}"
        >${this.escapeHtml(preset.label)}</button>
      `).join('');
    }
    if (readiness) {
      readiness.innerHTML = state.readiness.checks.map((check) => `
        <span class="composer-readiness-chip ${this.escapeHtml(check.status)}">
          ${this.escapeHtml(check.label)}
        </span>
      `).join('');
      readiness.setAttribute('aria-label', `Composer checks: ${state.readiness.scoreLabel}`);
    }

    container.classList.toggle('hidden', !state.showPreview);
  }

  useComposerSuggestedReply(replyId = 'confirm') {
    if (!this.currentContactId) return;
    const state = this.getCurrentComposerAssistState();
    const selectedReply = state.smartReplies.find(reply => reply.id === replyId) || state.smartReplies[0];
    const replyText = selectedReply?.text || state.suggestedReply;
    if (!replyText) return;

    const input = document.getElementById('message-input');
    if (!input) return;

    input.value = replyText;
    this.autoResizeTextarea(input);
    this.updateSendButton();
    this.handleDraftInput();
    input.focus();
  }

  applyComposerProfilePreset(updates = {}) {
    if (!this.currentContactId) return;

    const nextUpdates = {};
    if (updates.language) {
      nextUpdates.languageOverride = updates.language;
      nextUpdates.targetLanguage = updates.language;
    }
    if (updates.tone) {
      nextUpdates.translationStyle = updates.tone;
    }
    if (Object.keys(nextUpdates).length === 0) return;

    this.updateContactMetadata(this.currentContactId, nextUpdates);
    const contact = this.getCurrentContact();
    if (contact) {
      Object.assign(contact, nextUpdates);
    }
    this.updateChatHeaderNote();
    this.renderContacts();
    this.renderComposerAssist();
    this.renderConversationWorkspace();
  }

  setComposerReminderPreset(presetId = 'tomorrow') {
    if (!this.currentContactId) return;

    const contact = this.getCurrentContact() || { id: this.currentContactId };
    const input = document.getElementById('message-input');
    const draft = String(input?.value || '').trim();
    const latestIncoming = this.getLatestIncomingMessage();
    const reminderSource = draft || (latestIncoming ? getMessageSnippet(latestIncoming, 72) : '');
    const displayName = this.getContactDisplayName(contact);
    const state = this.getCurrentComposerAssistState();
    const fallbackPresets = getComposerReminderPresets();
    const preset = state.reminderPresets.find(entry => entry.id === presetId)
      || fallbackPresets.find(entry => entry.id === 'tomorrow');

    this.updateContactMetadata(this.currentContactId, {
      reminderText: reminderSource ? `Follow up: ${reminderSource}` : `Follow up with ${displayName}`,
      reminderAt: preset?.reminderAt || Date.now() + 86_400_000,
    });
    this.renderContacts();
    this.updateChatHeaderNote();
    this.renderComposerAssist();
  }

  setComposerReminderTomorrow() {
    this.setComposerReminderPreset('tomorrow');
  }

  buildContactBadgeMarkup(contactId, now = Date.now()) {
    const metadata = this.getContactMetadata(contactId);
    const badges = [];
    const priority = this.getPriorityInfo(contactId);
    const checklist = this.getChecklistSummary(contactId);
    const reminderStatus = getReminderStatus(metadata, now);
    const replyState = this.getReplyState(contactId, now);
    if (priority.isImportant) {
      badges.push(`<span class="contact-meta-chip priority ${priority.value}">${priority.label}</span>`);
    }
    if (replyState === 'needs-reply') {
      badges.push('<span class="contact-meta-chip reply">Reply</span>');
    } else if (replyState === 'drafting') {
      badges.push('<span class="contact-meta-chip drafting">Drafting</span>');
    } else if (replyState === 'waiting') {
      badges.push('<span class="contact-meta-chip waiting">Waiting</span>');
    }

    if (checklist.open > 0) {
      badges.push(`<span class="contact-meta-chip tasks">${checklist.open} task${checklist.open === 1 ? '' : 's'}</span>`);
    }

    if (reminderStatus === 'due') {
      badges.push('<span class="contact-meta-chip due">Due</span>');
    } else if (reminderStatus === 'upcoming') {
      badges.push('<span class="contact-meta-chip reminder">Reminder</span>');
    }

    if (isContactSnoozed(metadata, now)) {
      badges.push('<span class="contact-meta-chip snoozed">Snoozed</span>');
    }

    if (this.getContactNotePreview(contactId)) {
      badges.push('<span class="contact-meta-chip">Note</span>');
    }

    this.getContactLabels(contactId).slice(0, 2).forEach((label) => {
      badges.push(`<span class="contact-meta-chip label">${this.escapeHtml(label)}</span>`);
    });

    return badges.join('');
  }

  buildChatMetadataSummary(contactId, now = Date.now()) {
    if (!contactId) return '';

    const pieces = [];
    const metadata = this.getContactMetadata(contactId);
    const priority = this.getPriorityInfo(contactId);
    const checklist = this.getChecklistSummary(contactId);
    const timezoneInfo = this.getTimezoneInfo(contactId, now);

    if (priority.value !== 'normal') {
      pieces.push(`${priority.label} priority`);
    }

    const replySummary = this.getReplySummary(contactId, now);
    if (replySummary) {
      pieces.push(replySummary);
    }

    if (checklist.open > 0) {
      pieces.push(`${checklist.open} open task${checklist.open === 1 ? '' : 's'}`);
    }

    const reminderStatus = getReminderStatus(metadata, now);
    if (reminderStatus === 'due') {
      pieces.push('Reminder due');
    } else if (reminderStatus === 'upcoming') {
      pieces.push('Reminder set');
    }

    if (isContactSnoozed(metadata, now)) {
      pieces.push('Snoozed');
    }

    if (timezoneInfo) {
      pieces.push(`${timezoneInfo.label} · ${timezoneInfo.statusLabel}`);
    }

    return pieces.slice(0, 3).join(' • ');
  }

  scheduleMetadataRefresh() {
    if (this.metadataRefreshTimer) {
      clearTimeout(this.metadataRefreshTimer);
      this.metadataRefreshTimer = null;
    }

    const now = Date.now();
    const nextTransitions = Object.values(this.contactMetadata || {})
      .flatMap((metadata) => [metadata?.reminderAt, metadata?.snoozedUntil])
      .map(value => Number(value || 0))
      .filter(value => Number.isFinite(value) && value > now)
      .sort((a, b) => a - b);

    if (nextTransitions.length === 0) {
      return;
    }

    const delay = Math.min(Math.max(nextTransitions[0] - now, 250), 2_147_483_647);
    this.metadataRefreshTimer = window.setTimeout(() => {
      this.metadataRefreshTimer = null;
      this.renderContacts();
      this.updateChatHeaderNote();
      this.renderConversationWorkspace();
      this.scheduleMetadataRefresh();
    }, delay + 50);
  }

  updateContactMetadata(contactId, updates = {}) {
    if (!contactId) return {};

    const current = this.getContactMetadata(contactId);
    const next = { ...current };
    for (const [key, value] of Object.entries(updates)) {
      if (value === null || value === undefined || value === '') {
        delete next[key];
      } else {
        next[key] = value;
      }
    }

    if (Object.keys(next).length === 0) {
      delete this.contactMetadata[contactId];
    } else {
      this.contactMetadata[contactId] = next;
    }

    this.persistContactMetadata();
    this.scheduleMetadataRefresh();
    this.renderVisitorDashboard();
    this.renderConversationWorkspace();
    this.renderComposerAssist();
    return this.getContactMetadata(contactId);
  }

  applyStoredContactMetadata(contact) {
    if (!contact?.id) return contact;
    return { ...contact, ...this.getContactMetadata(contact.id) };
  }

  syncInboxControls() {
    const searchInput = document.getElementById('inbox-search-input');
    if (searchInput && searchInput.value !== this.sidebarSearchQuery) {
      searchInput.value = this.sidebarSearchQuery;
    }

    document.querySelectorAll('[data-inbox-filter]').forEach((button) => {
      const key = button.dataset.inboxFilter;
      button.classList.toggle('active', Boolean(this.contactFilters[key]));
    });
  }

  setInboxSearchQuery(value) {
    this.sidebarSearchQuery = String(value || '').trimStart();
    this.persistInboxPreferences();
    this.renderContacts();
  }

  toggleInboxFilter(filterName) {
    if (!(filterName in this.contactFilters)) return;
    this.contactFilters[filterName] = !this.contactFilters[filterName];
    this.persistInboxPreferences();
    this.syncInboxControls();
    this.renderContacts();
  }

  clearInboxFilters() {
    this.sidebarSearchQuery = '';
    for (const key of Object.keys(this.contactFilters)) {
      this.contactFilters[key] = false;
    }
    this.persistInboxPreferences();
    this.syncInboxControls();
    this.renderContacts();
  }

  buildMessagePreviewLookup() {
    const previewByContact = {};
    for (const contact of this.contacts) {
      const messages = this.messages.get(contact.id) || [];
      const lastMessage = messages[messages.length - 1];
      if (lastMessage) {
        previewByContact[contact.id] = this.getMessagePreview(lastMessage);
      } else if (contact.lastMessagePreview) {
        previewByContact[contact.id] = contact.lastMessagePreview;
      }
    }
    return previewByContact;
  }

  getFilteredContacts() {
    const now = Date.now();
    const messagesByContact = Object.fromEntries(this.messages.entries());
    return getVisibleContacts({
      contacts: this.contacts,
      drafts: this.drafts,
      searchQuery: this.sidebarSearchQuery,
      filters: this.contactFilters,
      messagePreviewByContact: this.buildMessagePreviewLookup(),
      metadataByContact: this.contactMetadata,
      messagesByContact,
      now,
    }).sort((a, b) => {
      const aMeta = this.getContactMetadata(a.id);
      const bMeta = this.getContactMetadata(b.id);
      const aPinned = a.pinnedAt != null;
      const bPinned = b.pinnedAt != null;
      if (aPinned && !bPinned) return -1;
      if (!aPinned && bPinned) return 1;
      if (aPinned && bPinned) return a.pinnedAt - b.pinnedAt;

      const aPriority = this.getPriorityInfo(a.id);
      const bPriority = this.getPriorityInfo(b.id);
      if (aPriority.rank !== bPriority.rank) return aPriority.rank - bPriority.rank;

      const aChecklist = this.getChecklistSummary(a.id);
      const bChecklist = this.getChecklistSummary(b.id);
      if (aChecklist.open > 0 && bChecklist.open === 0) return -1;
      if (aChecklist.open === 0 && bChecklist.open > 0) return 1;

      const aReminder = getReminderStatus(aMeta, now);
      const bReminder = getReminderStatus(bMeta, now);
      if (aReminder === 'due' && bReminder !== 'due') return -1;
      if (aReminder !== 'due' && bReminder === 'due') return 1;
      if (aReminder === 'upcoming' && bReminder === 'none') return -1;
      if (aReminder === 'none' && bReminder === 'upcoming') return 1;

      const replyRank = { drafting: 0, 'needs-reply': 1, waiting: 2, idle: 3, snoozed: 4 };
      const aReplyRank = replyRank[this.getReplyState(a.id, now)] ?? 99;
      const bReplyRank = replyRank[this.getReplyState(b.id, now)] ?? 99;
      if (aReplyRank !== bReplyRank) return aReplyRank - bReplyRank;

      const aDraft = getDraftText(this.drafts, a.id);
      const bDraft = getDraftText(this.drafts, b.id);
      if (aDraft && !bDraft) return -1;
      if (!aDraft && bDraft) return 1;

      return (b.lastMessageTime || 0) - (a.lastMessageTime || 0);
    });
  }

  getContactActionSnapshot(contactId, now = Date.now()) {
    const contact = this.contacts.find(entry => entry.id === contactId) || { id: contactId };
    return getContactActionSnapshot({
      contact,
      drafts: this.drafts,
      metadata: this.getContactMetadata(contactId),
      messages: this.messages.get(contactId) || [],
      now,
    });
  }

  getVisitorDashboardData(now = Date.now()) {
    return buildVisitorDashboard({
      contacts: this.contacts,
      drafts: this.drafts,
      metadataByContact: this.contactMetadata,
      messagesByContact: Object.fromEntries(this.messages.entries()),
      now,
    });
  }

  applyInboxPreset(preset = 'all') {
    this.sidebarSearchQuery = '';
    for (const key of Object.keys(this.contactFilters)) {
      this.contactFilters[key] = false;
    }

    const filterKeyByPreset = {
      needsReply: 'needsReplyOnly',
      dueReminders: 'dueRemindersOnly',
      drafts: 'draftsOnly',
      openTasks: 'tasksOnly',
      pinned: 'pinnedOnly',
    };

    const filterKey = filterKeyByPreset[preset];
    if (filterKey) {
      this.contactFilters[filterKey] = true;
    }

    this.persistInboxPreferences();
    this.syncInboxControls();
    this.renderContacts();
  }

  renderVisitorDashboard() {
    const container = document.getElementById('visitor-dashboard');
    const statsEl = document.getElementById('visitor-dashboard-stats');
    const focusEl = document.getElementById('visitor-dashboard-focus');
    const remindersEl = document.getElementById('visitor-dashboard-reminders');
    const focusCountEl = document.getElementById('visitor-dashboard-focus-count');
    const reminderCountEl = document.getElementById('visitor-dashboard-reminder-count');
    if (!container || !statsEl || !focusEl || !remindersEl) return;

    if ((this.contacts || []).length === 0) {
      container.classList.add('hidden');
      return;
    }

    container.classList.remove('hidden');
    const dashboard = this.getVisitorDashboardData();
    const statCards = [
      {
        id: 'needsReply',
        label: 'Needs reply',
        value: dashboard.stats.needsReply,
        description: 'Chats waiting on you',
        active: this.contactFilters.needsReplyOnly,
      },
      {
        id: 'dueReminders',
        label: 'Due reminders',
        value: dashboard.stats.dueReminders,
        description: 'Follow-ups that are due now',
        active: this.contactFilters.dueRemindersOnly,
      },
      {
        id: 'drafts',
        label: 'Saved drafts',
        value: dashboard.stats.drafts,
        description: 'Replies you already started',
        active: this.contactFilters.draftsOnly,
      },
      {
        id: 'openTasks',
        label: 'Open tasks',
        value: dashboard.stats.openTasks,
        description: 'Checklist items still open',
        active: this.contactFilters.tasksOnly,
      },
    ];

    statsEl.innerHTML = statCards.map(card => `
      <button
        class="visitor-stat-card ${card.active ? 'active' : ''}"
        type="button"
        data-dashboard-preset="${card.id}"
      >
        <span class="visitor-stat-value">${card.value}</span>
        <span class="visitor-stat-label">${card.label}</span>
        <span class="visitor-stat-description">${card.description}</span>
      </button>
    `).join('');

    focusCountEl.textContent = `${dashboard.focusContacts.length} active`;
    reminderCountEl.textContent = `${dashboard.upcomingReminders.length} scheduled`;

    if (dashboard.focusContacts.length === 0) {
      focusEl.innerHTML = `
        <div class="visitor-dashboard-empty">
          <strong>Your inbox is in good shape.</strong>
          <span>No replies, reminders, or open tasks are asking for attention right now.</span>
        </div>
      `;
    } else {
      focusEl.innerHTML = dashboard.focusContacts.map(({ contact, snapshot }) => `
        <button class="visitor-focus-item" type="button" data-dashboard-contact="${contact.id}">
          <div class="visitor-focus-copy">
            <span class="visitor-focus-name">${this.escapeHtml(this.getContactDisplayName(contact))}</span>
            <span class="visitor-focus-summary">${this.escapeHtml(snapshot.summary || snapshot.headline)}</span>
          </div>
          <div class="visitor-focus-meta">
            <span class="visitor-focus-time">${this.escapeHtml(snapshot.lastMessageTime ? this.formatTime(snapshot.lastMessageTime) : 'No recent activity')}</span>
          </div>
        </button>
      `).join('');
    }

    if (dashboard.upcomingReminders.length === 0) {
      remindersEl.innerHTML = `
        <div class="visitor-dashboard-empty">
          <strong>No upcoming reminders yet.</strong>
          <span>Set a reminder inside any conversation to keep your future follow-ups visible.</span>
        </div>
      `;
    } else {
      remindersEl.innerHTML = dashboard.upcomingReminders.map(({ contact, snapshot }) => `
        <button class="visitor-focus-item visitor-reminder-item" type="button" data-dashboard-contact="${contact.id}">
          <div class="visitor-focus-copy">
            <span class="visitor-focus-name">${this.escapeHtml(this.getContactDisplayName(contact))}</span>
            <span class="visitor-focus-summary">${this.escapeHtml(this.getReminderSummary(contact.id) || snapshot.summary)}</span>
          </div>
          <div class="visitor-focus-meta">
            <span class="visitor-focus-time">${this.escapeHtml(this.formatMetadataTime(snapshot.reminderAt))}</span>
          </div>
        </button>
      `).join('');
    }
  }

  startDemoWorkspace() {
    const demo = createDemoWorkspace();
    this.demoMode = true;
    this.connected = true;

    if (this.ws) {
      this.ws.onclose = null;
      this.ws.close();
      this.ws = null;
    }

    this.contacts = demo.contacts.map(contact => ({ ...contact }));
    this.messages = new Map(
      Object.entries(demo.messagesByContact).map(([contactId, messages]) => [
        contactId,
        messages.map(message => ({ ...message, content: { ...(message.content || {}) } })),
      ]),
    );
    this.contactMetadata = {
      ...this.contactMetadata,
      ...demo.metadataByContact,
    };
    this.drafts = {
      ...demo.drafts,
      ...this.drafts,
    };
    this.quickReplies = this.quickReplies.length > 0 ? this.quickReplies : demo.quickReplies;
    this.globalUsage = demo.globalUsage;
    this.currentContactId = null;

    document.getElementById('password-overlay')?.classList.add('hidden');
    document.getElementById('qr-overlay')?.classList.add('hidden');
    document.getElementById('connecting-overlay')?.classList.add('hidden');
    document.getElementById('main-container')?.classList.remove('hidden');
    document.getElementById('no-chat-selected')?.classList.remove('hidden');
    document.getElementById('chat-view')?.classList.add('hidden');
    document.getElementById('main-container')?.classList.remove('chat-open');

    const userName = document.getElementById('user-name');
    const userInitial = document.getElementById('user-initial');
    const userPhone = document.getElementById('user-phone');
    if (userName) userName.textContent = 'Demo Visitor';
    if (userInitial) userInitial.textContent = 'D';
    if (userPhone) userPhone.textContent = 'Sample inbox';

    const statusIndicator = document.getElementById('status-indicator');
    const statusText = statusIndicator?.querySelector('span:last-child');
    statusIndicator?.querySelector('.status-dot')?.classList.add('connected');
    if (statusText) statusText.textContent = 'Demo mode';

    this.persistDrafts();
    this.saveStoredArray(this.quickRepliesStorageKey, this.quickReplies);
    this.syncInboxControls();
    this.renderContacts();
    this.updateGlobalUsageDisplay();
    this.updateDraftBanner();
    this.renderQuickReplies();
    this.renderVisitorDashboard();
    this.updateWorkspaceUI();
  }

  getCommandPaletteActions() {
    const actions = [
      {
        id: 'action-dashboard',
        kind: 'action',
        title: 'Go to Today dashboard',
        subtitle: 'Return to the visitor overview and action queue',
        keywords: 'today dashboard home overview',
        run: () => {
          this.closeCommandPalette();
          this.closeChat();
        },
      },
      {
        id: 'action-inbox-search',
        kind: 'action',
        title: 'Search inbox',
        subtitle: 'Focus the left-side inbox search box',
        keywords: 'search inbox chats filters',
        run: () => {
          this.closeCommandPalette();
          document.getElementById('inbox-search-input')?.focus();
          document.getElementById('inbox-search-input')?.select();
        },
      },
      {
        id: 'action-needs-reply',
        kind: 'action',
        title: 'Show chats that need a reply',
        subtitle: 'Apply the reply queue inbox preset',
        keywords: 'reply queue unread pending',
        run: () => {
          this.closeCommandPalette();
          this.applyInboxPreset('needsReply');
          this.closeChat();
        },
      },
      {
        id: 'action-reminders',
        kind: 'action',
        title: 'Show due reminders',
        subtitle: 'Filter the inbox to follow-ups that are due now',
        keywords: 'reminders due follow-up',
        run: () => {
          this.closeCommandPalette();
          this.applyInboxPreset('dueReminders');
          this.closeChat();
        },
      },
      {
        id: 'action-clear-filters',
        kind: 'action',
        title: 'Clear inbox filters',
        subtitle: 'Reset search and show every visible conversation again',
        keywords: 'clear reset filters inbox',
        run: () => {
          this.closeCommandPalette();
          this.clearInboxFilters();
        },
      },
      {
        id: 'action-appearance',
        kind: 'action',
        title: 'Open appearance settings',
        subtitle: 'Change theme and light/dark mode',
        keywords: 'appearance theme dark light',
        run: () => {
          this.closeCommandPalette();
          this.openAppearanceModal();
        },
      },
    ];

    if (this.currentContactId) {
      actions.unshift(
        {
          id: 'action-workspace',
          kind: 'action',
          title: this.workspaceExpanded ? 'Collapse workspace panel' : 'Expand workspace panel',
          subtitle: 'Show or hide the conversation context card',
          keywords: 'workspace panel context notes checklist',
          run: () => {
            this.closeCommandPalette();
            this.toggleWorkspacePanel();
          },
        },
        {
          id: 'action-chat-search',
          kind: 'action',
          title: 'Search current conversation',
          subtitle: 'Open in-chat search and focus it immediately',
          keywords: 'search messages current chat',
          run: () => {
            this.closeCommandPalette();
            if (document.getElementById('chat-search-bar')?.classList.contains('hidden')) {
              this.toggleChatSearch();
            } else {
              document.getElementById('chat-search-input')?.focus();
              document.getElementById('chat-search-input')?.select();
            }
          },
        },
        {
          id: 'action-copy-brief',
          kind: 'action',
          title: 'Copy current conversation brief',
          subtitle: 'Copy next action, context, tasks, draft, and recent messages',
          keywords: 'copy brief handoff summary context',
          run: () => {
            this.closeCommandPalette();
            this.copyConversationBrief();
          },
        },
        {
          id: 'action-translate-all',
          kind: 'action',
          title: 'Translate incoming messages',
          subtitle: 'Translate every untranslated incoming message in this chat',
          keywords: 'translate incoming unread messages batch',
          run: () => {
            this.closeCommandPalette();
            this.translateUntranslatedIncomingMessages();
          },
        },
        {
          id: 'action-chat-settings',
          kind: 'action',
          title: 'Edit current conversation settings',
          subtitle: 'Open alias, notes, labels, reminder, and translation settings',
          keywords: 'settings edit notes labels reminder translation',
          run: () => {
            this.closeCommandPalette();
            this.openSettingsModal();
          },
        },
      );
    }

    return actions;
  }

  getCommandPaletteItems(query = '') {
    const normalizedQuery = String(query || '').trim().toLowerCase();
    const tokens = normalizedQuery.split(/\s+/).filter(Boolean);
    const matchesTokens = (value) => {
      if (!tokens.length) return true;
      const haystack = String(value || '').toLowerCase();
      return tokens.every(token => haystack.includes(token));
    };

    const actions = this.getCommandPaletteActions()
      .filter(action => matchesTokens(`${action.title} ${action.subtitle} ${action.keywords}`))
      .map(action => ({ ...action }));

    const contacts = (this.contacts || [])
      .map((contact) => {
        const snapshot = this.getContactActionSnapshot(contact.id);
        const searchText = [
          this.getContactDisplayName(contact),
          contact.phone,
          snapshot.summary,
          snapshot.headline,
          this.getContactNotePreview(contact.id),
          snapshot.labels.join(' '),
        ]
          .filter(Boolean)
          .join(' ');
        const matches = matchesTokens(searchText);
        if (!matches) return null;

        let rank = snapshot.attentionScore + Math.min(20, Math.max(0, Number(contact.unreadCount || 0) * 2));
        if (!tokens.length) {
          rank += 8;
        } else {
          const displayName = this.getContactDisplayName(contact).toLowerCase();
          if (tokens.some(token => displayName.startsWith(token))) {
            rank += 20;
          }
        }

        return {
          id: `contact-${contact.id}`,
          kind: 'contact',
          contactId: contact.id,
          title: this.getContactDisplayName(contact),
          subtitle: snapshot.summary || snapshot.headline,
          rank,
          snapshot,
          run: () => {
            this.closeCommandPalette();
            this.selectContact(contact.id);
          },
        };
      })
      .filter(Boolean)
      .sort((a, b) => {
        if (b.rank !== a.rank) return b.rank - a.rank;
        return (b.snapshot?.lastMessageTime || 0) - (a.snapshot?.lastMessageTime || 0);
      })
      .slice(0, tokens.length ? 8 : 6);

    return [...actions.slice(0, tokens.length ? 6 : 4), ...contacts];
  }

  renderCommandPalette() {
    const container = document.getElementById('command-palette-results');
    const input = document.getElementById('command-palette-input');
    if (!container || !input) return;

    input.value = this.commandPaletteQuery;
    this.commandPaletteItems = this.getCommandPaletteItems(this.commandPaletteQuery);
    if (this.commandPaletteItems.length === 0) {
      this.commandPaletteSelectionIndex = 0;
      container.innerHTML = `
        <div class="command-palette-empty">
          <strong>No matching chats or actions.</strong>
          <span>Try a contact name, “reply”, “theme”, or “reminder”.</span>
        </div>
      `;
      return;
    }

    this.commandPaletteSelectionIndex = Math.min(
      this.commandPaletteSelectionIndex,
      this.commandPaletteItems.length - 1,
    );

    container.innerHTML = this.commandPaletteItems.map((item, index) => {
      const snapshot = item.snapshot;
      const badge = item.kind === 'contact' && snapshot?.headline
        ? `<span class="command-palette-item-badge">${this.escapeHtml(snapshot.headline)}</span>`
        : `<span class="command-palette-item-badge action">Action</span>`;
      const meta = item.kind === 'contact'
        ? `<span class="command-palette-item-meta">${this.escapeHtml(snapshot?.lastMessageTime ? this.formatTime(snapshot.lastMessageTime) : 'No messages yet')}</span>`
        : '<span class="command-palette-item-meta">Shortcut</span>';

      return `
        <button
          class="command-palette-item ${index === this.commandPaletteSelectionIndex ? 'active' : ''}"
          type="button"
          data-command-index="${index}"
        >
          <div class="command-palette-item-copy">
            <span class="command-palette-item-title">${this.escapeHtml(item.title)}</span>
            <span class="command-palette-item-subtitle">${this.escapeHtml(item.subtitle || '')}</span>
          </div>
          <div class="command-palette-item-meta-group">
            ${badge}
            ${meta}
          </div>
        </button>
      `;
    }).join('');

    container.querySelector('.command-palette-item.active')?.scrollIntoView({ block: 'nearest' });
  }

  openCommandPalette(initialQuery = '') {
    const palette = document.getElementById('command-palette');
    const input = document.getElementById('command-palette-input');
    if (!palette || !input) return;

    this.commandPaletteOpen = true;
    this.commandPaletteQuery = initialQuery;
    this.commandPaletteSelectionIndex = 0;
    palette.classList.remove('hidden');
    this.renderCommandPalette();
    input.focus();
    input.select();
  }

  closeCommandPalette() {
    this.commandPaletteOpen = false;
    this.commandPaletteQuery = '';
    this.commandPaletteSelectionIndex = 0;
    this.commandPaletteItems = [];
    document.getElementById('command-palette')?.classList.add('hidden');
  }

  moveCommandPaletteSelection(direction = 1) {
    if (!this.commandPaletteOpen || this.commandPaletteItems.length === 0) return;
    const maxIndex = this.commandPaletteItems.length - 1;
    this.commandPaletteSelectionIndex = this.commandPaletteSelectionIndex + direction;
    if (this.commandPaletteSelectionIndex < 0) {
      this.commandPaletteSelectionIndex = maxIndex;
    } else if (this.commandPaletteSelectionIndex > maxIndex) {
      this.commandPaletteSelectionIndex = 0;
    }
    this.renderCommandPalette();
  }

  activateCommandPaletteSelection(index = this.commandPaletteSelectionIndex) {
    const item = this.commandPaletteItems[index];
    if (!item) return;
    item.run();
  }

  toggleWorkspacePanel(force) {
    this.workspaceExpanded = typeof force === 'boolean' ? force : !this.workspaceExpanded;
    this.updateWorkspaceUI();
  }

  updateWorkspaceUI() {
    const workspace = document.getElementById('conversation-workspace');
    const toggleButton = document.getElementById('chat-workspace-toggle');
    const backdrop = document.getElementById('workspace-backdrop');
    const chatView = document.getElementById('chat-view');
    const showWorkspaceOverlay = this.isMobile() && this.workspaceExpanded && Boolean(this.currentContactId);
    if (workspace) {
      workspace.classList.toggle('collapsed', !this.workspaceExpanded);
    }
    if (toggleButton) {
      toggleButton.classList.toggle('active', this.workspaceExpanded);
      toggleButton.setAttribute('aria-expanded', String(this.workspaceExpanded));
      toggleButton.title = this.workspaceExpanded
        ? (this.isMobile() ? 'Hide conversation context' : 'Hide workspace')
        : (this.isMobile() ? 'Show conversation context' : 'Show workspace');
      toggleButton.setAttribute(
        'aria-label',
        this.workspaceExpanded
          ? (this.isMobile() ? 'Hide conversation context' : 'Hide workspace')
          : (this.isMobile() ? 'Show conversation context' : 'Show workspace')
      );
    }
    if (backdrop) {
      backdrop.classList.toggle('hidden', !showWorkspaceOverlay);
    }
    if (chatView) {
      chatView.classList.toggle('workspace-open', showWorkspaceOverlay);
    }
    this.renderConversationWorkspace();
  }

  renderConversationWorkspace() {
    const summaryEl = document.getElementById('conversation-workspace-summary');
    const statusEl = document.getElementById('workspace-primary-status');
    const notesEl = document.getElementById('workspace-notes');
    const checklistEl = document.getElementById('workspace-checklist');
    const checklistSummaryEl = document.getElementById('workspace-checklist-summary');
    const labelsEl = document.getElementById('workspace-labels');
    const reminderEl = document.getElementById('workspace-reminder');
    const timezoneEl = document.getElementById('workspace-timezone');
    const briefEl = document.getElementById('workspace-brief');
    const actionPlanEl = document.getElementById('workspace-action-plan');
    const copyBriefButton = document.getElementById('workspace-copy-brief');
    const translateAllButton = document.getElementById('workspace-translate-all');
    const workspace = document.getElementById('conversation-workspace');
    if (!summaryEl || !statusEl || !notesEl || !checklistEl || !labelsEl || !reminderEl || !timezoneEl || !briefEl || !actionPlanEl || !workspace) {
      return;
    }

    if (!this.currentContactId) {
      summaryEl.innerHTML = '';
      statusEl.textContent = 'No local context yet.';
      notesEl.textContent = 'No notes yet.';
      checklistEl.innerHTML = '';
      checklistSummaryEl.textContent = 'No tasks';
      labelsEl.innerHTML = '';
      reminderEl.textContent = 'No reminder set.';
      timezoneEl.textContent = '';
      briefEl.innerHTML = '';
      actionPlanEl.innerHTML = '';
      if (copyBriefButton) copyBriefButton.disabled = true;
      if (translateAllButton) translateAllButton.disabled = true;
      return;
    }

    const contact = this.contacts.find(entry => entry.id === this.currentContactId) || { id: this.currentContactId };
    const snapshot = this.getContactActionSnapshot(this.currentContactId);
    const metadata = this.getContactMetadata(this.currentContactId);
    const checklist = Array.isArray(metadata.checklist) ? metadata.checklist : [];
    const labels = snapshot.labels;
    const notes = String(metadata.notes || '').trim();
    const brief = buildConversationBrief({
      contact,
      metadata,
      messages: this.messages.get(this.currentContactId) || [],
      drafts: this.drafts,
    });
    const actionPlan = buildConversationActionPlan({
      contact,
      metadata,
      messages: this.messages.get(this.currentContactId) || [],
      drafts: this.drafts,
    });

    summaryEl.innerHTML = this.buildContactBadgeMarkup(this.currentContactId) || '<span class="workspace-empty-chip">No visitor context yet</span>';
    statusEl.textContent = snapshot.summary || 'Add notes, labels, reminders, or tasks to keep this conversation organized.';
    briefEl.innerHTML = `
      <div class="workspace-brief-primary">${this.escapeHtml(brief.nextAction)}</div>
      ${brief.contextLines.length > 0 ? `
        <div class="workspace-brief-lines">
          ${brief.contextLines.map(line => `<span>${this.escapeHtml(line)}</span>`).join('')}
        </div>
      ` : ''}
    `;
    actionPlanEl.innerHTML = `
      <span class="workspace-section-label">Action plan</span>
      <div class="workspace-action-list">
        ${actionPlan.actions.map(action => `
          <div class="workspace-action-item ${this.escapeHtml(action.priority || 'normal')}">
            <span class="workspace-action-title">${this.escapeHtml(action.label)}</span>
            <span class="workspace-action-detail">${this.escapeHtml(action.detail)}</span>
          </div>
        `).join('')}
      </div>
    `;

    if (copyBriefButton) {
      copyBriefButton.disabled = false;
      copyBriefButton.textContent = 'Copy brief';
    }
    if (translateAllButton) {
      translateAllButton.disabled = actionPlan.untranslatedCount === 0;
      translateAllButton.textContent = actionPlan.untranslatedCount === 0
        ? 'Translated'
        : `Translate ${actionPlan.untranslatedCount}`;
    }

    notesEl.textContent = notes || 'No notes yet. Add private context in conversation settings so you can keep tone, follow-ups, and reminders visible while replying.';
    notesEl.classList.toggle('empty', !notes);

    checklistSummaryEl.textContent = snapshot.checklist.label;
    if (checklist.length === 0) {
      checklistEl.innerHTML = '<span class="workspace-empty-chip">No checklist items yet</span>';
    } else {
      checklistEl.innerHTML = checklist.map((item) => `
        <button
          class="workspace-checklist-item ${item.done ? 'done' : ''}"
          type="button"
          data-workspace-checklist="${item.id}"
        >
          <span class="workspace-checklist-mark">${item.done ? '✓' : ''}</span>
          <span class="workspace-checklist-text">${this.escapeHtml(item.text)}</span>
        </button>
      `).join('');
    }

    if (labels.length === 0) {
      labelsEl.innerHTML = '<span class="workspace-empty-chip">No labels yet</span>';
    } else {
      labelsEl.innerHTML = labels.map(label => `<span class="workspace-label-chip">${this.escapeHtml(label)}</span>`).join('');
    }

    reminderEl.textContent = this.getReminderSummary(this.currentContactId) || 'No reminder set.';
    reminderEl.classList.toggle('due', snapshot.reminderStatus === 'due');
    timezoneEl.textContent = snapshot.timezoneInfo
      ? `${snapshot.timezoneInfo.localTime} · ${snapshot.timezoneInfo.statusLabel}`
      : '';

    workspace.classList.toggle('has-reminder-due', snapshot.reminderStatus === 'due');
    workspace.classList.toggle('collapsed', !this.workspaceExpanded);
  }

  toggleWorkspaceChecklistItem(itemId) {
    if (!this.currentContactId || !itemId) return;

    const metadata = this.getContactMetadata(this.currentContactId);
    const checklist = toggleChecklistItem(metadata.checklist || [], itemId);
    this.updateContactMetadata(this.currentContactId, { checklist });

    const contact = this.contacts.find(entry => entry.id === this.currentContactId);
    if (contact) {
      contact.checklist = checklist;
    }

    this.renderContacts();
    this.updateChatHeaderNote();
    this.renderConversationWorkspace();
  }

  // Keep the app height aligned with iOS Safari's real usable screen area.
  // We only apply a JS viewport override while the keyboard is open. When the
  // keyboard is closed we fall back to CSS large-viewport sizing so the app can
  // occupy the full screen instead of stopping above Safari's bottom chrome.
  setupVisualViewport() {
    const root = document.documentElement;
    let frame = null;

    const updateViewportHeight = () => {
      if (frame !== null) {
        cancelAnimationFrame(frame);
      }

      frame = requestAnimationFrame(() => {
        const viewport = window.visualViewport;
        const layout = calculateViewportLayout({
          innerHeight: window.innerHeight,
          viewportHeight: viewport?.height,
          viewportOffsetTop: viewport?.offsetTop,
        });

        if (layout.effectiveHeight === null) {
          root.style.removeProperty('--viewport-height');
        } else {
          root.style.setProperty('--viewport-height', `${layout.effectiveHeight}px`);
        }
        root.style.setProperty('--keyboard-offset', `${layout.keyboardOffset}px`);
        frame = null;
      });
    };

    updateViewportHeight();

    if (window.visualViewport) {
      window.visualViewport.addEventListener('resize', updateViewportHeight);
      window.visualViewport.addEventListener('scroll', updateViewportHeight);
    }

    window.addEventListener('resize', updateViewportHeight);
    window.addEventListener('orientationchange', updateViewportHeight);
    window.addEventListener('pageshow', updateViewportHeight);
  }

  async checkAuth() {
    try {
      const response = await fetch('/api/auth/check');
      const data = await response.json();
      return data.required;
    } catch (err) {
      console.error('Failed to check auth:', err);
      return false;
    }
  }

  async verifyToken() {
    try {
      // Try to make an authenticated request to verify token is valid
      const response = await this.apiFetch('/api/status', {
        headers: this.getAuthHeaders()
      });
      return response.ok;
    } catch (err) {
      return false;
    }
  }

  showPasswordOverlay() {
    document.getElementById('password-overlay')?.classList.remove('hidden');
    document.getElementById('connecting-overlay')?.classList.add('hidden');
    document.getElementById('password-input')?.focus();
  }

  bindPasswordEvents() {
    const form = document.getElementById('password-form');
    const input = document.getElementById('password-input');
    const error = document.getElementById('password-error');

    form?.addEventListener('submit', (event) => {
      event.preventDefault();
      this.handleLogin();
    });

    input?.addEventListener('input', () => {
      error?.classList.add('hidden');
    });
  }

  async handleLogin() {
    const input = document.getElementById('password-input');
    const submit = document.getElementById('password-submit');
    const error = document.getElementById('password-error');
    
    const password = input?.value;
    if (!password) return;

    submit.disabled = true;
    error?.classList.add('hidden');

    try {
      const response = await fetch('/api/auth', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ password })
      });

      const result = await response.json();

      if (result.success) {
        // Store token
        if (result.token) {
          this.authToken = result.token;
          localStorage.setItem('wa_auth_token', result.token);
        }
        
        // Clear password input
        if (input) input.value = '';
        
        // Start the app
        this.startApp();
      } else {
        error?.classList.remove('hidden');
        input?.focus();
        input?.select();
      }
    } catch (err) {
      console.error('Login failed:', err);
      error?.classList.remove('hidden');
    } finally {
      submit.disabled = false;
    }
  }

  // Get auth headers for API requests
  getAuthHeaders(baseHeaders = {}) {
    const headers = { ...baseHeaders };
    if (this.authToken) {
      headers['Authorization'] = `Bearer ${this.authToken}`;
    }
    return headers;
  }

  apiFetch(url, options = {}) {
    return fetch(url, {
      ...options,
      headers: this.getAuthHeaders(options.headers || {})
    });
  }

  // Handle logout
  async handleLogout() {
    const confirmed = confirm(
      'Are you sure you want to logout?\n\n' +
      'This will:\n' +
      '- Disconnect from WhatsApp\n' +
      '- Delete all messages and contacts\n' +
      '- Clear the session (require new QR scan)\n\n' +
      'This action cannot be undone.'
    );

    if (!confirmed) return;

    try {
      const response = await this.apiFetch('/api/logout', {
        method: 'POST',
        headers: this.getAuthHeaders()
      });

      const result = await response.json();

      if (result.success) {
        // Clear local storage
        localStorage.removeItem('wa_auth_token');
        this.authToken = null;

        // Clear local data
        this.contacts = [];
        this.messages.clear();
        this.avatarCache.clear();
        this.currentContactId = null;

        // Clear the UI
        document.getElementById('contacts-list').innerHTML = `
          <div class="empty-state">
            <p>No conversations yet</p>
            <p class="hint">Messages will appear here</p>
          </div>
        `;

        // Show connecting overlay - bridge will restart and send new QR
        this.showConnecting();
        
        // The WebSocket will receive the new QR code when bridge restarts
      } else {
        alert('Logout failed: ' + (result.error || 'Unknown error'));
      }
    } catch (err) {
      console.error('Logout failed:', err);
      alert('Logout failed: ' + err.message);
    }
  }

  // Update placeholder to show correct keyboard shortcut for OS
  updateInputPlaceholder() {
    const input = document.getElementById('message-input');
    if (input) {
      const isMac = navigator.platform.toUpperCase().indexOf('MAC') >= 0;
      const shortcut = isMac ? '⌘+Enter' : 'Ctrl+Enter';
      input.placeholder = this.isMobile()
        ? 'Type a message'
        : `Type a message (${shortcut} to send)`;
    }
  }

  handleResponsiveLayoutChange() {
    const isMobile = this.isMobile();

    if (isMobile !== this.mobileLayoutActive) {
      this.mobileLayoutActive = isMobile;
      this.workspaceExpanded = !isMobile;
    }

    if (!isMobile) {
      document.getElementById('workspace-backdrop')?.classList.add('hidden');
    }

    this.updateInputPlaceholder();
    this.updateWorkspaceUI();
  }

  // WebSocket connection
  connectWebSocket() {
    if (this.demoMode) return;

    const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
    const tokenQuery = this.authToken ? `?token=${encodeURIComponent(this.authToken)}` : '';
    const wsUrl = `${protocol}//${window.location.host}/ws${tokenQuery}`;
    
    this.ws = new WebSocket(wsUrl);
    
    this.ws.onopen = () => {
      console.log('WebSocket connected');
    };
    
    this.ws.onmessage = (event) => {
      const data = JSON.parse(event.data);
      this.handleMessage(data);
    };
    
    this.ws.onclose = () => {
      if (this.demoMode) return;
      console.log('WebSocket disconnected, reconnecting...');
      setTimeout(() => this.connectWebSocket(), 3000);
    };
    
    this.ws.onerror = (err) => {
      console.error('WebSocket error:', err);
    };
  }

  // Handle incoming WebSocket messages
  handleMessage(data) {
    switch (data.type) {
      case 'status':
        this.handleStatus(data);
        break;
      
      case 'qr':
        this.showQRCode(data.data);
        break;
      
      case 'connected':
        this.handleConnected(data);
        break;
      
      case 'disconnected':
        this.handleDisconnected();
        break;
      
      case 'message':
        this.handleNewMessage(data.message);
        break;
      
      case 'typing':
        this.handleTyping(data);
        break;
      
      case 'mark_as_read':
        this.handleMarkAsRead(data.chat_id);
        break;
      
      case 'error':
        console.error('Error:', data.error);
        break;
    }
  }

  // Handle mark-as-read event from another device
  handleMarkAsRead(chatId) {
    const contact = this.contacts.find(c => c.id === chatId);
    if (contact) {
      contact.unreadCount = 0;
      this.scheduleRenderContacts();
    }
  }

  // Toggle pin status for a contact
  async togglePin(contactId) {
    const contact = this.contacts.find(c => c.id === contactId);
    if (!contact) return;

    const nextPinnedAt = contact.pinnedAt != null ? null : Date.now();
    contact.pinnedAt = nextPinnedAt;
    this.updateContactMetadata(contactId, { pinnedAt: nextPinnedAt });
    this.scheduleRenderContacts();
    if (contactId === this.currentContactId) {
      this.updateChatHeaderNote();
      this.renderConversationWorkspace();
    }
  }

  // Handle status update
  handleStatus(data) {
    if (data.connected) {
      this.handleConnected(data);
    } else {
      // Show connecting overlay
      this.showConnecting();
    }
  }

  // Show connecting overlay
  showConnecting() {
    if (this.demoMode) return;

    document.getElementById('qr-overlay').classList.add('hidden');
    document.getElementById('connecting-overlay').classList.remove('hidden');
    document.getElementById('main-container').classList.add('hidden');
  }

  // Show QR code
  showQRCode(qrData) {
    document.getElementById('connecting-overlay').classList.add('hidden');
    document.getElementById('qr-overlay').classList.remove('hidden');
    document.getElementById('main-container').classList.add('hidden');
    
    // Generate QR code
    const container = document.getElementById('qr-container');
    container.innerHTML = '';
    
    // Use QRCode library if available, otherwise show text
    if (typeof QRCode !== 'undefined') {
      new QRCode(container, {
        text: qrData,
        width: 264,
        height: 264,
        colorDark: '#000000',
        colorLight: '#ffffff',
        correctLevel: QRCode.CorrectLevel.L
      });
    } else {
      // Fallback: create canvas QR code manually using simple approach
      this.renderQRCode(container, qrData);
    }
  }

  // Fallback QR code renderer - displays text if library not available
  renderQRCode(container, data) {
    // If QRCode library is available, use it
    if (typeof QRCode !== 'undefined') {
      container.innerHTML = '';
      new QRCode(container, {
        text: data,
        width: 264,
        height: 264,
        colorDark: '#000000',
        colorLight: '#ffffff',
        correctLevel: QRCode.CorrectLevel.L
      });
    } else {
      // Fallback: show the QR data as text for manual entry
      container.innerHTML = `<div style="word-break: break-all; font-size: 10px; max-width: 264px;">${data}</div>`;
    }
  }

  // Handle connected state
  handleConnected(data) {
    this.connected = true;
    this.notificationsReadyAt = Date.now() + 10000;
    
    document.getElementById('qr-overlay').classList.add('hidden');
    document.getElementById('connecting-overlay').classList.add('hidden');
    document.getElementById('main-container').classList.remove('hidden');
    
    // Update user info
    if (data.name) {
      document.getElementById('user-name').textContent = data.name;
      document.getElementById('user-initial').textContent = data.name.charAt(0).toUpperCase();
    }
    if (data.phone) {
      document.getElementById('user-phone').textContent = '+' + data.phone;
    }
    
    // Update status indicator
    const statusDot = document.querySelector('.status-dot');
    statusDot.classList.add('connected');
    
    // Load contacts
    this.loadContacts();
    
    // Load global usage
    this.fetchGlobalUsage();
  }

  // Handle disconnected state
  handleDisconnected() {
    if (this.demoMode) return;

    this.connected = false;
    
    const statusDot = document.querySelector('.status-dot');
    statusDot.classList.remove('connected');
    
    this.showConnecting();
  }

  // Handle new message
  handleNewMessage(message) {
    if (!message.senderJid) {
      message.senderJid = this.getMessageSenderJid(message);
    }
    if (!message.replyContext && message.content?.reply_context) {
      message.replyContext = message.content.reply_context;
    }

    // Check if this is a reaction message
    if (message.content && message.content.type === 'reaction') {
      this.handleReactionMessage(message);
      return;
    }
    
    // Skip protocol and unknown messages - these shouldn't be displayed
    if (message.content && (message.content.type === 'protocol' || message.content.type === 'unknown')) {
      console.log('Skipping non-displayable message type:', message.content.type);
      return;
    }
    
    // Add to local cache
    if (!this.messages.has(message.contactId)) {
      this.messages.set(message.contactId, []);
    }
    
    const messages = this.messages.get(message.contactId);
    if (!messages.some(m => m.id === message.id)) {
      messages.push(message);
      messages.sort((a, b) => a.timestamp - b.timestamp);
    }

    this.trimCachedMessages(message.contactId, true);
    
    // Update contact in list
    this.updateContactInList(message);

    this.maybeShowNotification(message);
    
    // If this contact is currently selected, show the message
    if (this.currentContactId === message.contactId) {
      if (!message.isFromMe && !message.is_from_me) {
        this.markConversationRead(message.contactId, message);
      }
      this.refreshCurrentConversationView();
      this.scrollToBottom();
      this.updateChatHeaderNote();
      this.renderConversationWorkspace();
    }
    
    // Refresh usage stats if this was a translated message
    if (message.isTranslated || message.is_translated) {
      this.fetchGlobalUsage();
      if (this.currentContactId === message.contactId) {
        this.fetchConversationUsage(message.contactId);
      }
    }
    
    // Clear typing indicator when message arrives from that user
    if (!message.isFromMe && !message.is_from_me) {
      this.clearTypingState(message.contactId);
    }
  }

  notificationsSupported() {
    return (
      typeof window !== 'undefined' &&
      'Notification' in window &&
      window.isSecureContext
    );
  }

  updateNotificationPrompt() {
    const host = document.getElementById('contacts-panel');
    if (!host) return;

    if (!this.notificationPromptEl) {
      this.notificationPromptEl = document.createElement('div');
      this.notificationPromptEl.id = 'notification-prompt';
      this.notificationPromptEl.className = 'notification-prompt hidden';
      host.insertBefore(this.notificationPromptEl, document.getElementById('contacts-list'));
    }

    if (!this.notificationsSupported()) {
      this.notificationPromptEl.innerHTML = `
        <span>Browser notifications are not available in this context.</span>
      `;
      this.notificationPromptEl.classList.remove('hidden');
      return;
    }

    if (Notification.permission === 'default') {
      this.notificationPromptEl.innerHTML = `
        <span>Enable browser notifications for new translated messages.</span>
        <button id="notification-enable-button" type="button">Enable</button>
      `;
      this.notificationPromptEl.classList.remove('hidden');
      this.notificationPromptEl
        .querySelector('#notification-enable-button')
        ?.addEventListener('click', () => this.requestNotificationPermission());
      return;
    }

    if (Notification.permission === 'denied') {
      this.notificationPromptEl.innerHTML = `
        <span>Notifications are blocked in your browser settings.</span>
      `;
      this.notificationPromptEl.classList.remove('hidden');
      return;
    }

    this.notificationPromptEl.classList.add('hidden');
  }

  async requestNotificationPermission() {
    if (!this.notificationsSupported()) {
      this.updateNotificationPrompt();
      return;
    }

    this.notificationPermissionRequested = true;

    try {
      await Notification.requestPermission();
    } catch (err) {
      console.error('Failed to request notification permission:', err);
    } finally {
      this.updateNotificationPrompt();
    }
  }

  setupNotificationPermissionRequest() {
    if (!this.notificationsSupported()) return;
    if (Notification.permission !== 'default') return;
    if (this.notificationPermissionRequested) return;

    const requestPermission = async () => {
      if (this.notificationPermissionRequested) return;
      this.notificationPermissionRequested = true;

      try {
        await Notification.requestPermission();
      } catch (err) {
        console.error('Failed to request notification permission:', err);
      } finally {
        this.updateNotificationPrompt();
        window.removeEventListener('click', requestPermission, true);
        window.removeEventListener('keydown', requestPermission, true);
        window.removeEventListener('touchstart', requestPermission, true);
      }
    };

    window.addEventListener('click', requestPermission, true);
    window.addEventListener('keydown', requestPermission, true);
    window.addEventListener('touchstart', requestPermission, true);
  }

  shouldNotifyForMessage(message) {
    if (!this.notificationsSupported()) return false;
    if (Notification.permission !== 'granted') return false;
    if (Date.now() < this.notificationsReadyAt) return false;
    if (message.isFromMe || message.is_from_me) return false;

    const contentType = message.content?.type || message.contentType || message.content_type;
    if (!contentType || ['reaction', 'protocol', 'unknown'].includes(contentType)) {
      return false;
    }

    const isCurrentThreadVisible =
      this.currentContactId === message.contactId &&
      document.visibilityState === 'visible' &&
      document.hasFocus();

    return !isCurrentThreadVisible;
  }

  getThreadDisplayName(message) {
    const contact = this.contacts.find(c => c.id === message.contactId);
    return (
      contact?.name ||
      message.contactName ||
      contact?.phone ||
      message.contactPhone ||
      message.senderName ||
      message.senderPhone ||
      'WhatsApp'
    );
  }

  getNotificationText(message) {
    const translated = message.translatedText || message.translated_text;
    if (translated) {
      return translated;
    }

    const content = message.content || {};
    switch (content.type) {
      case 'text':
        return content.body || content.text || '';
      case 'image':
        return content.caption || '[ Image ]';
      case 'video':
        return content.caption || '[ Video ]';
      case 'audio':
        return content.isVoiceNote || content.is_voice_note ? '[ Voice note ]' : '[ Audio ]';
      case 'document':
        return content.fileName || content.file_name || '[ Document ]';
      case 'sticker':
        return '[ Sticker ]';
      case 'location':
        return '[ Location ]';
      case 'contact':
        return content.name || '[ Contact ]';
      case 'poll':
        return content.question || '[ Poll ]';
      default:
        return this.getMessagePreview(message).replace(/^You:\s*/, '');
    }
  }

  maybeShowNotification(message) {
    if (!this.shouldNotifyForMessage(message)) {
      return;
    }

    const title = this.getThreadDisplayName(message);
    let body = this.getNotificationText(message);

    const isGroup = (message.chatType || message.chat_type) === 'group';
    const sender = message.senderName || message.sender_name || message.senderPhone || message.sender_phone;
    if (isGroup && sender) {
      body = `${sender}: ${body}`;
    }

    if (!body) {
      body = 'New message';
    }

    const notification = new Notification(title, {
      body,
      icon: '/icon-192.png',
      badge: '/icon-192.png',
      tag: `chat:${message.contactId}`,
      renotify: false,
    });

    notification.onclick = () => {
      window.focus();
      if (message.contactId) {
        this.selectContact(message.contactId);
      }
      notification.close();
    };
  }

  // Handle incoming reaction message
  handleReactionMessage(reactionMsg) {
    const contactId = reactionMsg.contactId;
    const targetMessageId = reactionMsg.content.target_message_id || reactionMsg.content.targetMessageId;
    const emoji = reactionMsg.content.emoji || '';
    const senderPhone = reactionMsg.senderPhone || reactionMsg.sender_phone || 'unknown';
    const isFromMe = reactionMsg.isFromMe || reactionMsg.is_from_me;
    
    if (!targetMessageId) {
      console.warn('Reaction message missing target_message_id');
      return;
    }
    
    // Find the target message in our cache
    const messages = this.messages.get(contactId);
    if (!messages) return;
    
    const targetMessage = messages.find(m => m.id === targetMessageId);
    if (!targetMessage) {
      console.log('Target message not found for reaction:', targetMessageId);
      return;
    }
    
    // Initialize reactions if needed
    if (!targetMessage.reactions) {
      targetMessage.reactions = {};
    }
    
    // Determine reactor identifier
    const reactor = isFromMe ? (document.getElementById('user-phone')?.textContent?.replace('+', '') || 'me') : senderPhone;
    
    // Remove previous reaction from this sender
    for (const [existingEmoji, reactors] of Object.entries(targetMessage.reactions)) {
      targetMessage.reactions[existingEmoji] = reactors.filter(r => r !== reactor);
      if (targetMessage.reactions[existingEmoji].length === 0) {
        delete targetMessage.reactions[existingEmoji];
      }
    }
    
    // Add new reaction (empty emoji means removal)
    if (emoji) {
      if (!targetMessage.reactions[emoji]) {
        targetMessage.reactions[emoji] = [];
      }
      if (!targetMessage.reactions[emoji].includes(reactor)) {
        targetMessage.reactions[emoji].push(reactor);
      }
    }
    
    // Update display if viewing this chat
    if (this.currentContactId === contactId) {
      this.updateMessageReactions(targetMessageId);
    }
    
    console.log('Reaction updated:', { targetMessageId, emoji, reactor });
  }

  // Handle typing indicator
  handleTyping(data) {
    const { chat_id, user_id, state } = data;
    console.log('Typing event received:', { chat_id, user_id, state, currentContactId: this.currentContactId });
    
    // Clear existing timeout for this chat
    if (this.typingTimeouts.has(chat_id)) {
      clearTimeout(this.typingTimeouts.get(chat_id));
      this.typingTimeouts.delete(chat_id);
    }
    
    if (state === 'paused') {
      // Remove typing state
      this.typingState.delete(chat_id);
    } else {
      // Set typing or recording state
      this.typingState.set(chat_id, {
        userId: user_id,
        state: state, // 'typing' or 'recording'
        timestamp: Date.now()
      });
      
      // Auto-clear after 10 seconds (in case paused event is missed)
      const timeoutId = setTimeout(() => {
        this.clearTypingState(chat_id);
      }, 10000);
      this.typingTimeouts.set(chat_id, timeoutId);
    }
    
    // Update UI if this is the current chat
    if (this.currentContactId === chat_id) {
      this.updateTypingIndicator();
    }
    
    // Also update the contact list preview
    this.updateContactTypingPreview(chat_id);
  }

  // Clear typing state for a chat
  clearTypingState(chatId) {
    this.typingState.delete(chatId);
    if (this.typingTimeouts.has(chatId)) {
      clearTimeout(this.typingTimeouts.get(chatId));
      this.typingTimeouts.delete(chatId);
    }
    
    if (this.currentContactId === chatId) {
      this.updateTypingIndicator();
    }
    this.updateContactTypingPreview(chatId);
  }

  // Update typing indicator in chat header
  updateTypingIndicator() {
    const indicatorEl = document.getElementById('typing-indicator');
    console.log('updateTypingIndicator called, element:', indicatorEl, 'currentContactId:', this.currentContactId);
    if (!indicatorEl) {
      console.warn('typing-indicator element not found!');
      return;
    }
    
    const typingInfo = this.typingState.get(this.currentContactId);
    console.log('typingInfo for current contact:', typingInfo);
    
    if (typingInfo) {
      const text = typingInfo.state === 'recording' ? 'recording audio...' : 'typing...';
      indicatorEl.textContent = text;
      indicatorEl.classList.remove('hidden');
      console.log('Showing typing indicator:', text);
    } else {
      indicatorEl.classList.add('hidden');
      console.log('Hiding typing indicator');
    }
  }

  // Update contact list to show typing preview
  updateContactTypingPreview(chatId) {
    const contactItem = document.querySelector(`.contact-item[data-contact-id="${chatId}"]`);
    if (!contactItem) return;
    
    const previewEl = contactItem.querySelector('.preview-text');
    if (!previewEl) return;
    
    const typingInfo = this.typingState.get(chatId);
    
    if (typingInfo) {
      const text = typingInfo.state === 'recording' ? 'recording audio...' : 'typing...';
      previewEl.innerHTML = `<span class="typing-preview">${text}</span>`;
    } else {
      // Restore the original preview
      const contact = this.contacts.find(c => c.id === chatId);
      if (contact) {
        const messages = this.messages.get(chatId) || [];
        const lastMessage = messages[messages.length - 1];
        const preview = lastMessage ? this.getMessagePreview(lastMessage) : '';
        previewEl.textContent = preview;
      }
    }
  }

  // Update contact in the list
  updateContactInList(message) {
    // Find or create contact
    let contact = this.contacts.find(c => c.id === message.contactId);
    
    if (!contact) {
      // Use contactName (the chat/contact name) not senderName
      contact = {
        id: message.contactId,
        name: message.contactName || message.contactPhone,
        phone: message.contactPhone,
        type: message.chatType,
        lastMessageTime: message.timestamp,
        unreadCount: 0
      };
      this.contacts.push(contact);
    } else {
      contact.lastMessageTime = Math.max(contact.lastMessageTime, message.timestamp);
      // Only update name if we have a better one from contactName (not senderName!)
      // contactName is the stable chat name (other person for DMs, group name for groups)
      if (message.contactName && message.contactName !== message.contactPhone && !contact.name) {
        contact.name = message.contactName;
      }
    }
    
    // Increment unread if not from me and not currently viewing
    if (!message.isFromMe && this.currentContactId !== message.contactId) {
      contact.unreadCount = (contact.unreadCount || 0) + 1;
    }
    
    // Batch contact list updates during reconnect/history sync.
    this.scheduleRenderContacts();
  }

  // Load contacts from server
  async loadContacts() {
    try {
      const response = await this.apiFetch('/api/contacts', {
        headers: this.getAuthHeaders()
      });
      this.contacts = (await response.json()).map(contact => this.applyStoredContactMetadata(contact));
      this.syncInboxControls();
      this.renderContacts();
    } catch (err) {
      console.error('Failed to load contacts:', err);
    }
  }

  scheduleRenderContacts() {
    if (this.contactsRenderTimer) return;

    this.contactsRenderTimer = setTimeout(() => {
      this.contactsRenderTimer = null;
      this.renderContacts();
    }, 50);
  }

  // Fetch avatar for a contact
  async fetchAvatar(jid) {
    if (this.demoMode) return;

    // Skip if already cached or being fetched
    if (this.avatarCache.has(jid) || this.avatarFetching.has(jid)) {
      return;
    }

    this.avatarFetching.add(jid);

    try {
      const response = await this.apiFetch(`/api/avatar/${encodeURIComponent(jid)}`, {
        headers: this.getAuthHeaders()
      });
      const data = await response.json();
      
      if (data.url) {
        this.avatarCache.set(jid, data.url);
        // Update any visible avatars for this contact
        this.updateAvatarDisplay(jid, data.url);
      }
    } catch (err) {
      console.error('Failed to fetch avatar:', err);
    } finally {
      this.avatarFetching.delete(jid);
    }
  }

  // Update avatar display for a specific JID
  updateAvatarDisplay(jid, url) {
    const initial = this.getInitial(jid);
    
    // Update in contacts list
    const contactItem = document.querySelector(`.contact-item[data-contact-id="${jid}"] .avatar`);
    if (contactItem) {
      contactItem.innerHTML = `<img src="${url}" alt="" onerror="this.parentElement.innerHTML='<span>${initial}</span>'">`;
    }

    // Update in chat header if this is the current contact
    if (this.currentContactId === jid) {
      const chatAvatar = document.querySelector('.chat-header .avatar');
      if (chatAvatar) {
        chatAvatar.innerHTML = `<img src="${url}" alt="" onerror="this.parentElement.innerHTML='<span>${initial}</span>'">`;
      }
    }
  }

  // Get initial for a contact by JID
  getInitial(jid) {
    const contact = this.contacts.find(c => c.id === jid);
    return (contact?.name || contact?.phone || '?').charAt(0).toUpperCase();
  }

  // Render contacts list
  renderContacts() {
    const container = document.getElementById('contacts-list');
    this.renderVisitorDashboard();
    
    if (this.contacts.length === 0) {
      container.innerHTML = `
        <div class="empty-state">
          <p>No conversations yet</p>
          <p class="hint">Messages will appear here</p>
        </div>
      `;
      return;
    }

    this.syncInboxControls();
    const sorted = this.getFilteredContacts();
    if (sorted.length === 0) {
      container.innerHTML = `
        <div class="empty-state">
          <p>No conversations match this inbox view</p>
          <p class="hint">Try a different search or clear a few filters.</p>
          <button id="inbox-reset-empty" class="sidebar-reset-button">Reset inbox filters</button>
        </div>
      `;
      document.getElementById('inbox-reset-empty')?.addEventListener('click', () => this.clearInboxFilters());
      return;
    }

    container.innerHTML = sorted.map(contact => {
      const isGroup = contact.type === 'group';
      const isPinned = contact.pinnedAt != null;
      const reminderStatus = getReminderStatus(this.getContactMetadata(contact.id));
      const isSnoozed = isContactSnoozed(this.getContactMetadata(contact.id));
      const priority = this.getPriorityInfo(contact.id);
      const checklist = this.getChecklistSummary(contact.id);
      const timezoneInfo = this.getTimezoneInfo(contact.id);
      const displayName = this.getContactDisplayName(contact);
      const initial = (displayName || '?').charAt(0).toUpperCase();
      const time = this.formatTime(contact.lastMessageTime);
      const isActive = contact.id === this.currentContactId;
      const unread = contact.unreadCount > 0 ? 
        `<span class="unread-badge">${contact.unreadCount}</span>` : '';
      const badgeMarkup = this.buildContactBadgeMarkup(contact.id);
      
      // Get last message preview - prefer saved drafts, then cached messages, then backend preview
      const messages = this.messages.get(contact.id) || [];
      const lastMessage = messages[messages.length - 1];
      const draftPreview = getDraftPreview(this.drafts, contact.id);
      const snoozeSummary = this.getSnoozeSummary(contact.id);
      const reminderSummary = this.getReminderSummary(contact.id);
      let preview = '';
      let previewClass = draftPreview ? 'preview-text draft-preview' : 'preview-text';
      
      if (draftPreview) {
        preview = draftPreview;
      } else if (reminderStatus === 'due' && reminderSummary) {
        preview = reminderSummary;
        previewClass = 'preview-text reminder-preview';
      } else if (isSnoozed && snoozeSummary) {
        preview = snoozeSummary;
        previewClass = 'preview-text snooze-preview';
      } else if (lastMessage) {
        // Use locally cached message for preview
        preview = this.getMessagePreview(lastMessage);
        // For groups, prefix with sender name if not from me
        if (isGroup && !lastMessage.isFromMe && lastMessage.senderName) {
          const senderFirst = lastMessage.senderName.split(' ')[0];
          if (!preview.startsWith('You: ')) {
            preview = `${senderFirst}: ${preview}`;
          }
        }
      } else if (contact.lastMessagePreview) {
        // Fall back to server-provided preview (used before messages are loaded)
        preview = contact.lastMessagePreview;
      } else if (checklist.open > 0) {
        preview = checklist.label;
        previewClass = 'preview-text checklist-preview';
      } else if (timezoneInfo) {
        preview = `${timezoneInfo.label} · ${timezoneInfo.statusLabel}`;
        previewClass = 'preview-text timezone-preview';
      }
      
      // Check for cached avatar
      const avatarUrl = this.avatarCache.get(contact.id);
      const avatarContent = avatarUrl 
        ? `<img src="${avatarUrl}" alt="" onerror="this.parentElement.innerHTML='<span>${initial}</span>'">`
        : `<span>${initial}</span>`;
      
      // Group indicator (fold mark in corner)
      const groupIndicator = isGroup ? '<div class="group-indicator"></div>' : '';
      
      // Pin button (shows on hover, filled when pinned)
      const pinButton = `
        <button class="pin-button ${isPinned ? 'pinned' : ''}" 
                onclick="event.stopPropagation(); app.togglePin('${contact.id}')" 
                title="${isPinned ? 'Unpin' : 'Pin'}">
          <svg viewBox="0 0 24 24" width="14" height="14">
            <path fill="currentColor" d="M16 12V4h1V2H7v2h1v8l-2 2v2h5.2v6h1.6v-6H18v-2l-2-2z"/>
          </svg>
        </button>
      `;
      
      return `
        <div class="contact-item ${isActive ? 'active' : ''} ${isGroup ? 'is-group' : ''} ${isPinned ? 'is-pinned' : ''} ${priority.isImportant ? 'is-important' : ''} ${reminderStatus === 'due' ? 'has-due-reminder' : ''}" data-contact-id="${contact.id}">
          <div class="avatar-container">
            <div class="avatar">
              ${avatarContent}
              ${groupIndicator}
            </div>
            ${pinButton}
          </div>
          <div class="contact-details">
            <div class="contact-header">
              <div class="contact-title">
                <span class="contact-name">${this.escapeHtml(displayName)}</span>
                ${badgeMarkup}
              </div>
              <span class="contact-time">${time}</span>
            </div>
            <div class="contact-preview">
              <span class="${previewClass}">${this.escapeHtml(preview)}</span>
              ${unread}
            </div>
          </div>
        </div>
      `;
    }).join('');

    // Fetch avatars lazily for just the visible contact rows after the list renders.
    const visibleContacts = Array.from(container.querySelectorAll('.contact-item[data-contact-id]'))
      .slice(0, 24)
      .map(el => el.dataset.contactId)
      .filter(Boolean);

    visibleContacts.forEach(contactId => this.fetchAvatar(contactId));
    this.scheduleMetadataRefresh();
  }

  // Get message preview text
  getMessagePreview(message) {
    const prefix = message.isFromMe ? 'You: ' : '';
    const content = message.content;
    
    switch (content.type) {
      case 'text':
        return prefix + (content.body || content.text || '').substring(0, 50);
      case 'image':
        return prefix + '[ Image ]' + (content.caption ? ' ' + content.caption.substring(0, 30) : '');
      case 'video':
        return prefix + '[ Video ]' + (content.caption ? ' ' + content.caption.substring(0, 30) : '');
      case 'audio':
        return prefix + (content.isVoiceNote ? '[ Voice Note ]' : '[ Audio ]');
      case 'document':
        return prefix + '[ Document: ' + (content.fileName || 'file') + ' ]';
      case 'sticker':
        return prefix + '[ Sticker ]';
      case 'location':
        return prefix + '[ Location ]';
      case 'contact':
        return prefix + '[ Contact: ' + content.name + ' ]';
      case 'reaction':
        return prefix + content.emoji;
      case 'revoked':
        return '[ Message deleted ]';
      case 'poll':
        return prefix + '[ Poll: ' + content.question + ' ]';
      default:
        return prefix + '[ Message ]';
    }
  }

  getMessageSenderJid(message) {
    const isFromMe = message.isFromMe || message.is_from_me;

    if (isFromMe) {
      const myPhone = document.getElementById('user-phone')?.textContent?.replace('+', '') || '';
      return myPhone ? `${myPhone}@s.whatsapp.net` : '';
    }

    const senderJid = message.senderJid || message.sender_jid;
    if (senderJid) return senderJid;

    const senderPhone = message.senderPhone || message.sender_phone || '';
    if (!senderPhone) return '';
    return senderPhone.includes('@') ? senderPhone : `${senderPhone}@s.whatsapp.net`;
  }

  getLatestIncomingMessage(contactId) {
    const messages = this.messages.get(contactId) || [];
    for (let i = messages.length - 1; i >= 0; i -= 1) {
      const message = messages[i];
      if (!message.isFromMe && !message.is_from_me) {
        return message;
      }
    }
    return null;
  }

  async markConversationRead(contactId, message = null) {
    if (!contactId) return;
    if (this.demoMode) return;

    try {
      const targetMessage = message || this.getLatestIncomingMessage(contactId);
      const body = targetMessage ? {
        messageId: targetMessage.id,
        timestamp: Math.floor((targetMessage.timestamp || 0) / 1000),
        senderJid: targetMessage.senderJid || this.getMessageSenderJid(targetMessage) || null
      } : {};

      await this.apiFetch(`/api/contacts/${encodeURIComponent(contactId)}/read`, {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
          ...this.getAuthHeaders()
        },
        body: JSON.stringify(body)
      });
    } catch (err) {
      console.error('Failed to mark conversation as read:', err);
    }
  }

  // Select a contact
  async selectContact(contactId) {
    try {
      this.closeCommandPalette();
      this.currentContactId = contactId;
      this.messageSearchQuery = '';
      this.starredOnly = false;
      
      // Clear any pending reply from previous chat
      this.clearReply();
      
      // Mark as read
      const contact = this.contacts.find(c => c.id === contactId);
      if (contact) {
        contact.unreadCount = 0;
      }
      this.markConversationRead(contactId);
      
      // Update UI
      document.getElementById('no-chat-selected').classList.add('hidden');
      document.getElementById('chat-view').classList.remove('hidden');
      
      // Add chat-open class for mobile view
      document.getElementById('main-container').classList.add('chat-open');
      
      // Push history state for mobile back button
      if (this.isMobile()) {
        history.pushState({ chat: contactId }, '', `?chat=${encodeURIComponent(contactId)}`);
      }
      
      // Update chat header
      if (contact) {
        const displayName = this.getContactDisplayName(contact);
        document.getElementById('chat-name').textContent = displayName;
        
        const initial = (displayName || '?').charAt(0).toUpperCase();
        // Get avatar container - it's the .avatar element in .chat-header
        const avatarContainer = document.querySelector('.chat-header .avatar');
        const avatarUrl = this.avatarCache.get(contactId);
        
        if (avatarContainer) {
          if (avatarUrl) {
            avatarContainer.innerHTML = `<img src="${avatarUrl}" alt="" onerror="this.parentElement.innerHTML='<span>${initial}</span>'">`;
          } else {
            avatarContainer.innerHTML = `<span>${initial}</span>`;
            // Fetch avatar if not cached
            this.fetchAvatar(contactId);
          }
        }
      }

      const cachedUsage = this.conversationUsageCache.get(contactId);
      this.updateConversationUsageDisplay(cachedUsage || { costUsd: 0 });

      const cachedMessages = this.messages.get(contactId);
      if (cachedMessages && cachedMessages.length > 0) {
        const visibleMessages = this.getRecentVisibleMessages(cachedMessages);
        if (visibleMessages.length !== cachedMessages.length) {
          this.messages.set(contactId, visibleMessages);
          this.messagesHasMore.set(contactId, true);
        }
        this.renderMessages(visibleMessages);
        this.setupScrollHandler();
      } else {
        this.showMessagesLoading();
        await this.loadMessages(contactId);
      }

      this.markConversationRead(contactId);

      this.scheduleConversationUsageFetch(contactId);
      
      // Re-render contacts to update active state and unread
      this.scheduleRenderContacts();
      this.restoreDraftForCurrentContact();
      this.updateDraftBanner();
      this.renderQuickReplies();
      this.closeConversationMenu();
      document.getElementById('chat-search-bar')?.classList.add('hidden');
      const chatSearchInput = document.getElementById('chat-search-input');
      if (chatSearchInput) chatSearchInput.value = '';
      this.updateChatSearchUI();
      this.updateStarredToggleUI();
      this.updateChatHeaderNote();
      this.updateWorkspaceUI();
      this.renderComposerAssist();
      
      // Update send button state and focus input (only on desktop)
      this.updateSendButton();
      if (!this.isMobile()) {
        document.getElementById('message-input').focus();
      }
    } catch (err) {
      console.error('Error selecting contact:', err);
    }
  }

  showMessagesLoading() {
    const container = document.getElementById('messages-list');
    if (!container) return;

    container.innerHTML = `
      <div class="messages-loading-state">
        <div class="loading-spinner"></div>
        <span>Loading messages...</span>
      </div>
    `;
  }

  getRecentVisibleMessages(messages) {
    if (!messages || messages.length <= this.initialMessageLimit) {
      return messages || [];
    }

    return messages.slice(-this.initialMessageLimit);
  }

  trimCachedMessages(contactId, preserveVisible = false) {
    const messages = this.messages.get(contactId);
    if (!messages || messages.length <= this.messageCacheLimit) {
      return;
    }

    if (preserveVisible && this.currentContactId === contactId) {
      return;
    }

    this.messages.set(contactId, messages.slice(-this.messageCacheLimit));
    this.messagesHasMore.set(contactId, true);
  }

  scheduleDeferredWork(callback, delay = 80) {
    const run = () => window.setTimeout(callback, delay);

    if (typeof window.requestIdleCallback === 'function') {
      window.requestIdleCallback(callback, { timeout: delay + 200 });
      return;
    }

    if (typeof window.requestAnimationFrame === 'function') {
      window.requestAnimationFrame(run);
      return;
    }

    window.setTimeout(callback, delay);
  }

  scheduleConversationUsageFetch(contactId) {
    if (this.conversationUsageTimer) {
      clearTimeout(this.conversationUsageTimer);
      this.conversationUsageTimer = null;
    }

    this.conversationUsageTimer = window.setTimeout(() => {
      this.conversationUsageTimer = null;
      this.scheduleDeferredWork(() => {
        if (contactId === this.currentContactId) {
          this.fetchConversationUsage(contactId);
        }
      }, 40);
    }, 0);
  }

  scheduleLinkPreviewLoad(root = document) {
    if (this.linkPreviewLoadTimer) {
      clearTimeout(this.linkPreviewLoadTimer);
      this.linkPreviewLoadTimer = null;
    }

    const activeContactId = this.currentContactId;
    this.linkPreviewLoadTimer = window.setTimeout(() => {
      this.linkPreviewLoadTimer = null;
      this.scheduleDeferredWork(() => {
        if (activeContactId !== this.currentContactId) {
          return;
        }

        const scope = root && root.isConnected ? root : document;
        this.loadVisibleLinkPreviews(scope);
      });
    }, 0);
  }

  // Load messages for a contact (initial load - most recent page)
  async loadMessages(contactId) {
    try {
      this.messagesLoading.set(contactId, true);
      if (this.demoMode) {
        this.messagesHasMore.set(contactId, false);
        this.fullyLoadedContacts.add(contactId);
        if (contactId === this.currentContactId) {
          this.renderMessages(this.getVisibleMessagesForCurrentConversation());
        }
        return;
      }

      const response = await this.apiFetch(`/api/messages/${encodeURIComponent(contactId)}?limit=${this.initialMessageLimit}`, {
        headers: this.getAuthHeaders()
      });
      const data = await response.json();
      
      // Handle both old format (array) and new format (object with messages/hasMore)
      const messages = Array.isArray(data) ? data : data.messages;
      const hasMore = Array.isArray(data) ? false : data.hasMore;

      messages.forEach(message => {
        if (!message.senderJid) {
          message.senderJid = this.getMessageSenderJid(message);
        }
        if (!message.replyContext && message.content?.reply_context) {
          message.replyContext = message.content.reply_context;
        }
      });
      
      this.messages.set(contactId, messages);
      this.messagesHasMore.set(contactId, hasMore);
      if (hasMore) {
        this.fullyLoadedContacts.delete(contactId);
      } else {
        this.fullyLoadedContacts.add(contactId);
      }
      if (contactId === this.currentContactId) {
        this.renderMessages(this.getVisibleMessagesForCurrentConversation());
      }
      
      // Set up scroll handler for infinite scroll
      if (contactId === this.currentContactId) {
        this.setupScrollHandler();
      }
    } catch (err) {
      console.error('Failed to load messages:', err);
    } finally {
      this.messagesLoading.set(contactId, false);
    }
  }

  async ensureFullConversationLoaded(contactId = this.currentContactId) {
    if (!contactId || this.fullyLoadedContacts.has(contactId)) {
      return;
    }

    try {
      this.messagesLoading.set(contactId, true);
      const response = await this.apiFetch(`/api/messages/${encodeURIComponent(contactId)}?limit=0`, {
        headers: this.getAuthHeaders()
      });
      const data = await response.json();
      const messages = Array.isArray(data) ? data : data.messages;

      messages.forEach(message => {
        if (!message.senderJid) {
          message.senderJid = this.getMessageSenderJid(message);
        }
        if (!message.replyContext && message.content?.reply_context) {
          message.replyContext = message.content.reply_context;
        }
      });

      this.messages.set(contactId, messages);
      this.messagesHasMore.set(contactId, false);
      this.fullyLoadedContacts.add(contactId);
    } catch (err) {
      console.error('Failed to load the full conversation:', err);
    } finally {
      this.messagesLoading.set(contactId, false);
    }
  }

  // Load older messages (for infinite scroll)
  async loadOlderMessages(contactId) {
    // Don't load if already loading or no more messages
    if (this.messagesLoading.get(contactId)) return;
    if (!this.messagesHasMore.get(contactId)) return;
    
    const existingMessages = this.messages.get(contactId) || [];
    if (existingMessages.length === 0) return;
    
    // Get the oldest message cursor
    const oldestMessage = existingMessages[0];
    const oldestTimestamp = oldestMessage.timestamp;
    const olderParams = new URLSearchParams({
      before: String(oldestTimestamp),
      limit: '50'
    });
    if (oldestMessage.id) {
      olderParams.set('beforeId', oldestMessage.id);
    }
    
    try {
      this.messagesLoading.set(contactId, true);
      this.showLoadingIndicator();
      
      const response = await this.apiFetch(
        `/api/messages/${encodeURIComponent(contactId)}?${olderParams.toString()}`,
        {
          headers: this.getAuthHeaders()
        }
      );
      const data = await response.json();
      
      const olderMessages = Array.isArray(data) ? data : data.messages;
      const hasMore = Array.isArray(data) ? false : data.hasMore;

      olderMessages.forEach(message => {
        if (!message.senderJid) {
          message.senderJid = this.getMessageSenderJid(message);
        }
        if (!message.replyContext && message.content?.reply_context) {
          message.replyContext = message.content.reply_context;
        }
      });
      
      if (olderMessages.length > 0) {
        // Prepend older messages
        const allMessages = [...olderMessages, ...existingMessages];
        this.messages.set(contactId, allMessages);
        this.messagesHasMore.set(contactId, hasMore);
        if (!hasMore) {
          this.fullyLoadedContacts.add(contactId);
        }
        
        // Re-render and maintain scroll position
        if (this.messageSearchQuery || this.starredOnly) {
          this.refreshCurrentConversationView();
        } else {
          this.prependMessages(olderMessages);
        }
      } else {
        this.messagesHasMore.set(contactId, false);
        this.fullyLoadedContacts.add(contactId);
      }
    } catch (err) {
      console.error('Failed to load older messages:', err);
    } finally {
      this.messagesLoading.set(contactId, false);
      this.hideLoadingIndicator();
    }
  }

  // Set up scroll handler for infinite scroll
  setupScrollHandler() {
    const container = document.getElementById('messages-list');
    if (!container) return;
    
    // Remove existing handler if any
    container.removeEventListener('scroll', this.handleMessagesScroll);
    
    // Add scroll handler
    this.handleMessagesScroll = () => {
      // Load more when scrolled near the top (within 200px)
      if (container.scrollTop < 200 && this.currentContactId) {
        this.loadOlderMessages(this.currentContactId);
      }
      this.scheduleLinkPreviewLoad(container);
    };
    
    container.addEventListener('scroll', this.handleMessagesScroll);
  }

  // Show loading indicator at top of messages
  showLoadingIndicator() {
    const container = document.getElementById('messages-list');
    if (!container) return;
    
    // Remove existing indicator
    const existing = container.querySelector('.loading-more-indicator');
    if (existing) existing.remove();
    
    // Add new indicator at top
    const indicator = document.createElement('div');
    indicator.className = 'loading-more-indicator';
    indicator.innerHTML = '<div class="loading-spinner"></div><span>Loading older messages...</span>';
    container.insertBefore(indicator, container.firstChild);
  }

  // Hide loading indicator
  hideLoadingIndicator() {
    const indicator = document.querySelector('.loading-more-indicator');
    if (indicator) indicator.remove();
  }

  // Prepend older messages to the list (maintaining scroll position)
  prependMessages(olderMessages) {
    const container = document.getElementById('messages-list');
    if (!container || olderMessages.length === 0) return;
    
    // Remember scroll position from bottom
    const scrollHeightBefore = container.scrollHeight;
    const scrollTopBefore = container.scrollTop;
    
    // Build HTML for older messages
    let html = '';
    let lastDate = null;
    
    for (const message of olderMessages) {
      const messageDate = new Date(message.timestamp).toDateString();
      if (messageDate !== lastDate) {
        html += `<div class="date-separator"><span>${this.formatDate(message.timestamp)}</span></div>`;
        lastDate = messageDate;
      }
      html += this.renderMessage(message);
    }
    
    // Remove loading indicator and first date separator if it will be duplicated
    const firstDateSep = container.querySelector('.date-separator');
    if (firstDateSep && olderMessages.length > 0) {
      const lastOlderDate = new Date(olderMessages[olderMessages.length - 1].timestamp).toDateString();
      const firstExistingDate = firstDateSep.textContent;
      // Check if dates match (approximately)
      if (firstDateSep.textContent.includes(this.formatDate(olderMessages[olderMessages.length - 1].timestamp).split(',')[0])) {
        firstDateSep.remove();
      }
    }
    
    // Insert at the beginning
    container.insertAdjacentHTML('afterbegin', html);
    
    // Restore scroll position (keep viewing the same messages)
    const scrollHeightAfter = container.scrollHeight;
    container.scrollTop = scrollTopBefore + (scrollHeightAfter - scrollHeightBefore);
    
    // Load link previews after the DOM settles
    this.scheduleLinkPreviewLoad(container);
  }

  // Render messages
  renderMessages(messages) {
    const container = document.getElementById('messages-list');
    
    if (messages.length === 0) {
      const emptyMessage = this.messageSearchQuery
        ? 'No messages match your search yet'
        : (this.starredOnly ? 'No starred messages in this conversation yet' : 'No messages yet');
      container.innerHTML = `<div class="empty-state"><p>${this.escapeHtml(emptyMessage)}</p></div>`;
      this.updateChatSearchUI();
      this.renderComposerAssist();
      return;
    }
    
    let html = '';
    let lastDate = null;
    
    for (const message of messages) {
      // Add date separator if needed
      const messageDate = new Date(message.timestamp).toDateString();
      if (messageDate !== lastDate) {
        html += `<div class="date-separator"><span>${this.formatDate(message.timestamp)}</span></div>`;
        lastDate = messageDate;
      }
      
      html += this.renderMessage(message);
    }
    
    container.innerHTML = html;
    this.scrollToBottom();
    
    // Defer previews until after the first paint.
    this.scheduleLinkPreviewLoad(container);
    this.updateChatSearchUI();
    this.renderComposerAssist();
  }

  // Load link previews for messages near the viewport.
  loadVisibleLinkPreviews(root = document) {
    const containers = root.querySelectorAll('.link-previews-container[data-urls]');
    containers.forEach(container => {
      if (container.dataset.previewLoaded === 'true' || container.dataset.previewLoading === 'true') {
        return;
      }

      const rect = container.getBoundingClientRect();
      const viewportHeight = window.innerHeight || document.documentElement.clientHeight || 0;
      const isNearViewport = rect.bottom >= -300 && rect.top <= viewportHeight + 600;
      if (!isNearViewport) {
        return;
      }

      try {
        const urls = JSON.parse(container.dataset.urls);
        if (urls && urls.length > 0) {
          const messageEl = container.closest('.message');
          if (messageEl) {
            this.loadLinkPreviews(messageEl, urls);
          }
        }
      } catch (e) {
        console.error('Failed to parse URLs:', e);
      }
    });
  }

  // Render a single message
  renderMessage(message) {
    const isOutgoing = message.isFromMe || message.is_from_me;
    const isTranslated = message.isTranslated || message.is_translated;
    const time = this.formatMessageTime(message.timestamp);
    const content = this.renderContent(message);
    
    let forwarded = '';
    if (message.isForwarded || message.is_forwarded) {
      forwarded = '<div class="message-forwarded">Forwarded</div>';
    }
    
    let sender = '';
    if (!isOutgoing && (message.chatType === 'group' || message.chat_type === 'group')) {
      sender = `<div class="message-sender">${this.escapeHtml(message.senderName || message.sender_name || message.senderPhone || message.sender_phone)}</div>`;
    }

    // Translation indicator
    let translationIndicator = '';
    if (isTranslated) {
      const sourceLanguage = message.sourceLanguage || message.source_language || 'Unknown';
      
      // Tooltip shows the "other" version:
      // - Outgoing: show translated_text (what was sent to them in foreign language)
      // - Incoming: show original_text (what they sent in foreign language)
      let tooltipText, tooltipHeader, languageLabel;
      
      if (isOutgoing) {
        // Outgoing: show what was sent (translated foreign text)
        tooltipText = message.translatedText || message.translated_text || '';
        tooltipHeader = 'Sent as';
        languageLabel = sourceLanguage;
      } else {
        // Incoming: show original (foreign text they sent)
        tooltipText = message.originalText || message.original_text || '';
        tooltipHeader = 'Original message';
        languageLabel = sourceLanguage;
      }
      
      translationIndicator = `
        <span class="translation-indicator" onclick="event.stopPropagation(); this.classList.toggle('show-tooltip');">
          <span class="info-icon">i</span>
          <span>Translated</span>
          <div class="original-tooltip">
            <button class="tooltip-close" onclick="event.stopPropagation(); this.closest('.translation-indicator').classList.remove('show-tooltip');">&times;</button>
            <div class="tooltip-header">${tooltipHeader} (${this.escapeHtml(languageLabel)})</div>
            <div class="tooltip-text">${this.escapeHtml(tooltipText)}</div>
          </div>
        </span>
      `;
    }
    
    // Get message metadata for reactions
    const messageId = message.id;
    const senderJid = this.getMessageSenderJid(message);
    const contactId = message.contactId || message.contact_id || this.currentContactId;
    const starred = isMessageStarred(this.starredMessages, messageId);
    
    // Check if message can be translated (incoming, has text, not already translated)
    const hasText = message.content && (message.content.body || message.content.caption || message.content.text);
    const canTranslate = !isOutgoing && hasText && !isTranslated;
    
    // Translate button (for untranslated incoming messages)
    const translateButton = `
      <button class="message-action-btn translate-button ${canTranslate ? 'can-translate' : ''}" 
              onclick="event.stopPropagation(); app.translateMessage('${messageId}')" 
              title="${isTranslated ? 'Already translated' : (canTranslate ? 'Translate' : 'No text to translate')}">
        <svg viewBox="0 0 24 24"><path fill="currentColor" d="M12.87 15.07l-2.54-2.51.03-.03c1.74-1.94 2.98-4.17 3.71-6.53H17V4h-7V2H8v2H1v1.99h11.17C11.5 7.92 10.44 9.75 9 11.35 8.07 10.32 7.3 9.19 6.69 8h-2c.73 1.63 1.73 3.17 2.98 4.56l-5.09 5.02L4 19l5-5 3.11 3.11.76-2.04zM18.5 10h-2L12 22h2l1.12-3h4.75L21 22h2l-4.5-12zm-2.62 7l1.62-4.33L19.12 17h-3.24z"/></svg>
      </button>
    `;
    
    // Reply button
    const replyButton = `
      <button class="message-action-btn" onclick="event.stopPropagation(); app.handleReplyClick('${messageId}')" title="Reply">
        <svg viewBox="0 0 24 24"><path fill="currentColor" d="M10 9V5l-7 7 7 7v-4.1c5 0 8.5 1.6 11 5.1-1-5-4-10-11-11z"/></svg>
      </button>
    `;
    
    // AI Reply button (only for incoming messages with text content)
    const canAIReply = !isOutgoing && hasText;
    const aiReplyButton = canAIReply ? `
      <button class="message-action-btn ai-reply-btn" 
              onclick="event.stopPropagation(); app.generateAIReply('${messageId}')" 
              title="Generate AI reply that sounds like you">
        <svg viewBox="0 0 24 24" width="18" height="18">
          <path fill="currentColor" d="M21 11.5a8.38 8.38 0 0 1-.9 3.8 8.5 8.5 0 0 1-7.6 4.7 8.38 8.38 0 0 1-3.8-.9L3 21l1.9-5.7a8.38 8.38 0 0 1-.9-3.8 8.5 8.5 0 0 1 4.7-7.6 8.38 8.38 0 0 1 3.8-.9h.5a8.48 8.48 0 0 1 8 8v.5z"/>
        </svg>
        <span class="ai-sparkle">✨</span>
      </button>
    ` : '';
    
    // Reaction button with quick emoji picker
    const reactionButton = `
      <div class="reaction-button-container">
        <button class="message-action-btn" onclick="event.stopPropagation(); this.parentElement.querySelector('.reaction-picker').classList.toggle('show');" title="React">
          <svg viewBox="0 0 24 24"><path fill="currentColor" d="M12 2C6.48 2 2 6.48 2 12s4.48 10 10 10 10-4.48 10-10S17.52 2 12 2zm0 18c-4.41 0-8-3.59-8-8s3.59-8 8-8 8 3.59 8 8-3.59 8-8 8zm-5-6c.78 2.34 2.72 4 5 4s4.22-1.66 5-4H7zm2-3c.55 0 1-.45 1-1s-.45-1-1-1-1 .45-1 1 .45 1 1 1zm6 0c.55 0 1-.45 1-1s-.45-1-1-1-1 .45-1 1 .45 1 1 1z"/></svg>
        </button>
        <div class="reaction-picker">
          <span class="reaction-emoji" onclick="app.sendReaction('${messageId}', '${contactId}', '${senderJid}', '👍')">👍</span>
          <span class="reaction-emoji" onclick="app.sendReaction('${messageId}', '${contactId}', '${senderJid}', '❤️')">❤️</span>
          <span class="reaction-emoji" onclick="app.sendReaction('${messageId}', '${contactId}', '${senderJid}', '😂')">😂</span>
          <span class="reaction-emoji" onclick="app.sendReaction('${messageId}', '${contactId}', '${senderJid}', '😮')">😮</span>
          <span class="reaction-emoji" onclick="app.sendReaction('${messageId}', '${contactId}', '${senderJid}', '😢')">😢</span>
          <span class="reaction-emoji" onclick="app.sendReaction('${messageId}', '${contactId}', '${senderJid}', '🙏')">🙏</span>
        </div>
      </div>
    `;

    const starButton = `
      <button class="message-action-btn star-toggle ${starred ? 'active' : ''}"
              onclick="event.stopPropagation(); app.toggleMessageStar('${messageId}')"
              title="${starred ? 'Remove star' : 'Star message'}">
        <svg viewBox="0 0 24 24"><path fill="currentColor" d="m12 17.27 6.18 3.73-1.64-7.03L22 9.24l-7.19-.61L12 2 9.19 8.63 2 9.24l5.46 4.73L5.82 21z"/></svg>
      </button>
    `;
    
    // Quoted message (reply context)
    let quotedMessage = '';
    if (message.replyContext) {
      const ctx = message.replyContext;
      quotedMessage = `
        <div class="quoted-message">
          <div class="quoted-sender">${this.escapeHtml(ctx.senderName || 'Unknown')}</div>
          <div class="quoted-text">${this.escapeHtml(ctx.text || '')}</div>
        </div>
      `;
    }
    
    // Reactions display
    const reactionsHtml = this.renderReactions(message.reactions);
    const starredBadge = starred ? '<span class="message-starred-badge" title="Starred">★</span>' : '';
    
    return `
      <div class="message ${isOutgoing ? 'outgoing' : 'incoming'}" data-message-id="${messageId}">
        ${forwarded}
        ${sender}
        ${quotedMessage}
        ${content}
        ${reactionsHtml}
        <div class="message-footer">
          <span class="message-time">${time}</span>
          ${starredBadge}
          ${translationIndicator}
          <div class="message-actions">
            ${translateButton}
            ${replyButton}
            ${aiReplyButton}
            ${starButton}
            ${reactionButton}
          </div>
        </div>
      </div>
    `;
  }

  // Render message content
  renderContent(message) {
    const content = message.content;
    const isTranslated = message.is_translated || message.isTranslated;
    const isFromMe = message.isFromMe || message.is_from_me;
    
    // Display logic - always show MY language (English) in the bubble:
    // - Outgoing translated: show original_text (English - what I typed)
    // - Outgoing non-translated: show content.body (English - what I typed)
    // - Incoming translated: show translated_text (English translation of what they sent)
    // - Incoming non-translated: show content.body/caption (already English)
    let displayText;
    let displayCaption;
    
    if (isTranslated && isFromMe) {
      // Outgoing translated: show original_text (English - what I typed)
      displayText = message.original_text || message.originalText || content.body || content.text || '';
      displayCaption = content.caption ? (message.original_text || message.originalText || content.caption) : null;
    } else if (isTranslated && !isFromMe) {
      // Incoming translated: show the English translation
      // translated_text contains the translated body or caption
      const translatedContent = message.translated_text || message.translatedText || '';
      displayText = translatedContent || content.body || content.text || '';
      // For media with captions, the translated_text IS the translated caption
      displayCaption = content.caption ? translatedContent : null;
    } else {
      // Non-translated: show content.body/caption as-is
      displayText = content.body || content.text || '';
      displayCaption = content.caption || null;
    }
    
    // Extract URLs for link previews
    const urls = this.extractUrls(displayText);
    const hasUrls = urls.length > 0;
    
    switch (content.type) {
      case 'text':
        // Use single quotes for data-urls attribute since JSON contains double quotes
        const urlsJson = hasUrls ? JSON.stringify(urls).replace(/'/g, '&#39;') : '';
        return `
          <div class="message-text">${this.linkifyText(displayText)}</div>
          ${hasUrls ? `<div class="link-previews-container" data-urls='${urlsJson}'></div>` : ''}
        `;
      
      case 'image':
        // Check if we have the actual image data or if it needs to be lazy loaded
        const mediaData = content.media_data || content.mediaData;
        const hasMedia = content.has_media || content.hasMedia;
        const messageId = message.id;
        const mimeType = content.mime_type || content.mimeType || 'image/jpeg';
        
        if (mediaData) {
          const imgSrc = mediaData.startsWith('data:') ? mediaData : `data:${mimeType};base64,${mediaData}`;
          return `
            <div class="message-image">
              <img src="${imgSrc}" alt="Image" loading="lazy" onclick="this.classList.toggle('fullscreen')">
            </div>
            ${displayCaption ? `<div class="message-caption">${this.escapeHtml(displayCaption)}</div>` : ''}
          `;
        } else if (hasMedia) {
          // Media needs to be lazy loaded - show placeholder
          return `
            <div class="message-image lazy-media" data-message-id="${messageId}" data-mime-type="${mimeType}" data-media-type="image">
              <div class="media-placeholder" onclick="app.loadMedia('${messageId}', this)">
                <svg viewBox="0 0 24 24" width="48" height="48">
                  <path fill="currentColor" d="M21 19V5c0-1.1-.9-2-2-2H5c-1.1 0-2 .9-2 2v14c0 1.1.9 2 2 2h14c1.1 0 2-.9 2-2zM8.5 13.5l2.5 3.01L14.5 12l4.5 6H5l3.5-4.5z"/>
                </svg>
                <span>Click to load image</span>
              </div>
            </div>
            ${displayCaption ? `<div class="message-caption">${this.escapeHtml(displayCaption)}</div>` : ''}
          `;
        } else {
          // No media available
          return `
            <div class="message-media image">[ Image ]${content.file_size ? ' - ' + this.formatSize(content.file_size) : ''}</div>
            ${displayCaption ? `<div class="message-caption">${this.escapeHtml(displayCaption)}</div>` : ''}
          `;
        }
      
      case 'video':
        const videoData = content.media_data || content.mediaData;
        const videoHasMedia = content.has_media || content.hasMedia;
        const videoMsgId = message.id;
        const videoMime = content.mime_type || content.mimeType || 'video/mp4';
        
        if (videoData) {
          const videoSrc = videoData.startsWith('data:') ? videoData : `data:${videoMime};base64,${videoData}`;
          return `
            <div class="message-video">
              <video controls preload="metadata" onclick="event.stopPropagation()">
                <source src="${videoSrc}" type="${videoMime}">
                Your browser does not support video playback.
              </video>
            </div>
            ${displayCaption ? `<div class="message-caption">${this.escapeHtml(displayCaption)}</div>` : ''}
          `;
        } else if (videoHasMedia) {
          // Media needs to be lazy loaded - show placeholder
          const durationText = content.duration_seconds ? this.formatDuration(content.duration_seconds) : '';
          return `
            <div class="message-video lazy-media" data-message-id="${videoMsgId}" data-mime-type="${videoMime}" data-media-type="video">
              <div class="media-placeholder" onclick="app.loadMedia('${videoMsgId}', this)">
                <svg viewBox="0 0 24 24" width="48" height="48">
                  <path fill="currentColor" d="M17 10.5V7c0-.55-.45-1-1-1H4c-.55 0-1 .45-1 1v10c0 .55.45 1 1 1h12c.55 0 1-.45 1-1v-3.5l4 4v-11l-4 4z"/>
                </svg>
                <span>Click to load video${durationText ? ` (${durationText})` : ''}</span>
              </div>
            </div>
            ${displayCaption ? `<div class="message-caption">${this.escapeHtml(displayCaption)}</div>` : ''}
          `;
        } else {
          return `
            <div class="message-media video">[ Video ]${content.duration_seconds ? ' - ' + this.formatDuration(content.duration_seconds) : ''}${content.file_size ? ' - ' + this.formatSize(content.file_size) : ''}</div>
            ${displayCaption ? `<div class="message-caption">${this.escapeHtml(displayCaption)}</div>` : ''}
          `;
        }
      
      case 'audio':
        const audioData = content.media_data || content.mediaData;
        const audioHasMedia = content.has_media || content.hasMedia;
        const audioMsgId = message.id;
        const audioMime = content.mime_type || content.mimeType || 'audio/ogg';
        const isVoiceNote = content.is_voice_note || content.isVoiceNote;
        
        if (audioData) {
          const audioSrc = audioData.startsWith('data:') ? audioData : `data:${audioMime};base64,${audioData}`;
          return `
            <div class="message-audio ${isVoiceNote ? 'voice-note' : ''}">
              <audio controls preload="metadata">
                <source src="${audioSrc}" type="${audioMime}">
                Your browser does not support audio playback.
              </audio>
              ${isVoiceNote ? '<span class="voice-note-label">Voice Note</span>' : ''}
            </div>
          `;
        } else if (audioHasMedia) {
          // Media needs to be lazy loaded - show placeholder
          const audioType = isVoiceNote ? 'voice note' : 'audio';
          const durationText = content.duration_seconds ? this.formatDuration(content.duration_seconds) : '';
          return `
            <div class="message-audio lazy-media ${isVoiceNote ? 'voice-note' : ''}" data-message-id="${audioMsgId}" data-mime-type="${audioMime}" data-media-type="audio">
              <div class="media-placeholder" onclick="app.loadMedia('${audioMsgId}', this)">
                <svg viewBox="0 0 24 24" width="32" height="32">
                  <path fill="currentColor" d="M12 3v9.28c-.47-.17-.97-.28-1.5-.28C8.01 12 6 14.01 6 16.5S8.01 21 10.5 21c2.31 0 4.2-1.75 4.45-4H15V6h4V3h-7z"/>
                </svg>
                <span>Click to load ${audioType}${durationText ? ` (${durationText})` : ''}</span>
              </div>
            </div>
          `;
        } else {
          const audioType = isVoiceNote ? 'Voice Note' : 'Audio';
          return `<div class="message-media audio">[ ${audioType} ]${content.duration_seconds ? ' - ' + this.formatDuration(content.duration_seconds) : ''}</div>`;
        }
      
      case 'document':
        const docData = content.media_data || content.mediaData;
        const docHasMedia = content.has_media || content.hasMedia;
        const docMsgId = message.id;
        const fileName = content.file_name || content.fileName || 'document';
        const docMime = content.mime_type || content.mimeType || 'application/octet-stream';
        
        if (docData) {
          const docSrc = docData.startsWith('data:') ? docData : `data:${docMime};base64,${docData}`;
          return `
            <div class="message-document">
              <a href="${docSrc}" download="${this.escapeHtml(fileName)}" class="document-download">
                <svg viewBox="0 0 24 24" width="24" height="24">
                  <path fill="currentColor" d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8l-6-6zm4 18H6V4h7v5h5v11z"/>
                </svg>
                <div class="document-info">
                  <span class="document-name">${this.escapeHtml(fileName)}</span>
                  <span class="document-size">${content.file_size ? this.formatSize(content.file_size) : ''}</span>
                </div>
              </a>
            </div>
            ${displayCaption ? `<div class="message-caption">${this.escapeHtml(displayCaption)}</div>` : ''}
          `;
        } else if (docHasMedia) {
          // Media needs to be lazy loaded - show placeholder
          const sizeText = content.file_size ? this.formatSize(content.file_size) : '';
          return `
            <div class="message-document lazy-media" data-message-id="${docMsgId}" data-mime-type="${docMime}" data-media-type="document" data-file-name="${this.escapeHtml(fileName)}">
              <div class="media-placeholder document-placeholder" onclick="app.loadMedia('${docMsgId}', this)">
                <svg viewBox="0 0 24 24" width="24" height="24">
                  <path fill="currentColor" d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8l-6-6zm4 18H6V4h7v5h5v11z"/>
                </svg>
                <div class="document-info">
                  <span class="document-name">${this.escapeHtml(fileName)}</span>
                  <span class="document-size">${sizeText ? sizeText : 'Click to download'}</span>
                </div>
              </div>
            </div>
            ${displayCaption ? `<div class="message-caption">${this.escapeHtml(displayCaption)}</div>` : ''}
          `;
        } else {
          return `
            <div class="message-media document">[ Document: ${this.escapeHtml(fileName)} ]${content.file_size ? ' - ' + this.formatSize(content.file_size) : ''}</div>
            ${displayCaption ? `<div class="message-caption">${this.escapeHtml(displayCaption)}</div>` : ''}
          `;
        }
      
      case 'sticker':
        const stickerData = content.media_data || content.mediaData;
        const stickerHasMedia = content.has_media || content.hasMedia;
        const stickerMsgId = message.id;
        const stickerMime = content.mime_type || content.mimeType || 'image/webp';
        const isAnimated = content.is_animated || content.isAnimated;
        
        if (stickerData) {
          const stickerSrc = stickerData.startsWith('data:') ? stickerData : `data:${stickerMime};base64,${stickerData}`;
          return `
            <div class="message-sticker">
              <img src="${stickerSrc}" alt="Sticker" loading="lazy">
            </div>
          `;
        } else if (stickerHasMedia) {
          // Media needs to be lazy loaded - show placeholder
          return `
            <div class="message-sticker lazy-media" data-message-id="${stickerMsgId}" data-mime-type="${stickerMime}" data-media-type="sticker">
              <div class="media-placeholder sticker-placeholder" onclick="app.loadMedia('${stickerMsgId}', this)">
                <svg viewBox="0 0 24 24" width="48" height="48">
                  <path fill="currentColor" d="M21.99 4c0-1.1-.89-2-1.99-2H4c-1.1 0-2 .9-2 2v12c0 1.1.9 2 2 2h14l4 4-.01-18zM17 11h-4v4h-2v-4H7V9h4V5h2v4h4v2z"/>
                </svg>
                <span>${isAnimated ? 'Animated sticker' : 'Sticker'}</span>
              </div>
            </div>
          `;
        } else {
          return `<div class="message-media">[ ${isAnimated ? 'Animated ' : ''}Sticker ]</div>`;
        }
      
      case 'location':
        const lat = content.latitude;
        const lng = content.longitude;
        const locName = content.name || content.address || 'Shared Location';
        const locAddress = content.address && content.address !== content.name ? content.address : '';
        const mapsUrl = lat && lng ? `https://www.google.com/maps?q=${lat},${lng}` : null;
        
        if (mapsUrl) {
          return `
            <div class="message-location">
              <a href="${mapsUrl}" target="_blank" rel="noopener" class="location-link">
                <div class="location-preview">
                  <img src="https://maps.googleapis.com/maps/api/staticmap?center=${lat},${lng}&zoom=15&size=300x150&markers=color:red%7C${lat},${lng}&key=" 
                       onerror="this.style.display='none';this.nextElementSibling.style.display='flex';"
                       alt="Map">
                  <div class="location-placeholder" style="display:none;">
                    <svg viewBox="0 0 24 24" width="32" height="32">
                      <path fill="currentColor" d="M12 2C8.13 2 5 5.13 5 9c0 5.25 7 13 7 13s7-7.75 7-13c0-3.87-3.13-7-7-7zm0 9.5c-1.38 0-2.5-1.12-2.5-2.5s1.12-2.5 2.5-2.5 2.5 1.12 2.5 2.5-1.12 2.5-2.5 2.5z"/>
                    </svg>
                  </div>
                </div>
                <div class="location-info">
                  <span class="location-name">${this.escapeHtml(locName)}</span>
                  ${locAddress ? `<span class="location-address">${this.escapeHtml(locAddress)}</span>` : ''}
                  <span class="location-coords">${lat.toFixed(6)}, ${lng.toFixed(6)}</span>
                </div>
              </a>
            </div>
          `;
        } else {
          return `<div class="message-media">[ Location: ${this.escapeHtml(locName)} ]</div>`;
        }
      
      case 'contact':
        return `<div class="message-media">[ Contact: ${this.escapeHtml(content.name)} ]</div>`;
      
      case 'reaction':
        return `<div class="message-text" style="font-size: 32px;">${content.emoji}</div>`;
      
      case 'revoked':
        return `<div class="message-text" style="font-style: italic; opacity: 0.7;">This message was deleted</div>`;
      
      case 'poll':
        const options = (content.options || []).map(o => `  - ${this.escapeHtml(o)}`).join('\n');
        return `
          <div class="message-text">
            <strong>Poll: ${this.escapeHtml(content.question)}</strong>
            <pre style="margin-top: 8px; font-family: inherit;">${options}</pre>
          </div>
        `;
      
      default:
        return `<div class="message-media">[ ${content.rawType || 'Unknown message type'} ]</div>`;
    }
  }

  // Append a single message to the list
  appendMessage(message) {
    const container = document.getElementById('messages-list');
    
    // Check if we need a date separator
    const messages = this.messages.get(message.contactId) || [];
    const prevMessage = messages[messages.length - 2];
    
    let html = '';
    if (prevMessage) {
      const prevDate = new Date(prevMessage.timestamp).toDateString();
      const currDate = new Date(message.timestamp).toDateString();
      if (prevDate !== currDate) {
        html += `<div class="date-separator"><span>${this.formatDate(message.timestamp)}</span></div>`;
      }
    }
    
    html += this.renderMessage(message);
    container.insertAdjacentHTML('beforeend', html);
    
    const newMessage = container.lastElementChild;
    if (newMessage) {
      this.scheduleLinkPreviewLoad(newMessage);
    }
  }

  // Scroll to bottom of messages
  scrollToBottom() {
    const container = document.getElementById('messages-list');
    container.scrollTop = container.scrollHeight;
  }

  restoreDraftForCurrentContact() {
    const input = document.getElementById('message-input');
    if (!input || !this.currentContactId) return;

    input.value = getDraftText(this.drafts, this.currentContactId);
    this.autoResizeTextarea(input);
  }

  handleDraftInput() {
    if (!this.currentContactId) return;

    const input = document.getElementById('message-input');
    this.drafts = upsertDraft(this.drafts, this.currentContactId, input?.value || '');
    this.persistDrafts();
    this.updateDraftBanner();
    this.renderQuickReplies();
    this.renderComposerAssist();
    this.scheduleRenderContacts();
    this.updateChatHeaderNote();
    this.renderConversationWorkspace();
  }

  clearCurrentDraft() {
    if (!this.currentContactId) return;

    this.drafts = upsertDraft(this.drafts, this.currentContactId, '');
    this.persistDrafts();

    const input = document.getElementById('message-input');
    if (input) {
      input.value = '';
      this.autoResizeTextarea(input);
    }

    this.updateDraftBanner();
    this.renderQuickReplies();
    this.renderComposerAssist();
    this.updateSendButton();
    this.scheduleRenderContacts();
    this.updateChatHeaderNote();
    this.renderConversationWorkspace();
  }

  updateDraftBanner() {
    const banner = document.getElementById('draft-banner');
    const text = document.getElementById('draft-banner-text');
    if (!banner || !text) return;

    const preview = this.currentContactId ? getDraftPreview(this.drafts, this.currentContactId, 80) : '';
    if (!preview) {
      banner.classList.add('hidden');
      return;
    }

    text.textContent = `${preview} · Restored automatically when you return.`;
    banner.classList.remove('hidden');
  }

  toggleConversationMenu() {
    const menu = document.getElementById('chat-actions-menu');
    if (!menu) return;

    const shouldOpen = menu.classList.contains('hidden');
    this.closeConversationMenu();
    if (!shouldOpen) return;

    menu.classList.remove('hidden');
    this.updateConversationMenuUI();
  }

  closeConversationMenu() {
    document.getElementById('chat-actions-menu')?.classList.add('hidden');
    this.updateConversationMenuUI();
  }

  updateConversationMenuUI() {
    const button = document.getElementById('chat-menu-button');
    const menu = document.getElementById('chat-actions-menu');
    const searchButton = document.getElementById('chat-search-toggle');
    const starredButton = document.getElementById('chat-starred-toggle');
    const translateAllButton = document.getElementById('chat-translate-all');
    const searchBarVisible = !document.getElementById('chat-search-bar')?.classList.contains('hidden');

    if (button && menu) {
      button.setAttribute('aria-expanded', String(!menu.classList.contains('hidden')));
      button.classList.toggle('active', !menu.classList.contains('hidden'));
    }

    if (searchButton) {
      searchButton.classList.toggle('active', searchBarVisible || Boolean(this.messageSearchQuery));
      searchButton.textContent = searchBarVisible ? 'Hide search' : 'Search messages';
    }

    if (starredButton) {
      starredButton.classList.toggle('active', this.starredOnly);
      starredButton.textContent = this.starredOnly ? 'Show all messages' : 'Show starred messages only';
    }

    if (translateAllButton) {
      const count = this.getUntranslatedIncomingMessagesForCurrentConversation().length;
      translateAllButton.disabled = count === 0;
      translateAllButton.textContent = count === 0
        ? 'All incoming translated'
        : `Translate ${count} incoming message${count === 1 ? '' : 's'}`;
    }
  }

  getUntranslatedIncomingMessagesForCurrentConversation() {
    return getUntranslatedIncomingMessages(this.messages.get(this.currentContactId) || []);
  }

  renderQuickReplies() {
    const bar = document.querySelector('.quick-replies-bar');
    const container = document.getElementById('quick-replies-list');
    const saveButton = document.getElementById('quick-reply-save');
    const draftText = this.currentContactId ? getDraftText(this.drafts, this.currentContactId) : '';
    const hasReplies = Array.isArray(this.quickReplies) && this.quickReplies.length > 0;
    const shouldShowBar = Boolean(this.currentContactId) && (hasReplies || Boolean(draftText));

    if (saveButton) {
      saveButton.disabled = !this.currentContactId || !draftText;
      saveButton.textContent = hasReplies ? 'Save draft' : 'Save this draft';
    }

    if (bar) {
      bar.classList.toggle('hidden', !shouldShowBar);
    }

    if (!container) return;

    if (!shouldShowBar) {
      container.innerHTML = '';
      return;
    }

    if (!hasReplies) {
      container.innerHTML = '<span class="quick-replies-empty">Saved replies will appear here once you keep one.</span>';
      return;
    }

    container.innerHTML = this.quickReplies.map((reply) => `
      <button class="quick-reply-chip" type="button" data-quick-reply-id="${reply.id}" title="Insert saved reply">
        <span class="quick-reply-text">${this.escapeHtml(reply.text)}</span>
        <span class="quick-reply-remove" data-quick-reply-remove="${reply.id}" title="Remove saved reply">×</span>
      </button>
    `).join('');
  }

  saveCurrentDraftAsQuickReply() {
    if (!this.currentContactId) return;
    const draftText = getDraftText(this.drafts, this.currentContactId);
    if (!draftText) return;

    this.quickReplies = upsertQuickReply(this.quickReplies, draftText);
    this.persistQuickReplies();
    this.renderQuickReplies();
  }

  applyQuickReply(quickReplyId) {
    const quickReply = (this.quickReplies || []).find((entry) => entry.id === quickReplyId);
    if (!quickReply) return;

    const input = document.getElementById('message-input');
    if (!input) return;

    input.value = quickReply.text;
    this.autoResizeTextarea(input);
    this.updateSendButton();
    this.handleDraftInput();
    input.focus();
  }

  deleteQuickReply(quickReplyId) {
    this.quickReplies = removeQuickReply(this.quickReplies, quickReplyId);
    this.persistQuickReplies();
    this.renderQuickReplies();
  }

  toggleMessageStar(messageId) {
    if (!this.currentContactId) return;

    const messages = this.messages.get(this.currentContactId) || [];
    const message = messages.find((entry) => entry.id === messageId);
    if (!message) return;

    this.starredMessages = toggleStarredMessage(this.starredMessages, message);
    this.persistStarredMessages();
    this.updateStarredToggleUI();

    const visibleMessages = this.getVisibleMessagesForCurrentConversation();
    this.renderMessages(visibleMessages);
  }

  async toggleStarredOnly() {
    this.starredOnly = !this.starredOnly;
    if (this.starredOnly) {
      await this.ensureFullConversationLoaded();
    }
    this.updateStarredToggleUI();
    this.refreshCurrentConversationView();
  }

  toggleChatSearch() {
    const searchBar = document.getElementById('chat-search-bar');
    const input = document.getElementById('chat-search-input');
    if (!searchBar || !input) return;

    const shouldShow = searchBar.classList.contains('hidden');
    searchBar.classList.toggle('hidden', !shouldShow);

    if (shouldShow) {
      input.focus();
      input.select();
    } else {
      this.messageSearchQuery = '';
      input.value = '';
      this.updateChatSearchUI();
      this.refreshCurrentConversationView();
    }

    this.updateConversationMenuUI();
  }

  clearChatSearch() {
    this.messageSearchQuery = '';
    const input = document.getElementById('chat-search-input');
    if (input) input.value = '';
    this.updateChatSearchUI();
    this.refreshCurrentConversationView();
    this.updateConversationMenuUI();
  }

  async setChatSearchQuery(query) {
    this.messageSearchQuery = query || '';
    if (this.messageSearchQuery) {
      await this.ensureFullConversationLoaded();
    }
    this.updateChatSearchUI();
    this.refreshCurrentConversationView();
    this.updateConversationMenuUI();
  }

  updateChatSearchUI() {
    const countEl = document.getElementById('chat-search-count');
    this.updateConversationMenuUI();
    if (!countEl) return;

    const visibleMessages = this.getVisibleMessagesForCurrentConversation();
    const count = countMatchingMessages(
      this.messages.get(this.currentContactId) || [],
      this.messageSearchQuery,
      { starredOnly: this.starredOnly, starredLookup: this.starredMessages },
    );

    if (!this.currentContactId) {
      countEl.textContent = 'No chat selected';
    } else if (!this.messageSearchQuery) {
      countEl.textContent = this.starredOnly
        ? `${visibleMessages.length} starred shown`
        : `${visibleMessages.length} messages`;
    } else {
      countEl.textContent = `${count} match${count === 1 ? '' : 'es'}`;
    }
  }

  updateStarredToggleUI() {
    const button = document.getElementById('chat-starred-toggle');
    if (button) {
      button.title = this.starredOnly ? 'Show all messages' : 'Show starred messages only';
    }
    this.updateConversationMenuUI();
  }

  getVisibleMessagesForCurrentConversation() {
    const messages = this.messages.get(this.currentContactId) || [];
    return filterMessagesByQuery(messages, this.messageSearchQuery, {
      starredOnly: this.starredOnly,
      starredLookup: this.starredMessages,
    });
  }

  refreshCurrentConversationView() {
    if (!this.currentContactId) return;

    this.renderMessages(this.getVisibleMessagesForCurrentConversation());
    this.updateChatSearchUI();
  }

  // Send a message
  async sendMessage() {
    const input = document.getElementById('message-input');
    const text = input.value.trim();
    
    if (!text || !this.currentContactId) return;
    
    const sendButton = document.getElementById('send-button');
    sendButton.disabled = true;
    
    // Capture reply state before clearing
    const replyTo = this.replyingTo ? this.replyingTo.messageId : null;
    const replyToSender = this.replyingTo ? this.replyingTo.senderJid : null;

    if (this.demoMode) {
      const replyContext = this.replyingTo ? { ...this.replyingTo } : null;
      const metadata = this.getContactMetadata(this.currentContactId);
      const targetLanguage = metadata.targetLanguage || 'Spanish';
      const translatedText = simulateTranslation(text, targetLanguage);
      const localMessage = {
        id: `demo-out-${Date.now()}`,
        timestamp: Date.now(),
        contactId: this.currentContactId,
        isFromMe: true,
        isForwarded: false,
        senderJid: this.getMessageSenderJid({ isFromMe: true }),
        content: { type: 'text', body: text },
        isTranslated: Boolean(translatedText),
        originalText: text,
        translatedText,
        sourceLanguage: targetLanguage,
        replyContext,
      };

      input.value = '';
      this.drafts = upsertDraft(this.drafts, this.currentContactId, '');
      this.persistDrafts();
      this.updateDraftBanner();
      this.renderQuickReplies();
      this.clearReply();
      this.updateSendButton();
      this.autoResizeTextarea(input);

      if (!this.messages.has(this.currentContactId)) {
        this.messages.set(this.currentContactId, []);
      }
      this.messages.get(this.currentContactId).push(localMessage);
      this.refreshCurrentConversationView();
      this.scrollToBottom();
      this.updateContactInList(localMessage);
      this.renderConversationWorkspace();
      sendButton.disabled = false;
      this.updateSendButton();
      return;
    }
    
    try {
      const requestBody = {
        contactId: this.currentContactId,
        text: text
      };
      
      // Add reply params if replying
      if (replyTo) {
        requestBody.replyTo = replyTo;
        if (replyToSender) {
          requestBody.replyToSender = replyToSender;
        }
        requestBody.replyToText = this.replyingTo?.text || null;
        requestBody.replyToSenderName = this.replyingTo?.senderName || null;
      }
      
      const response = await this.apiFetch('/api/send', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(requestBody)
      });
      
      const result = await response.json();
      
      if (!response.ok) {
        throw new Error(result.error || 'Failed to send message');
      }
      
      // Save reply context before clearing (for local message display)
      const replyContext = this.replyingTo ? { ...this.replyingTo } : null;
      
      // Clear input and reply state on success
      input.value = '';
      this.drafts = upsertDraft(this.drafts, this.currentContactId, '');
      this.persistDrafts();
      this.updateDraftBanner();
      this.renderQuickReplies();
      this.clearReply();
      this.updateSendButton();
      this.autoResizeTextarea(input);
      this.scheduleRenderContacts();
      this.updateChatHeaderNote();
      this.renderConversationWorkspace();
      
      // Create a local message representation with translation info from response
      const localMessage = {
        id: result.messageId || 'temp-' + Date.now(),
        timestamp: result.timestamp || Date.now(),
        contactId: this.currentContactId,
        isFromMe: true,
        isForwarded: false,
        senderJid: this.getMessageSenderJid({ isFromMe: true }),
        content: { type: 'text', body: text },
        // Include translation info if the message was translated
        isTranslated: result.isTranslated || false,
        originalText: result.isTranslated ? text : null,  // What user typed (English)
        translatedText: result.translatedText || null,     // What was sent (foreign language)
        sourceLanguage: result.sourceLanguage || null,     // Target language
        // Include reply context if this was a reply
        replyContext: replyContext
      };
      
      // Add to local store and display
      if (!this.messages.has(this.currentContactId)) {
        this.messages.set(this.currentContactId, []);
      }
      
      const messages = this.messages.get(this.currentContactId);
      if (!messages.some(m => m.id === localMessage.id)) {
        messages.push(localMessage);
        this.refreshCurrentConversationView();
        this.scrollToBottom();
        this.updateChatHeaderNote();
        this.renderConversationWorkspace();
      }
      
      // Update contact list
      this.updateContactInList(localMessage);
      
      // Refresh usage stats if translation occurred
      if (result.isTranslated) {
        this.fetchGlobalUsage();
        this.fetchConversationUsage(this.currentContactId);
      }
      
    } catch (err) {
      console.error('Failed to send message:', err);
      alert('Failed to send message: ' + err.message);
    } finally {
      sendButton.disabled = false;
      this.updateSendButton();
    }
  }

  // Send an image
  async sendImage(file) {
    if (!file || !this.currentContactId) return;

    // Check file size (limit to 16MB)
    if (file.size > 16 * 1024 * 1024) {
      alert('Image is too large. Maximum size is 16MB.');
      return;
    }

    // Check file type
    const allowedImageTypes = new Set(['image/jpeg', 'image/png', 'image/gif', 'image/webp']);
    if (!allowedImageTypes.has(String(file.type || '').toLowerCase())) {
      alert('Please select a JPEG, PNG, GIF, or WebP image file.');
      return;
    }

    const attachButton = document.getElementById('attach-button');
    attachButton.disabled = true;
    
    // Capture reply state before clearing
    const replyTo = this.replyingTo ? this.replyingTo.messageId : null;
    const replyToSender = this.replyingTo ? this.replyingTo.senderJid : null;

    try {
      // Read file as base64
      const mediaData = await this.fileToBase64(file);
      
      const requestBody = {
        contactId: this.currentContactId,
        mediaData: mediaData,
        mimeType: file.type,
        caption: null
      };
      
      // Add reply params if replying
      if (replyTo) {
        requestBody.replyTo = replyTo;
        if (replyToSender) {
          requestBody.replyToSender = replyToSender;
        }
        requestBody.replyToText = this.replyingTo?.text || null;
        requestBody.replyToSenderName = this.replyingTo?.senderName || null;
      }
      
      const response = await this.apiFetch('/api/send-image', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(requestBody)
      });

      const result = await response.json();

      if (!response.ok) {
        throw new Error(result.error || 'Failed to send image');
      }

      // Save reply context before clearing (for local message display)
      const replyContext = this.replyingTo ? { ...this.replyingTo } : null;
      
      // Clear reply state on success
      this.clearReply();
      
      // Create a local message representation
      const localMessage = {
        id: result.messageId || 'temp-img-' + Date.now(),
        timestamp: result.timestamp || Date.now(),
        contactId: this.currentContactId,
        isFromMe: true,
        isForwarded: false,
        senderJid: this.getMessageSenderJid({ isFromMe: true }),
        content: { 
          type: 'image', 
          mime_type: file.type,
          media_data: mediaData
        },
        // Include reply context if this was a reply
        replyContext: replyContext
      };

      // Add to local store and display
      if (!this.messages.has(this.currentContactId)) {
        this.messages.set(this.currentContactId, []);
      }

      const messages = this.messages.get(this.currentContactId);
      if (!messages.some(m => m.id === localMessage.id)) {
        messages.push(localMessage);
        this.refreshCurrentConversationView();
        this.scrollToBottom();
      }

      // Update contact list
      this.updateContactInList(localMessage);

    } catch (err) {
      console.error('Failed to send image:', err);
      alert('Failed to send image: ' + err.message);
    } finally {
      attachButton.disabled = false;
    }
  }

  // Convert file to base64
  fileToBase64(file) {
    return new Promise((resolve, reject) => {
      const reader = new FileReader();
      reader.onload = () => {
        // Remove the data URL prefix (e.g., "data:image/jpeg;base64,")
        const base64 = reader.result.split(',')[1];
        resolve(base64);
      };
      reader.onerror = reject;
      reader.readAsDataURL(file);
    });
  }

  // Send a reaction to a message
  async sendReaction(messageId, contactId, senderJid, emoji) {
    // Close any open reaction pickers
    document.querySelectorAll('.reaction-picker.show').forEach(el => el.classList.remove('show'));
    
    if (!messageId || !contactId) {
      console.error('Missing messageId or contactId for reaction');
      return;
    }

    try {
      const response = await this.apiFetch('/api/react', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          contactId: contactId,
          messageId: messageId,
          senderJid: senderJid || null,
          emoji: emoji
        })
      });

      const result = await response.json();

      if (!response.ok) {
        throw new Error(result.error || 'Failed to send reaction');
      }

      // Update local message with the reaction
      const messages = this.messages.get(contactId);
      if (messages) {
        const message = messages.find(m => m.id === messageId);
        if (message) {
          // Initialize reactions map if needed
          if (!message.reactions) {
            message.reactions = {};
          }
          
          // Get my phone number for tracking who reacted
          const myPhone = document.getElementById('user-phone')?.textContent?.replace('+', '') || 'me';
          
          // Remove my previous reaction (if any)
          for (const [existingEmoji, reactors] of Object.entries(message.reactions)) {
            message.reactions[existingEmoji] = reactors.filter(r => r !== myPhone);
            if (message.reactions[existingEmoji].length === 0) {
              delete message.reactions[existingEmoji];
            }
          }
          
          // Add new reaction (empty emoji means remove)
          if (emoji) {
            if (!message.reactions[emoji]) {
              message.reactions[emoji] = [];
            }
            message.reactions[emoji].push(myPhone);
          }
          
          // Update the message display
          this.updateMessageReactions(messageId);
        }
      }
      
      console.log('Reaction sent successfully');
    } catch (err) {
      console.error('Failed to send reaction:', err);
      alert('Failed to send reaction: ' + err.message);
    }
  }

  getConversationHandoffBrief() {
    if (!this.currentContactId) return '';

    const contact = this.contacts.find(entry => entry.id === this.currentContactId) || { id: this.currentContactId };
    return buildConversationHandoffBrief({
      contact,
      metadata: this.getContactMetadata(this.currentContactId),
      messages: this.messages.get(this.currentContactId) || [],
      drafts: this.drafts,
    });
  }

  async copyConversationBrief() {
    const text = this.getConversationHandoffBrief();
    if (!text) return;

    const button = document.getElementById('workspace-copy-brief');
    try {
      if (navigator.clipboard?.writeText) {
        await navigator.clipboard.writeText(text);
      } else {
        const textarea = document.createElement('textarea');
        textarea.value = text;
        textarea.setAttribute('readonly', '');
        textarea.style.position = 'fixed';
        textarea.style.opacity = '0';
        document.body.appendChild(textarea);
        textarea.select();
        document.execCommand('copy');
        textarea.remove();
      }
      if (button) {
        button.textContent = 'Copied';
        window.setTimeout(() => this.renderConversationWorkspace(), 1600);
      }
    } catch (err) {
      console.error('Failed to copy conversation brief:', err);
      alert('Failed to copy conversation brief: ' + err.message);
    }
  }

  async translateUntranslatedIncomingMessages() {
    if (!this.currentContactId) return;

    const messages = this.getUntranslatedIncomingMessagesForCurrentConversation();
    if (messages.length === 0) {
      this.updateConversationMenuUI();
      this.renderConversationWorkspace();
      return;
    }

    const buttons = [
      document.getElementById('chat-translate-all'),
      document.getElementById('workspace-translate-all'),
    ].filter(Boolean);
    buttons.forEach((button) => {
      button.disabled = true;
      button.textContent = 'Translating...';
    });

    try {
      for (const message of messages) {
        await this.translateMessage(message.id, { silent: true, rerender: false });
      }
      this.refreshCurrentConversationView();
      this.renderConversationWorkspace();
    } finally {
      this.updateConversationMenuUI();
      this.renderConversationWorkspace();
    }
  }

  // Translate a message manually
  async translateMessage(messageId, options = {}) {
    const { silent = false, rerender = true } = options;
    const messages = this.messages.get(this.currentContactId);
    if (!messages) return;
    
    const message = messages.find(m => m.id === messageId);
    if (!message) return;
    
    // Check if already translated
    if (message.is_translated || message.isTranslated) {
      if (!silent) alert('This message has already been translated.');
      return;
    }
    
    // Get the text to translate
    const text = message.content?.body || message.content?.caption || message.content?.text;
    if (!text) {
      if (!silent) alert('No text to translate in this message.');
      return;
    }

    if (this.demoMode) {
      const metadata = this.getContactMetadata(this.currentContactId);
      const targetLanguage = metadata.targetLanguage || message.sourceLanguage || 'English';
      const translatedText = simulateTranslation(text, targetLanguage);
      message.original_text = text;
      message.originalText = text;
      message.translated_text = translatedText;
      message.translatedText = translatedText;
      message.source_language = targetLanguage;
      message.sourceLanguage = targetLanguage;
      message.is_translated = true;
      message.isTranslated = true;
      if (rerender) {
        this.renderMessages(this.getVisibleMessagesForCurrentConversation());
        this.renderConversationWorkspace();
        this.updateConversationMenuUI();
      }
      return;
    }
    
    try {
      // Call the translation API
      const response = await this.apiFetch('/api/translate', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json', ...this.getAuthHeaders() },
        body: JSON.stringify({
          text: text,
          messageId: messageId,
          contactId: this.currentContactId
        })
      });
      
      const result = await response.json();
      if (!response.ok || !result.success) {
        throw new Error(result.error || 'Translation failed');
      }
      if (!result.translatedText || !String(result.translatedText).trim()) {
        throw new Error(result.error || 'Translation did not return translated text');
      }
      
      // Update the local message with translation
      message.translated_text = result.translatedText;
      message.translatedText = result.translatedText;
      message.source_language = result.sourceLanguage;
      message.sourceLanguage = result.sourceLanguage;
      message.is_translated = true;
      message.isTranslated = true;
      
      // Re-render messages to show translation
      if (rerender) {
        const currentMessages = this.getVisibleMessagesForCurrentConversation();
        this.renderMessages(currentMessages);
        this.renderConversationWorkspace();
        this.updateConversationMenuUI();
      }
      
    } catch (err) {
      console.error('Translation failed:', err);
      if (!silent) alert('Translation failed: ' + err.message);
    }
  }

  // Update reactions display for a specific message
  updateMessageReactions(messageId) {
    const messageEl = document.querySelector(`.message[data-message-id="${messageId}"]`);
    if (!messageEl) return;
    
    const messages = this.messages.get(this.currentContactId);
    if (!messages) return;
    
    const message = messages.find(m => m.id === messageId);
    if (!message || !message.reactions) return;
    
    // Remove existing reactions container
    const existingReactions = messageEl.querySelector('.message-reactions');
    if (existingReactions) {
      existingReactions.remove();
    }
    
    // Build reactions HTML
    const reactionsHtml = this.renderReactions(message.reactions);
    if (reactionsHtml) {
      // Insert before message-footer
      const footer = messageEl.querySelector('.message-footer');
      if (footer) {
        footer.insertAdjacentHTML('beforebegin', reactionsHtml);
      }
    }
  }

  // Render reactions for a message
  renderReactions(reactions) {
    if (!reactions || Object.keys(reactions).length === 0) return '';
    
    const reactionItems = Object.entries(reactions)
      .filter(([emoji, reactors]) => reactors.length > 0)
      .map(([emoji, reactors]) => {
        const count = reactors.length > 1 ? `<span class="reaction-count">${reactors.length}</span>` : '';
        return `<span class="reaction-item">${emoji}${count}</span>`;
      })
      .join('');
    
    if (!reactionItems) return '';
    
    return `<div class="message-reactions">${reactionItems}</div>`;
  }

  // Set reply state for a message
  setReplyTo(message) {
    const content = message.content;
    const isFromMe = message.isFromMe || message.is_from_me;
    
    // Get message preview text for display
    let previewText = '';
    switch (content.type) {
      case 'text':
        previewText = content.body || content.text || '';
        break;
      case 'image':
        previewText = content.caption || '[ Image ]';
        break;
      case 'video':
        previewText = content.caption || '[ Video ]';
        break;
      case 'audio':
        previewText = content.isVoiceNote ? '[ Voice Note ]' : '[ Audio ]';
        break;
      case 'document':
        previewText = '[ Document: ' + (content.fileName || 'file') + ' ]';
        break;
      case 'sticker':
        previewText = '[ Sticker ]';
        break;
      default:
        previewText = '[ Message ]';
    }
    
    // Truncate long messages
    if (previewText.length > 100) {
      previewText = previewText.substring(0, 100) + '...';
    }
    
    // Get sender name
    let senderName = 'You';
    if (!isFromMe) {
      senderName = message.senderName || message.sender_name || message.senderPhone || message.sender_phone || 'Unknown';
    }
    
    // Get sender JID for the reply
    const senderJid = this.getMessageSenderJid(message);
    
    // Capture image data if replying to an image (for AI compose)
    let imageData = null;
    let imageType = null;
    if (content.type === 'image') {
      const mediaData = content.media_data || content.mediaData;
      if (mediaData) {
        imageData = mediaData;
        imageType = content.mime_type || content.mimeType || 'image/jpeg';
      }
    }
    
    this.replyingTo = {
      messageId: message.id,
      senderJid: senderJid,
      senderName: senderName,
      text: previewText,
      isFromMe: isFromMe,
      imageData: imageData,
      imageType: imageType
    };
    
    this.updateReplyPreview();
    
    // Focus the input
    document.getElementById('message-input').focus();
  }

  // Clear reply state
  clearReply() {
    this.replyingTo = null;
    this.updateReplyPreview();
  }

  // Update the reply preview UI
  updateReplyPreview() {
    const previewContainer = document.getElementById('reply-preview');
    if (!previewContainer) return;
    
    if (this.replyingTo) {
      const senderEl = previewContainer.querySelector('.reply-preview-sender');
      const textEl = previewContainer.querySelector('.reply-preview-text');
      
      if (senderEl) senderEl.textContent = this.replyingTo.senderName;
      if (textEl) textEl.textContent = this.replyingTo.text;
      
      previewContainer.classList.remove('hidden');
    } else {
      previewContainer.classList.add('hidden');
    }
  }

  // Handle reply button click
  handleReplyClick(messageId) {
    // Close any open reaction pickers
    document.querySelectorAll('.reaction-picker.show').forEach(el => el.classList.remove('show'));
    
    // Find the message in our cache
    if (!this.currentContactId) return;
    
    const messages = this.messages.get(this.currentContactId);
    if (!messages) return;
    
    const message = messages.find(m => m.id === messageId);
    if (message) {
      this.setReplyTo(message);
    }
  }

  // Update send button state
  updateSendButton() {
    const input = document.getElementById('message-input');
    const sendButton = document.getElementById('send-button');
    const dropdownToggle = document.getElementById('send-dropdown-toggle');
    const hasContent = input.value.trim() && this.currentContactId;
    
    sendButton.disabled = !hasContent;
    if (dropdownToggle) {
      dropdownToggle.disabled = !hasContent;
    }
  }

  // Send message composed by AI
  async sendWithAI() {
    const input = document.getElementById('message-input');
    const prompt = input.value.trim();
    
    if (!prompt || !this.currentContactId) return;
    
    // Show AI composing indicator
    this.showAIComposing(true);
    
    try {
      // Build request with optional reply context
      const requestBody = { prompt };
      
      if (this.replyingTo) {
        requestBody.replyToText = this.replyingTo.text;
        requestBody.replyToSender = this.replyingTo.senderName;
        
        // Include image data if replying to an image
        if (this.replyingTo.imageData) {
          requestBody.replyToImage = this.replyingTo.imageData;
          requestBody.replyToImageType = this.replyingTo.imageType;
        }
      }
      
      const response = await this.apiFetch('/api/ai-compose', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(requestBody)
      });
      
      const result = await response.json();
      
      if (!result.success) {
        throw new Error(result.error || 'AI compose failed');
      }
      
      const aiMessage = String(result.message || '').trim();
      if (!aiMessage) {
        throw new Error('AI compose returned an empty draft');
      }

      input.value = aiMessage;
      this.autoResizeTextarea(input);
      this.updateSendButton();
      this.handleDraftInput();
      input.focus();
      
      // Log cost if available
      if (result.costUsd) {
        console.log(`AI compose cost: $${result.costUsd.toFixed(6)}`);
      }
      
    } catch (err) {
      console.error('Failed to compose with AI:', err);
      alert('Failed to compose message with AI: ' + err.message);
    } finally {
      this.showAIComposing(false);
    }
  }

  // Generate AI reply for a received message (styled to sound like the user)
  async generateAIReply(messageId) {
    if (!this.currentContactId) return;
    
    // Find the message in cache
    const messages = this.messages.get(this.currentContactId);
    const message = messages?.find(m => m.id === messageId);
    if (!message) {
      console.error('Message not found:', messageId);
      return;
    }
    
    // Show loading state on the button
    const msgEl = document.querySelector(`[data-message-id="${messageId}"]`);
    const btn = msgEl?.querySelector('.ai-reply-btn');
    if (btn) {
      btn.classList.add('loading');
      btn.disabled = true;
    }
    
    try {
      if (this.demoMode) {
        const contact = this.contacts.find(entry => entry.id === this.currentContactId) || {};
        const replyText = suggestDemoReply({
          message,
          metadata: this.getContactMetadata(this.currentContactId),
          contact,
        });

        this.setReplyTo(message);
        const input = document.getElementById('message-input');
        input.value = replyText;
        this.autoResizeTextarea(input);
        this.updateSendButton();
        this.handleDraftInput();
        input.focus();
        return;
      }

      const response = await this.apiFetch('/api/ai-reply', {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
          ...this.getAuthHeaders()
        },
        body: JSON.stringify({
          contactId: this.currentContactId,
          messageId: messageId
        })
      });
      
      const result = await response.json();
      
      if (!result.success) {
        throw new Error(result.error || 'AI reply generation failed');
      }
      if (!result.replyText || !result.replyText.trim()) {
        throw new Error('AI reply was empty');
      }
      
      // Set reply context to this message (reuses existing setReplyTo)
      this.setReplyTo(message);
      
      // Populate input with AI-generated reply (user can edit before sending)
      const input = document.getElementById('message-input');
      input.value = result.replyText;
      this.autoResizeTextarea(input);
      this.updateSendButton();
      this.handleDraftInput();
      input.focus();
      
      // Log cost for debugging
      if (result.costUsd) {
        console.log(`AI reply generated, cost: $${result.costUsd.toFixed(6)}`);
      }
      
    } catch (err) {
      console.error('Failed to generate AI reply:', err);
      alert('Failed to generate AI reply: ' + err.message);
    } finally {
      if (btn) {
        btn.classList.remove('loading');
        btn.disabled = false;
      }
    }
  }

  // Show/hide AI composing indicator
  showAIComposing(show) {
    let indicator = document.getElementById('ai-composing-indicator');
    
    if (show) {
      if (!indicator) {
        indicator = document.createElement('div');
        indicator.id = 'ai-composing-indicator';
        indicator.className = 'ai-composing';
        indicator.innerHTML = `
          <div class="ai-composing-spinner"></div>
          <span>AI is composing your message...</span>
        `;
        
        const inputArea = document.querySelector('.message-input-area');
        const inputContainer = document.querySelector('.input-container');
        if (inputArea && inputContainer) {
          inputArea.insertBefore(indicator, inputContainer);
        }
      }
      indicator.style.display = 'flex';
      
      // Disable input while composing
      document.getElementById('message-input').disabled = true;
      document.getElementById('send-button').disabled = true;
      document.getElementById('send-dropdown-toggle').disabled = true;
    } else {
      if (indicator) {
        indicator.style.display = 'none';
      }
      // Re-enable input
      document.getElementById('message-input').disabled = false;
      this.updateSendButton();
    }
  }

  // Auto-resize textarea (expands up to max-height)
  autoResizeTextarea(textarea) {
    textarea.style.height = 'auto';
    textarea.style.height = Math.min(textarea.scrollHeight, 150) + 'px';
  }

  // Bind UI events
  bindEvents() {
    // Logout button
    document.getElementById('logout-button')?.addEventListener('click', () => {
      this.handleLogout();
    });

    document.getElementById('inbox-search-input')?.addEventListener('input', (event) => {
      this.setInboxSearchQuery(event.target.value);
    });

    document.getElementById('inbox-clear-button')?.addEventListener('click', () => {
      this.clearInboxFilters();
    });

    document.querySelectorAll('[data-inbox-filter]').forEach((button) => {
      button.addEventListener('click', () => this.toggleInboxFilter(button.dataset.inboxFilter));
    });

    document.getElementById('command-palette-button')?.addEventListener('click', () => {
      this.openCommandPalette();
    });

    document.getElementById('demo-mode-button')?.addEventListener('click', () => {
      this.startDemoWorkspace();
    });

    document.getElementById('visitor-dashboard-open-palette')?.addEventListener('click', () => {
      this.openCommandPalette();
    });

    document.getElementById('visitor-dashboard')?.addEventListener('click', (event) => {
      const presetTarget = event.target.closest('[data-dashboard-preset]');
      if (presetTarget) {
        this.applyInboxPreset(presetTarget.dataset.dashboardPreset);
        return;
      }

      const contactTarget = event.target.closest('[data-dashboard-contact]');
      if (contactTarget) {
        this.selectContact(contactTarget.dataset.dashboardContact);
      }
    });

    // Contact click
    document.getElementById('contacts-list').addEventListener('click', (e) => {
      const contactItem = e.target.closest('.contact-item');
      if (contactItem) {
        const contactId = contactItem.dataset.contactId;
        this.selectContact(contactId);
      }
    });

    // Back button (mobile)
    document.getElementById('back-button').addEventListener('click', () => {
      this.closeChat();
    });

    // Handle browser back button on mobile
    window.addEventListener('popstate', (e) => {
      if (this.currentContactId && this.isMobile()) {
        e.preventDefault();
        this.closeChat();
      }
    });

    window.addEventListener('resize', this.handleResponsiveLayoutChange);
    window.addEventListener('orientationchange', this.handleResponsiveLayoutChange);
    this.handleResponsiveLayoutChange();

    // Message input
    const input = document.getElementById('message-input');
    const sendButton = document.getElementById('send-button');

    // Update send button state on input
    input.addEventListener('input', () => {
      this.updateSendButton();
      this.autoResizeTextarea(input);
      this.handleDraftInput();
    });

    // Send on Cmd+Enter (Mac) or Ctrl+Enter (Windows/Linux)
    // Plain Enter creates newlines (like WhatsApp desktop)
    input.addEventListener('keydown', (e) => {
      if (e.key === 'Enter' && (e.metaKey || e.ctrlKey)) {
        e.preventDefault();
        if (!sendButton.disabled) {
          this.sendMessage();
        }
      }
    });

    // Send button click
    sendButton.addEventListener('click', () => {
      this.sendMessage();
    });

    document.getElementById('composer-use-suggestion')?.addEventListener('click', () => {
      this.useComposerSuggestedReply();
    });

    document.getElementById('composer-remind-tomorrow')?.addEventListener('click', () => {
      this.setComposerReminderTomorrow();
    });

    document.getElementById('composer-profile-presets')?.addEventListener('click', (event) => {
      const languageTarget = event.target.closest('[data-composer-language]');
      if (languageTarget) {
        this.applyComposerProfilePreset({ language: languageTarget.dataset.composerLanguage });
        return;
      }

      const toneTarget = event.target.closest('[data-composer-tone]');
      if (toneTarget) {
        this.applyComposerProfilePreset({ tone: toneTarget.dataset.composerTone });
      }
    });

    document.getElementById('composer-smart-replies')?.addEventListener('click', (event) => {
      const replyTarget = event.target.closest('[data-smart-reply-id]');
      if (replyTarget) {
        this.useComposerSuggestedReply(replyTarget.dataset.smartReplyId);
      }
    });

    document.getElementById('composer-reminder-presets')?.addEventListener('click', (event) => {
      const presetTarget = event.target.closest('[data-reminder-preset]');
      if (presetTarget) {
        this.setComposerReminderPreset(presetTarget.dataset.reminderPreset);
      }
    });

    document.getElementById('composer-edit-profile')?.addEventListener('click', () => {
      this.openSettingsModal();
    });

    // Image attach button
    const attachButton = document.getElementById('attach-button');
    const imageInput = document.getElementById('image-input');
    
    attachButton.addEventListener('click', () => {
      if (this.currentContactId) {
        imageInput.click();
      }
    });

    // Handle image selection
    imageInput.addEventListener('change', async (e) => {
      const file = e.target.files[0];
      if (file && this.currentContactId) {
        await this.sendImage(file);
      }
      // Reset input so the same file can be selected again
      imageInput.value = '';
    });

    // Handle visibility change (for reconnecting on mobile)
    document.addEventListener('visibilitychange', () => {
      if (document.visibilityState === 'visible' && !this.connected) {
        // Try to reconnect WebSocket if disconnected
        if (this.ws.readyState === WebSocket.CLOSED) {
          this.connectWebSocket();
        }
      }
    });

    // Prevent pull-to-refresh on mobile when scrolling messages
    const messagesList = document.getElementById('messages-list');
    messagesList.addEventListener('touchstart', (e) => {
      if (messagesList.scrollTop === 0) {
        messagesList.scrollTop = 1;
      }
    }, { passive: true });

    // Close reaction pickers and emoji picker when clicking outside
    document.addEventListener('click', (e) => {
      if (!e.target.closest('.reaction-button-container')) {
        document.querySelectorAll('.reaction-picker.show').forEach(el => el.classList.remove('show'));
      }
      // Close send dropdown when clicking outside
      if (!e.target.closest('.send-button-group')) {
        document.getElementById('send-dropdown')?.classList.add('hidden');
      }
      // Close emoji picker when clicking outside
      if (!e.target.closest('.emoji-button-container')) {
        document.getElementById('emoji-picker')?.classList.add('hidden');
      }
      // Close conversation actions menu when clicking outside
      if (!e.target.closest('.chat-actions')) {
        this.closeConversationMenu();
      }
    });

    // Emoji picker button
    const emojiButton = document.getElementById('emoji-button');
    const emojiPicker = document.getElementById('emoji-picker');
    
    emojiButton?.addEventListener('click', (e) => {
      e.stopPropagation();
      emojiPicker?.classList.toggle('hidden');
      if (!emojiPicker?.classList.contains('hidden')) {
        this.renderEmojiCategory(this.currentEmojiCategory);
      }
    });

    // Emoji tab clicks
    document.querySelectorAll('.emoji-tab').forEach(tab => {
      tab.addEventListener('click', (e) => {
        e.stopPropagation();
        const category = tab.dataset.category;
        this.currentEmojiCategory = category;
        document.querySelectorAll('.emoji-tab').forEach(t => t.classList.remove('active'));
        tab.classList.add('active');
        this.renderEmojiCategory(category);
      });
    });

    // Emoji selection (delegated)
    document.getElementById('emoji-picker-content')?.addEventListener('click', (e) => {
      const emojiSpan = e.target.closest('.emoji-item');
      if (emojiSpan) {
        const emoji = emojiSpan.textContent;
        this.insertEmoji(emoji);
        this.addRecentEmoji(emoji);
      }
    });

    // Send dropdown toggle
    const dropdownToggle = document.getElementById('send-dropdown-toggle');
    const sendDropdown = document.getElementById('send-dropdown');
    
    dropdownToggle?.addEventListener('click', (e) => {
      e.stopPropagation();
      sendDropdown?.classList.toggle('hidden');
    });

    // Send with AI button
    const sendAiButton = document.getElementById('send-ai-button');
    sendAiButton?.addEventListener('click', () => {
      sendDropdown?.classList.add('hidden');
      this.sendWithAI();
    });

    document.getElementById('draft-clear-button')?.addEventListener('click', () => {
      this.clearCurrentDraft();
    });

    document.getElementById('quick-reply-save')?.addEventListener('click', () => {
      this.saveCurrentDraftAsQuickReply();
    });

    document.getElementById('quick-replies-list')?.addEventListener('click', (event) => {
      const removeTarget = event.target.closest('[data-quick-reply-remove]');
      if (removeTarget) {
        event.stopPropagation();
        this.deleteQuickReply(removeTarget.dataset.quickReplyRemove);
        return;
      }

      const quickReplyButton = event.target.closest('[data-quick-reply-id]');
      if (quickReplyButton) {
        this.applyQuickReply(quickReplyButton.dataset.quickReplyId);
      }
    });

    document.getElementById('app-appearance-button')?.addEventListener('click', () => {
      this.openAppearanceModal();
    });

    document.getElementById('chat-menu-button')?.addEventListener('click', (event) => {
      event.stopPropagation();
      this.toggleConversationMenu();
    });

    document.getElementById('chat-search-toggle')?.addEventListener('click', () => {
      this.toggleChatSearch();
      this.closeConversationMenu();
    });

    document.getElementById('chat-search-clear')?.addEventListener('click', () => {
      this.clearChatSearch();
    });

    document.getElementById('chat-search-input')?.addEventListener('input', (event) => {
      this.setChatSearchQuery(event.target.value);
    });

    document.getElementById('chat-starred-toggle')?.addEventListener('click', () => {
      this.toggleStarredOnly();
      this.closeConversationMenu();
    });

    document.getElementById('chat-translate-all')?.addEventListener('click', () => {
      this.translateUntranslatedIncomingMessages();
      this.closeConversationMenu();
    });

    // Chat settings button in header
    document.getElementById('chat-settings-button')?.addEventListener('click', () => {
      this.openSettingsModal();
      this.closeConversationMenu();
    });

    document.getElementById('chat-workspace-toggle')?.addEventListener('click', () => {
      this.toggleWorkspacePanel();
    });

    document.getElementById('workspace-backdrop')?.addEventListener('click', () => {
      this.toggleWorkspacePanel(false);
    });

    document.getElementById('workspace-settings-button')?.addEventListener('click', () => {
      this.openSettingsModal();
    });

    document.getElementById('workspace-copy-brief')?.addEventListener('click', () => {
      this.copyConversationBrief();
    });

    document.getElementById('workspace-translate-all')?.addEventListener('click', () => {
      this.translateUntranslatedIncomingMessages();
    });

    document.getElementById('workspace-checklist')?.addEventListener('click', (event) => {
      const item = event.target.closest('[data-workspace-checklist]');
      if (item) {
        this.toggleWorkspaceChecklistItem(item.dataset.workspaceChecklist);
      }
    });

    // Context menu for contacts (right-click)
    document.getElementById('contacts-list').addEventListener('contextmenu', (e) => {
      const contactItem = e.target.closest('.contact-item');
      if (contactItem) {
        e.preventDefault();
        const contactId = contactItem.dataset.contactId;
        this.showContactContextMenu(e, contactId);
      }
    });

    // Context menu items
    document.getElementById('contact-context-menu')?.addEventListener('click', (e) => {
      const item = e.target.closest('.context-menu-item');
      if (!item) return;

      const action = item.dataset.action;
      const contactId = this.contextMenuContactId;

      if (action === 'pin') {
        this.togglePin(contactId);
      } else if (action === 'settings') {
        this.openSettingsModal(contactId);
      }

      this.hideContactContextMenu();
    });

    // Close context menu when clicking outside
    document.addEventListener('click', (e) => {
      if (!e.target.closest('#contact-context-menu')) {
        this.hideContactContextMenu();
      }
    });

    // Settings modal events
    document.querySelector('#settings-modal .modal-close')?.addEventListener('click', () => {
      this.closeSettingsModal();
    });

    document.querySelector('#settings-modal .modal-backdrop')?.addEventListener('click', () => {
      this.closeSettingsModal();
    });

    document.querySelector('#appearance-modal .modal-close')?.addEventListener('click', () => {
      this.closeAppearanceModal();
    });

    document.querySelector('#appearance-modal .modal-backdrop')?.addEventListener('click', () => {
      this.closeAppearanceModal();
    });

    document.getElementById('appearance-cancel')?.addEventListener('click', () => {
      this.closeAppearanceModal();
    });

    document.getElementById('appearance-save')?.addEventListener('click', () => {
      this.saveAppearanceSettings();
    });

    document.getElementById('appearance-theme-select')?.addEventListener('change', () => {
      this.syncAppearanceControls({
        theme: document.getElementById('appearance-theme-select')?.value,
        mode: document.getElementById('appearance-mode-select')?.value,
      });
    });

    document.getElementById('appearance-mode-select')?.addEventListener('change', () => {
      this.syncAppearanceControls({
        theme: document.getElementById('appearance-theme-select')?.value,
        mode: document.getElementById('appearance-mode-select')?.value,
      });
    });

    document.getElementById('settings-cancel')?.addEventListener('click', () => {
      this.closeSettingsModal();
    });

    document.getElementById('settings-save')?.addEventListener('click', () => {
      this.saveConversationSettings();
    });

    document.getElementById('command-palette-input')?.addEventListener('input', (event) => {
      this.commandPaletteQuery = event.target.value || '';
      this.commandPaletteSelectionIndex = 0;
      this.renderCommandPalette();
    });

    document.getElementById('command-palette-results')?.addEventListener('click', (event) => {
      const item = event.target.closest('[data-command-index]');
      if (!item) return;
      this.commandPaletteSelectionIndex = Number(item.dataset.commandIndex || 0);
      this.activateCommandPaletteSelection();
    });

    document.querySelector('#command-palette .command-palette-backdrop')?.addEventListener('click', () => {
      this.closeCommandPalette();
    });

    document.addEventListener('keydown', (e) => {
      const target = e.target;
      const isEditable = target instanceof HTMLElement
        && (target.isContentEditable || ['INPUT', 'TEXTAREA', 'SELECT'].includes(target.tagName));

      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === 'k') {
        e.preventDefault();
        if (this.commandPaletteOpen) {
          this.closeCommandPalette();
        } else {
          this.openCommandPalette();
        }
        return;
      }

      if (this.commandPaletteOpen) {
        if (e.key === 'ArrowDown') {
          e.preventDefault();
          this.moveCommandPaletteSelection(1);
        } else if (e.key === 'ArrowUp') {
          e.preventDefault();
          this.moveCommandPaletteSelection(-1);
        } else if (e.key === 'Enter') {
          e.preventDefault();
          this.activateCommandPaletteSelection();
        } else if (e.key === 'Escape') {
          e.preventDefault();
          this.closeCommandPalette();
        }
        return;
      }

      if (!isEditable && e.key === '/' && !e.metaKey && !e.ctrlKey && !e.altKey) {
        e.preventDefault();
        document.getElementById('inbox-search-input')?.focus();
        document.getElementById('inbox-search-input')?.select();
        return;
      }

      if (!isEditable && e.key.toLowerCase() === 'w' && this.currentContactId) {
        e.preventDefault();
        this.toggleWorkspacePanel();
      }
    });

    // Close modal on Escape key
    document.addEventListener('keydown', (e) => {
      if (e.key === 'Escape') {
        if (this.commandPaletteOpen) {
          this.closeCommandPalette();
          return;
        }
        this.closeSettingsModal();
        this.closeAppearanceModal();
        this.hideContactContextMenu();
        if (!document.getElementById('chat-search-bar')?.classList.contains('hidden')) {
          this.toggleChatSearch();
        }
      }
    });
  }

  // Check if on mobile device
  isMobile() {
    return window.innerWidth <= 768;
  }

  // Close chat view (mobile)
  closeChat() {
    this.currentContactId = null;
    this.messageSearchQuery = '';
    this.starredOnly = false;
    this.clearReply();
    document.getElementById('chat-view').querySelector('#chat-search-bar')?.classList.add('hidden');
    const chatSearchInput = document.getElementById('chat-search-input');
    if (chatSearchInput) chatSearchInput.value = '';
    document.getElementById('main-container').classList.remove('chat-open');
    document.getElementById('chat-view').classList.add('hidden');
    document.getElementById('no-chat-selected').classList.remove('hidden');
    this.closeConversationMenu();
    if (this.isMobile()) {
      this.workspaceExpanded = false;
    }
    this.updateDraftBanner();
    this.renderQuickReplies();
    this.updateStarredToggleUI();
    this.updateChatSearchUI();
    this.updateChatHeaderNote();
    this.updateWorkspaceUI();
    this.renderContacts();
    
    // Update URL without chat parameter
    if (this.isMobile()) {
      history.replaceState({}, '', window.location.pathname);
    }
  }

  // Utility functions
  escapeHtml(text) {
    if (!text) return '';
    const div = document.createElement('div');
    div.textContent = text;
    return div.innerHTML;
  }

  formatTime(timestamp) {
    if (!timestamp) return '';
    const date = new Date(timestamp);
    const now = new Date();
    const diff = now - date;
    
    // Today
    if (date.toDateString() === now.toDateString()) {
      return date.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
    }
    
    // Yesterday
    const yesterday = new Date(now);
    yesterday.setDate(yesterday.getDate() - 1);
    if (date.toDateString() === yesterday.toDateString()) {
      return 'Yesterday';
    }
    
    // This week
    if (diff < 7 * 24 * 60 * 60 * 1000) {
      return date.toLocaleDateString([], { weekday: 'short' });
    }
    
    // Older
    return date.toLocaleDateString([], { month: 'short', day: 'numeric' });
  }

  formatMetadataTime(timestamp) {
    if (!timestamp) return '';
    const date = new Date(timestamp);
    const now = new Date();
    const diff = timestamp - now.getTime();
    const absDiff = Math.abs(diff);

    if (date.toDateString() === now.toDateString()) {
      return `today at ${date.toLocaleTimeString([], { hour: 'numeric', minute: '2-digit' })}`;
    }

    const tomorrow = new Date(now);
    tomorrow.setDate(tomorrow.getDate() + 1);
    if (date.toDateString() === tomorrow.toDateString()) {
      return `tomorrow at ${date.toLocaleTimeString([], { hour: 'numeric', minute: '2-digit' })}`;
    }

    if (absDiff < 7 * 24 * 60 * 60 * 1000) {
      return date.toLocaleString([], { weekday: 'short', hour: 'numeric', minute: '2-digit' });
    }

    return date.toLocaleString([], {
      month: 'short',
      day: 'numeric',
      hour: 'numeric',
      minute: '2-digit',
    });
  }

  toDateTimeLocalValue(timestamp) {
    if (!timestamp) return '';
    const date = new Date(timestamp);
    const offset = date.getTimezoneOffset();
    const localDate = new Date(date.getTime() - offset * 60_000);
    return localDate.toISOString().slice(0, 16);
  }

  parseDateTimeLocalValue(value) {
    if (!value) return null;
    const timestamp = new Date(value).getTime();
    return Number.isFinite(timestamp) ? timestamp : null;
  }

  formatDate(timestamp) {
    const date = new Date(timestamp);
    const now = new Date();
    
    if (date.toDateString() === now.toDateString()) {
      return 'Today';
    }
    
    const yesterday = new Date(now);
    yesterday.setDate(yesterday.getDate() - 1);
    if (date.toDateString() === yesterday.toDateString()) {
      return 'Yesterday';
    }
    
    return date.toLocaleDateString([], { 
      weekday: 'long', 
      month: 'long', 
      day: 'numeric',
      year: date.getFullYear() !== now.getFullYear() ? 'numeric' : undefined
    });
  }

  formatMessageTime(timestamp) {
    return new Date(timestamp).toLocaleTimeString([], { 
      hour: '2-digit', 
      minute: '2-digit' 
    });
  }

  formatSize(bytes) {
    if (bytes < 1024) return bytes + ' B';
    if (bytes < 1024 * 1024) return (bytes / 1024).toFixed(1) + ' KB';
    return (bytes / (1024 * 1024)).toFixed(1) + ' MB';
  }

  formatDuration(seconds) {
    const mins = Math.floor(seconds / 60);
    const secs = seconds % 60;
    return `${mins}:${secs.toString().padStart(2, '0')}`;
  }

  // Format cost for display
  formatCost(costUsd) {
    if (costUsd < 0.01) {
      return '$' + costUsd.toFixed(4);
    }
    return '$' + costUsd.toFixed(2);
  }

  // Fetch and display global usage
  async fetchGlobalUsage() {
    if (this.demoMode) {
      this.updateGlobalUsageDisplay();
      return;
    }

    try {
      const response = await this.apiFetch('/api/usage');
      const usage = await response.json();
      this.globalUsage = usage;
      this.updateGlobalUsageDisplay();
    } catch (err) {
      console.error('Failed to fetch global usage:', err);
    }
  }

  // Update global usage display in sidebar
  updateGlobalUsageDisplay() {
    const costEl = document.getElementById('global-cost');
    if (costEl) {
      costEl.textContent = this.formatCost(this.globalUsage.costUsd || 0);
    }
  }

  // Fetch and display conversation usage
  async fetchConversationUsage(contactId) {
    if (this.demoMode) {
      const messages = this.messages.get(contactId) || [];
      const translatedCount = messages.filter(message => message.isTranslated || message.is_translated).length;
      const usage = { costUsd: translatedCount * 0.004 };
      this.conversationUsageCache.set(contactId, usage);
      if (contactId === this.currentContactId) {
        this.updateConversationUsageDisplay(usage);
      }
      return;
    }

    try {
      const response = await this.apiFetch(`/api/usage/${encodeURIComponent(contactId)}`, {
        headers: this.getAuthHeaders()
      });
      const usage = await response.json();
      this.conversationUsageCache.set(contactId, usage);
      if (contactId === this.currentContactId) {
        this.updateConversationUsageDisplay(usage);
      }
    } catch (err) {
      console.error('Failed to fetch conversation usage:', err);
    }
  }

  // Update conversation usage display in chat header
  updateConversationUsageDisplay(usage) {
    document.querySelectorAll('[data-chat-cost]').forEach((costEl) => {
      costEl.textContent = this.formatCost(usage.costUsd || 0);
    });
  }

  // Extract URLs from text
  extractUrls(text) {
    if (!text) return [];
    const urlRegex = /https?:\/\/[^\s<>\[\](){}|\\^`\x00-\x1f\x7f]+/gi;
    const matches = text.match(urlRegex) || [];
    
    // Clean trailing punctuation
    return matches.map(url => {
      while (url.match(/[.,!?)\]};:'"]+$/)) {
        url = url.slice(0, -1);
      }
      return url;
    });
  }

  // Fetch link preview from API
  async fetchLinkPreview(url) {
    // Check memory cache first
    if (this.linkPreviewCache.has(url)) {
      return this.linkPreviewCache.get(url);
    }

    // Skip if already fetching
    if (this.linkPreviewFetching.has(url)) {
      return null;
    }

    this.linkPreviewFetching.add(url);

    try {
      const response = await this.apiFetch(`/api/link-preview?url=${encodeURIComponent(url)}`);
      const preview = await response.json();
      
      // Cache the result
      this.linkPreviewCache.set(url, preview);
      
      return preview;
    } catch (err) {
      console.error('Failed to fetch link preview:', err);
      return null;
    } finally {
      this.linkPreviewFetching.delete(url);
    }
  }

  // Render link preview card HTML
  renderLinkPreviewCard(preview, url) {
    if (!preview || preview.error) {
      return ''; // Don't show card for errors
    }

    const hasImage = preview.imageUrl && !preview.imageUrl.includes('undefined');
    const title = preview.title || this.getDomainFromUrl(url);
    const description = preview.description || '';
    const siteName = preview.siteName || this.getDomainFromUrl(url);

    return `
      <a href="${this.escapeHtml(url)}" target="_blank" rel="noopener noreferrer" class="link-preview-card">
        ${hasImage ? `
          <div class="link-preview-image">
            <img src="${this.escapeHtml(preview.imageUrl)}" alt="" loading="lazy" onerror="this.parentElement.style.display='none'">
          </div>
        ` : ''}
        <div class="link-preview-content">
          <div class="link-preview-site">${this.escapeHtml(siteName)}</div>
          <div class="link-preview-title">${this.escapeHtml(title)}</div>
          ${description ? `<div class="link-preview-description">${this.escapeHtml(description)}</div>` : ''}
        </div>
      </a>
    `;
  }

  // Get domain from URL for display
  getDomainFromUrl(url) {
    try {
      const urlObj = new URL(url);
      return urlObj.hostname.replace(/^www\./, '');
    } catch {
      return url;
    }
  }

  // Convert URLs in text to clickable links and apply WhatsApp markdown formatting
  linkifyText(text) {
    if (!text) return '';
    
    const urlRegex = /(https?:\/\/[^\s<>\[\](){}|\\^`\x00-\x1f\x7f]+)/gi;
    const parts = text.split(urlRegex);
    
    return parts.map((part, index) => {
      // Even indices are non-URL text, odd indices are URLs (due to capture group)
      if (index % 2 === 0) {
        // Non-URL text - escape it and apply WhatsApp formatting
        return this.formatWhatsAppMarkdown(this.escapeHtml(part));
      } else {
        // URL - clean trailing punctuation and create link
        let cleanUrl = part;
        let trailing = '';
        while (cleanUrl.match(/[.,!?)\]};:'"]+$/)) {
          trailing = cleanUrl.slice(-1) + trailing;
          cleanUrl = cleanUrl.slice(0, -1);
        }
        return `<a href="${this.escapeHtml(cleanUrl)}" target="_blank" rel="noopener noreferrer" class="message-link">${this.escapeHtml(cleanUrl)}</a>${this.escapeHtml(trailing)}`;
      }
    }).join('');
  }

  // Apply WhatsApp-style markdown formatting
  // *bold* _italic_ ~strikethrough~ ```code block``` `inline code`
  formatWhatsAppMarkdown(text) {
    if (!text) return '';
    
    // Process code blocks first (```text```) - these should not have other formatting inside
    text = text.replace(/```([\s\S]*?)```/g, '<code class="wa-code-block">$1</code>');
    
    // Process inline code (`text`)
    text = text.replace(/`([^`\n]+)`/g, '<code class="wa-code-inline">$1</code>');
    
    // Process bold (*text*) - must not be inside a code block
    // Match * followed by non-whitespace, any chars, non-whitespace, then *
    text = text.replace(/\*([^\s*](?:[^*]*[^\s*])?)\*/g, '<strong>$1</strong>');
    
    // Process italic (_text_) - must not be inside a code block
    text = text.replace(/(?<![a-zA-Z0-9])_([^\s_](?:[^_]*[^\s_])?)_(?![a-zA-Z0-9])/g, '<em>$1</em>');
    
    // Process strikethrough (~text~)
    text = text.replace(/~([^\s~](?:[^~]*[^\s~])?)~/g, '<s>$1</s>');
    
    // Convert newlines to <br>
    text = text.replace(/\n/g, '<br>');
    
    return text;
  }

  // Load link previews for a message element
  async loadLinkPreviews(messageEl, urls) {
    const container = messageEl.querySelector('.link-previews-container');
    if (!container || urls.length === 0) return;
    if (container.dataset.previewLoaded === 'true' || container.dataset.previewLoading === 'true') return;

    container.dataset.previewLoading = 'true';

    for (const url of urls) {
      const preview = await this.fetchLinkPreview(url);
      if (preview && !preview.error) {
        const cardHtml = this.renderLinkPreviewCard(preview, url);
        if (cardHtml) {
          container.insertAdjacentHTML('beforeend', cardHtml);
        }
      }
    }

    container.dataset.previewLoading = 'false';
    container.dataset.previewLoaded = 'true';
  }

  // Load media on demand (lazy loading)
  async loadMedia(messageId, placeholderEl) {
    // Find the lazy-media container
    const container = placeholderEl.closest('.lazy-media');
    if (!container) {
      console.error('Could not find lazy-media container');
      return;
    }

    // Get media type info from the container
    const mediaType = container.dataset.mediaType;
    const mimeType = container.dataset.mimeType;
    const fileName = container.dataset.fileName;

    // Show loading state
    placeholderEl.classList.add('loading');
    placeholderEl.innerHTML = `
      <div class="media-loading-spinner"></div>
      <span>Loading...</span>
    `;

    try {
      // Fetch media from the API
      const response = await this.apiFetch(`/api/media/${encodeURIComponent(messageId)}`);
      
      if (!response.ok) {
        throw new Error('Failed to load media');
      }

      const data = await response.json();
      
      if (!data.media_data) {
        throw new Error('No media data received');
      }

      // Build the data URL
      const actualMimeType = data.mime_type || mimeType;
      const mediaSrc = data.media_data.startsWith('data:') 
        ? data.media_data 
        : `data:${actualMimeType};base64,${data.media_data}`;

      // Replace placeholder with actual media based on type
      let mediaHtml = '';
      
      switch (mediaType) {
        case 'image':
          mediaHtml = `<img src="${mediaSrc}" alt="Image" loading="lazy" onclick="this.classList.toggle('fullscreen')">`;
          break;
          
        case 'video':
          mediaHtml = `
            <video controls preload="metadata" onclick="event.stopPropagation()">
              <source src="${mediaSrc}" type="${actualMimeType}">
              Your browser does not support video playback.
            </video>
          `;
          break;
          
        case 'audio':
          const isVoiceNote = container.classList.contains('voice-note');
          mediaHtml = `
            <audio controls preload="metadata">
              <source src="${mediaSrc}" type="${actualMimeType}">
              Your browser does not support audio playback.
            </audio>
            ${isVoiceNote ? '<span class="voice-note-label">Voice Note</span>' : ''}
          `;
          break;
          
        case 'document':
          const docFileName = fileName || 'document';
          mediaHtml = `
            <a href="${mediaSrc}" download="${this.escapeHtml(docFileName)}" class="document-download">
              <svg viewBox="0 0 24 24" width="24" height="24">
                <path fill="currentColor" d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8l-6-6zm4 18H6V4h7v5h5v11z"/>
              </svg>
              <div class="document-info">
                <span class="document-name">${this.escapeHtml(docFileName)}</span>
                <span class="document-size">Click to download</span>
              </div>
            </a>
          `;
          break;
          
        case 'sticker':
          mediaHtml = `<img src="${mediaSrc}" alt="Sticker" loading="lazy">`;
          break;
          
        default:
          mediaHtml = `<img src="${mediaSrc}" alt="Media" loading="lazy">`;
      }

      // Remove lazy-media class and replace content
      container.classList.remove('lazy-media');
      container.innerHTML = mediaHtml;

      // Also update the message cache so re-renders show the media
      this.updateMessageMediaCache(messageId, data.media_data, actualMimeType);

    } catch (err) {
      console.error('Failed to load media:', err);
      // Show error state
      placeholderEl.classList.remove('loading');
      placeholderEl.classList.add('error');
      placeholderEl.innerHTML = `
        <svg viewBox="0 0 24 24" width="32" height="32">
          <path fill="currentColor" d="M12 2C6.48 2 2 6.48 2 12s4.48 10 10 10 10-4.48 10-10S17.52 2 12 2zm1 15h-2v-2h2v2zm0-4h-2V7h2v6z"/>
        </svg>
        <span>Failed to load. Click to retry.</span>
      `;
      // Allow retry on click
      placeholderEl.onclick = () => this.loadMedia(messageId, placeholderEl);
    }
  }

  // Update message cache with loaded media data
  updateMessageMediaCache(messageId, mediaData, mimeType) {
    // Find the message in the current contact's messages
    if (!this.currentContactId) return;
    
    const messages = this.messages.get(this.currentContactId);
    if (!messages) return;
    
    const message = messages.find(m => m.id === messageId);
    if (message && message.content) {
      message.content.media_data = mediaData;
      message.content.mediaData = mediaData;
      message.content.mime_type = mimeType;
      message.content.mimeType = mimeType;
      // Remove has_media flag since we now have the data
      delete message.content.has_media;
      delete message.content.hasMedia;
    }
  }

  // Render emojis for a category
  renderEmojiCategory(category) {
    const container = document.getElementById('emoji-picker-content');
    if (!container) return;

    let emojis;
    if (category === 'recent') {
      emojis = this.recentEmojis.length > 0 
        ? this.recentEmojis 
        : ['😀', '😊', '😂', '❤️', '👍', '🎉', '🔥', '✨']; // Default if no recent
    } else {
      emojis = this.emojiData[category] || [];
    }

    container.innerHTML = emojis
      .map(emoji => `<span class="emoji-item">${emoji}</span>`)
      .join('');
  }

  // Insert emoji at cursor position in message input
  insertEmoji(emoji) {
    const input = document.getElementById('message-input');
    if (!input) return;

    const start = input.selectionStart;
    const end = input.selectionEnd;
    const text = input.value;

    // Insert emoji at cursor position
    input.value = text.substring(0, start) + emoji + text.substring(end);

    // Move cursor after the inserted emoji
    const newCursorPos = start + emoji.length;
    input.setSelectionRange(newCursorPos, newCursorPos);

    // Focus the input
    input.focus();

    // Trigger input event to update send button state
    input.dispatchEvent(new Event('input', { bubbles: true }));

    // Close the picker
    document.getElementById('emoji-picker')?.classList.add('hidden');
  }

  // Add emoji to recent list
  addRecentEmoji(emoji) {
    // Remove if already exists
    this.recentEmojis = this.recentEmojis.filter(e => e !== emoji);
    // Add to front
    this.recentEmojis.unshift(emoji);
    // Keep only last 32 emojis
    this.recentEmojis = this.recentEmojis.slice(0, 32);
    // Save to localStorage
    localStorage.setItem('wa_recent_emojis', JSON.stringify(this.recentEmojis));
  }

  // ==================== Conversation Settings ====================

  // Track contact ID for context menu
  contextMenuContactId = null;

  // Show context menu for a contact
  showContactContextMenu(event, contactId) {
    const menu = document.getElementById('contact-context-menu');
    if (!menu) return;

    this.contextMenuContactId = contactId;

    // Update pin button text based on current state
    const contact = this.contacts.find(c => c.id === contactId);
    const pinText = menu.querySelector('.pin-text');
    if (pinText && contact) {
      pinText.textContent = contact.pinnedAt ? 'Unpin conversation' : 'Pin conversation';
    }

    // Position the menu near the cursor
    menu.style.left = `${event.clientX}px`;
    menu.style.top = `${event.clientY}px`;
    menu.classList.remove('hidden');

    // Ensure menu stays within viewport
    const rect = menu.getBoundingClientRect();
    if (rect.right > window.innerWidth) {
      menu.style.left = `${window.innerWidth - rect.width - 10}px`;
    }
    if (rect.bottom > window.innerHeight) {
      menu.style.top = `${window.innerHeight - rect.height - 10}px`;
    }
  }

  // Hide context menu
  hideContactContextMenu() {
    const menu = document.getElementById('contact-context-menu');
    if (menu) {
      menu.classList.add('hidden');
    }
    this.contextMenuContactId = null;
  }

  // Open settings modal for current contact
  async openSettingsModal(contactId = this.currentContactId) {
    if (!contactId) {
      console.warn('No contact selected');
      return;
    }

    const modal = document.getElementById('settings-modal');
    if (!modal) return;

    this.settingsContactId = contactId;
    let settings = {};
    try {
      const response = await this.apiFetch(`/api/contacts/${encodeURIComponent(contactId)}/settings`, {
        headers: this.getAuthHeaders()
      });
      if (response.ok) {
        settings = await response.json();
      }
    } catch (err) {
      console.warn('Failed to fetch conversation settings, falling back to local settings only:', err);
    }

    const localSettings = this.getContactMetadata(contactId);
    const mergedSettings = {
      ...settings,
      ...localSettings,
      alias: localSettings.alias || '',
      notes: localSettings.notes || '',
      pinnedAt: localSettings.pinnedAt || null,
      priority: this.getPriorityInfo(contactId).value,
      timezone: localSettings.timezone || '',
      checklist: localSettings.checklist || [],
      reminderText: localSettings.reminderText || '',
      reminderAt: localSettings.reminderAt || null,
      snoozedUntil: localSettings.snoozedUntil || null,
      labels: this.getContactLabels(contactId),
    };

    document.getElementById('contact-alias').value = mergedSettings.alias || '';
    document.getElementById('conversation-priority').value = mergedSettings.priority || 'normal';
    document.getElementById('conversation-timezone').value = mergedSettings.timezone || '';
    document.getElementById('language-override').value = mergedSettings.languageOverride || '';
    document.getElementById('translation-style').value = mergedSettings.translationStyle || '';
    document.getElementById('conversation-notes').value = mergedSettings.notes || '';
    document.getElementById('conversation-checklist').value = this.formatChecklistForTextarea(contactId);
    document.getElementById('conversation-labels').value = (mergedSettings.labels || []).join(', ');
    document.getElementById('conversation-reminder-text').value = mergedSettings.reminderText || '';
    document.getElementById('conversation-reminder-at').value = mergedSettings.reminderAt ? this.toDateTimeLocalValue(mergedSettings.reminderAt) : '';
    document.getElementById('conversation-snooze-until').value = mergedSettings.snoozedUntil ? this.toDateTimeLocalValue(mergedSettings.snoozedUntil) : '';
    const pinToggle = document.getElementById('settings-pinned');
    if (pinToggle) {
      pinToggle.checked = Boolean(mergedSettings.pinnedAt);
    }

    modal.classList.remove('hidden');
  }

  updateChatHeaderNote() {
    const noteEl = document.getElementById('chat-note-indicator');
    if (!noteEl) return;
    const summary = this.currentContactId ? this.buildChatMetadataSummary(this.currentContactId) : '';
    if (summary) {
      noteEl.textContent = summary;
      noteEl.classList.remove('hidden');
    } else {
      noteEl.textContent = '';
      noteEl.classList.add('hidden');
    }
  }

  // Close settings modal
  closeSettingsModal() {
    this.settingsContactId = null;
    const modal = document.getElementById('settings-modal');
    if (modal) {
      modal.classList.add('hidden');
    }
  }

  // Save conversation settings
  async saveConversationSettings() {
    const targetContactId = this.settingsContactId || this.currentContactId;
    if (!targetContactId) return;

    const languageOverride = document.getElementById('language-override')?.value?.trim() || null;
    const translationStyle = document.getElementById('translation-style')?.value?.trim() || null;
    const alias = document.getElementById('contact-alias')?.value?.trim() || null;
    const priority = document.getElementById('conversation-priority')?.value || 'normal';
    const timezone = document.getElementById('conversation-timezone')?.value?.trim() || null;
    const notes = document.getElementById('conversation-notes')?.value?.trim() || null;
    const checklistText = document.getElementById('conversation-checklist')?.value || '';
    const existingChecklist = this.getContactMetadata(targetContactId).checklist || [];
    const checklist = upsertChecklistItems(existingChecklist, checklistText);
    const labels = parseLabelsInput(document.getElementById('conversation-labels')?.value || '');
    const reminderText = document.getElementById('conversation-reminder-text')?.value?.trim() || null;
    const reminderAt = this.parseDateTimeLocalValue(document.getElementById('conversation-reminder-at')?.value || '');
    const snoozedUntil = this.parseDateTimeLocalValue(document.getElementById('conversation-snooze-until')?.value || '');
    if ((reminderText && !reminderAt) || (!reminderText && reminderAt)) {
      alert('To save a follow-up reminder, add both the reminder text and the reminder time.');
      return;
    }
    const pinned = Boolean(document.getElementById('settings-pinned')?.checked);
    const pinnedAt = pinned ? (this.getContactMetadata(targetContactId).pinnedAt || Date.now()) : null;

    this.updateContactMetadata(targetContactId, {
      alias,
      languageOverride,
      targetLanguage: languageOverride,
      translationStyle,
      priority,
      timezone,
      notes,
      notePreview: notes || null,
      pinnedAt,
      checklist,
      labels,
      labelsText: labels.join(', '),
      reminderText: reminderText && reminderAt ? reminderText : null,
      reminderAt: reminderText && reminderAt ? reminderAt : null,
      snoozedUntil,
    });

    const contact = this.contacts.find(item => item.id === targetContactId);
    if (contact) {
      contact.alias = alias;
      contact.languageOverride = languageOverride;
      contact.targetLanguage = languageOverride;
      contact.translationStyle = translationStyle;
      contact.priority = priority;
      contact.timezone = timezone;
      contact.notes = notes;
      contact.notePreview = notes || null;
      contact.pinnedAt = pinnedAt;
      contact.checklist = checklist;
      contact.labels = labels;
      contact.reminderText = reminderText && reminderAt ? reminderText : null;
      contact.reminderAt = reminderText && reminderAt ? reminderAt : null;
      contact.snoozedUntil = snoozedUntil;
    }

    try {
      const response = await this.apiFetch(`/api/contacts/${encodeURIComponent(targetContactId)}/settings`, {
        method: 'PUT',
        headers: {
          'Content-Type': 'application/json',
          ...this.getAuthHeaders()
        },
        body: JSON.stringify({
          languageOverride: languageOverride || null,
          translationStyle: translationStyle || null
        })
      });

      if (!response.ok) {
        throw new Error('Failed to save settings');
      }

      if (contact) {
        contact.languageOverride = languageOverride;
        contact.translationStyle = translationStyle;
      }
    } catch (err) {
      console.error('Failed to save conversation settings:', err);
      alert('Saved local labels, reminders, snooze state, and notes, but failed to save translation settings. Please try again.');
      return;
    }

    this.closeSettingsModal();
    this.syncInboxControls();
    this.renderContacts();
    this.updateChatHeaderNote();
    this.renderConversationWorkspace();
    this.updateConversationMenuUI();
  }
}

/* Initialize app */
window.app = new WhatsAppClient();
