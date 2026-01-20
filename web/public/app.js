// WhatsApp Translator Web Client

class WhatsAppClient {
  constructor() {
    this.ws = null;
    this.connected = false;
    this.contacts = [];
    this.currentContactId = null;
    this.messages = new Map();
    this.avatarCache = new Map(); // JID -> URL
    this.avatarFetching = new Set(); // JIDs currently being fetched
    this.globalUsage = { inputTokens: 0, outputTokens: 0, costUsd: 0 };
    this.linkPreviewCache = new Map(); // URL -> LinkPreview
    this.linkPreviewFetching = new Set(); // URLs currently being fetched
    this.typingState = new Map(); // chatId -> { userId, state, timestamp }
    this.typingTimeouts = new Map(); // chatId -> timeoutId (auto-clear after 10s)
    
    this.init();
  }

  init() {
    this.connectWebSocket();
    this.bindEvents();
    this.updateInputPlaceholder();
  }

  // Update placeholder to show correct keyboard shortcut for OS
  updateInputPlaceholder() {
    const input = document.getElementById('message-input');
    if (input) {
      const isMac = navigator.platform.toUpperCase().indexOf('MAC') >= 0;
      const shortcut = isMac ? '⌘+Enter' : 'Ctrl+Enter';
      input.placeholder = `Type a message (${shortcut} to send)`;
    }
  }

  // WebSocket connection
  connectWebSocket() {
    const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
    const wsUrl = `${protocol}//${window.location.host}/ws`;
    
    this.ws = new WebSocket(wsUrl);
    
    this.ws.onopen = () => {
      console.log('WebSocket connected');
    };
    
    this.ws.onmessage = (event) => {
      const data = JSON.parse(event.data);
      this.handleMessage(data);
    };
    
    this.ws.onclose = () => {
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
      
      case 'error':
        console.error('Error:', data.error);
        break;
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
    this.connected = false;
    
    const statusDot = document.querySelector('.status-dot');
    statusDot.classList.remove('connected');
    
    this.showConnecting();
  }

  // Handle new message
  handleNewMessage(message) {
    // Add to local cache
    if (!this.messages.has(message.contactId)) {
      this.messages.set(message.contactId, []);
    }
    
    const messages = this.messages.get(message.contactId);
    if (!messages.some(m => m.id === message.id)) {
      messages.push(message);
      messages.sort((a, b) => a.timestamp - b.timestamp);
    }
    
    // Update contact in list
    this.updateContactInList(message);
    
    // If this contact is currently selected, show the message
    if (this.currentContactId === message.contactId) {
      this.appendMessage(message);
      this.scrollToBottom();
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
      contact = {
        id: message.contactId,
        name: message.contactName,
        phone: message.contactPhone,
        type: message.chatType,
        lastMessageTime: message.timestamp,
        unreadCount: 0
      };
      this.contacts.push(contact);
    } else {
      contact.lastMessageTime = Math.max(contact.lastMessageTime, message.timestamp);
      if (message.contactName && message.contactName !== message.contactPhone) {
        contact.name = message.contactName;
      }
    }
    
    // Increment unread if not from me and not currently viewing
    if (!message.isFromMe && this.currentContactId !== message.contactId) {
      contact.unreadCount = (contact.unreadCount || 0) + 1;
    }
    
    // Re-render contacts list
    this.renderContacts();
  }

  // Load contacts from server
  async loadContacts() {
    try {
      const response = await fetch('/api/contacts');
      this.contacts = await response.json();
      this.renderContacts();
      
      // Fetch avatars for all contacts in the background
      this.contacts.forEach(contact => {
        this.fetchAvatar(contact.id);
      });
    } catch (err) {
      console.error('Failed to load contacts:', err);
    }
  }

  // Fetch avatar for a contact
  async fetchAvatar(jid) {
    // Skip if already cached or being fetched
    if (this.avatarCache.has(jid) || this.avatarFetching.has(jid)) {
      return;
    }

    this.avatarFetching.add(jid);

    try {
      const response = await fetch(`/api/avatar/${encodeURIComponent(jid)}`);
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
    
    if (this.contacts.length === 0) {
      container.innerHTML = `
        <div class="empty-state">
          <p>No conversations yet</p>
          <p class="hint">Messages will appear here</p>
        </div>
      `;
      return;
    }
    
    // Sort by last message time
    const sorted = [...this.contacts].sort((a, b) => b.lastMessageTime - a.lastMessageTime);
    
    container.innerHTML = sorted.map(contact => {
      const initial = (contact.name || contact.phone || '?').charAt(0).toUpperCase();
      const time = this.formatTime(contact.lastMessageTime);
      const isActive = contact.id === this.currentContactId;
      const unread = contact.unreadCount > 0 ? 
        `<span class="unread-badge">${contact.unreadCount}</span>` : '';
      
      // Get last message preview
      const messages = this.messages.get(contact.id) || [];
      const lastMessage = messages[messages.length - 1];
      const preview = lastMessage ? this.getMessagePreview(lastMessage) : '';
      
      // Check for cached avatar
      const avatarUrl = this.avatarCache.get(contact.id);
      const avatarContent = avatarUrl 
        ? `<img src="${avatarUrl}" alt="" onerror="this.parentElement.innerHTML='<span>${initial}</span>'">`
        : `<span>${initial}</span>`;
      
      return `
        <div class="contact-item ${isActive ? 'active' : ''}" data-contact-id="${contact.id}">
          <div class="avatar">
            ${avatarContent}
          </div>
          <div class="contact-details">
            <div class="contact-header">
              <span class="contact-name">${this.escapeHtml(contact.name || contact.phone || 'Unknown')}</span>
              <span class="contact-time">${time}</span>
            </div>
            <div class="contact-preview">
              <span class="preview-text">${this.escapeHtml(preview)}</span>
              ${unread}
            </div>
          </div>
        </div>
      `;
    }).join('');
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

  // Select a contact
  async selectContact(contactId) {
    try {
      this.currentContactId = contactId;
      
      // Mark as read
      const contact = this.contacts.find(c => c.id === contactId);
      if (contact) {
        contact.unreadCount = 0;
      }
      
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
        document.getElementById('chat-name').textContent = contact.name || contact.phone || 'Unknown';
        document.getElementById('chat-phone').textContent = contact.phone ? '+' + contact.phone : '';
        
        const initial = (contact.name || contact.phone || '?').charAt(0).toUpperCase();
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
      
      // Load messages
      await this.loadMessages(contactId);
      
      // Load conversation usage
      this.fetchConversationUsage(contactId);
      
      // Re-render contacts to update active state and unread
      this.renderContacts();
      
      // Update send button state and focus input (only on desktop)
      this.updateSendButton();
      if (!this.isMobile()) {
        document.getElementById('message-input').focus();
      }
    } catch (err) {
      console.error('Error selecting contact:', err);
    }
  }

  // Load messages for a contact
  async loadMessages(contactId) {
    try {
      const response = await fetch(`/api/messages/${encodeURIComponent(contactId)}`);
      const messages = await response.json();
      this.messages.set(contactId, messages);
      this.renderMessages(messages);
    } catch (err) {
      console.error('Failed to load messages:', err);
    }
  }

  // Render messages
  renderMessages(messages) {
    const container = document.getElementById('messages-list');
    
    if (messages.length === 0) {
      container.innerHTML = '<div class="empty-state"><p>No messages yet</p></div>';
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
    
    // Load link previews for all messages
    this.loadAllLinkPreviews();
  }

  // Load link previews for all messages in the current view
  loadAllLinkPreviews() {
    const containers = document.querySelectorAll('.link-previews-container[data-urls]');
    containers.forEach(container => {
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
    const senderJid = message.senderPhone || message.sender_phone || '';
    const contactId = message.contactId || message.contact_id || this.currentContactId;
    
    // Reaction button with quick emoji picker
    const reactionButton = `
      <div class="reaction-button-container">
        <button class="reaction-button" onclick="event.stopPropagation(); this.parentElement.querySelector('.reaction-picker').classList.toggle('show');" title="React">
          <svg viewBox="0 0 24 24" width="16" height="16"><path fill="currentColor" d="M12 2C6.48 2 2 6.48 2 12s4.48 10 10 10 10-4.48 10-10S17.52 2 12 2zm0 18c-4.41 0-8-3.59-8-8s3.59-8 8-8 8 3.59 8 8-3.59 8-8 8zm-5-6c.78 2.34 2.72 4 5 4s4.22-1.66 5-4H7zm2-3c.55 0 1-.45 1-1s-.45-1-1-1-1 .45-1 1 .45 1 1 1zm6 0c.55 0 1-.45 1-1s-.45-1-1-1-1 .45-1 1 .45 1 1 1z"/></svg>
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
    
    return `
      <div class="message ${isOutgoing ? 'outgoing' : 'incoming'}" data-message-id="${messageId}">
        ${forwarded}
        ${sender}
        ${content}
        <div class="message-footer">
          ${translationIndicator}
          <span class="message-time">${time}</span>
          ${reactionButton}
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
    // - Outgoing: show content.body (English - what I typed)
    // - Incoming translated: show translated_text (English translation of what they sent)
    // - Incoming non-translated: show content.body (already English)
    let displayText;
    if (isTranslated && !isFromMe) {
      // Incoming translated: show the English translation
      displayText = message.translated_text || message.translatedText || content.body || content.text || '';
    } else {
      // Outgoing (translated or not) or incoming non-translated: show content.body
      displayText = content.body || content.text || '';
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
        // Check if we have the actual image data
        const mediaData = content.media_data || content.mediaData;
        if (mediaData) {
          const mimeType = content.mime_type || content.mimeType || 'image/jpeg';
          const imgSrc = mediaData.startsWith('data:') ? mediaData : `data:${mimeType};base64,${mediaData}`;
          return `
            <div class="message-image">
              <img src="${imgSrc}" alt="Image" loading="lazy" onclick="this.classList.toggle('fullscreen')">
            </div>
            ${content.caption ? `<div class="message-caption">${this.escapeHtml(content.caption)}</div>` : ''}
          `;
        } else {
          // Fallback for images without data
          return `
            <div class="message-media image">[ Image ]${content.file_size ? ' - ' + this.formatSize(content.file_size) : ''}</div>
            ${content.caption ? `<div class="message-caption">${this.escapeHtml(content.caption)}</div>` : ''}
          `;
        }
      
      case 'video':
        return `
          <div class="message-media video">[ Video ]${content.duration ? ' - ' + this.formatDuration(content.duration) : ''}</div>
          ${content.caption ? `<div class="message-caption">${this.escapeHtml(content.caption)}</div>` : ''}
        `;
      
      case 'audio':
        const audioType = content.isVoiceNote ? 'Voice Note' : 'Audio';
        return `<div class="message-media audio">[ ${audioType} ]${content.duration ? ' - ' + this.formatDuration(content.duration) : ''}</div>`;
      
      case 'document':
        return `
          <div class="message-media document">[ Document: ${this.escapeHtml(content.fileName || 'file')} ]</div>
          ${content.caption ? `<div class="message-caption">${this.escapeHtml(content.caption)}</div>` : ''}
        `;
      
      case 'sticker':
        return `<div class="message-media">[ ${content.animated ? 'Animated ' : ''}Sticker ]</div>`;
      
      case 'location':
        const locName = content.name || content.address || 'Location';
        return `<div class="message-media">[ Location: ${this.escapeHtml(locName)} ]</div>`;
      
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
    
    // Load link previews for the new message
    const newMessage = container.lastElementChild;
    const previewContainer = newMessage?.querySelector('.link-previews-container[data-urls]');
    if (previewContainer) {
      try {
        const urls = JSON.parse(previewContainer.dataset.urls);
        if (urls && urls.length > 0) {
          this.loadLinkPreviews(newMessage, urls);
        }
      } catch (e) {
        console.error('Failed to parse URLs:', e);
      }
    }
  }

  // Scroll to bottom of messages
  scrollToBottom() {
    const container = document.getElementById('messages-list');
    container.scrollTop = container.scrollHeight;
  }

  // Send a message
  async sendMessage() {
    const input = document.getElementById('message-input');
    const text = input.value.trim();
    
    if (!text || !this.currentContactId) return;
    
    const sendButton = document.getElementById('send-button');
    sendButton.disabled = true;
    
    try {
      const response = await fetch('/api/send', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          contactId: this.currentContactId,
          text: text
        })
      });
      
      const result = await response.json();
      
      if (!response.ok) {
        throw new Error(result.error || 'Failed to send message');
      }
      
      // Clear input on success
      input.value = '';
      this.updateSendButton();
      this.autoResizeTextarea(input);
      
      // Create a local message representation with translation info from response
      const localMessage = {
        id: result.messageId || 'temp-' + Date.now(),
        timestamp: result.timestamp || Date.now(),
        contactId: this.currentContactId,
        isFromMe: true,
        isForwarded: false,
        content: { type: 'text', body: text },
        // Include translation info if the message was translated
        isTranslated: result.isTranslated || false,
        originalText: result.isTranslated ? text : null,  // What user typed (English)
        translatedText: result.translatedText || null,     // What was sent (foreign language)
        sourceLanguage: result.sourceLanguage || null      // Target language
      };
      
      // Add to local store and display
      if (!this.messages.has(this.currentContactId)) {
        this.messages.set(this.currentContactId, []);
      }
      
      const messages = this.messages.get(this.currentContactId);
      if (!messages.some(m => m.id === localMessage.id)) {
        messages.push(localMessage);
        this.appendMessage(localMessage);
        this.scrollToBottom();
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
    if (!file.type.startsWith('image/')) {
      alert('Please select an image file.');
      return;
    }

    const attachButton = document.getElementById('attach-button');
    attachButton.disabled = true;

    try {
      // Read file as base64
      const mediaData = await this.fileToBase64(file);
      
      const response = await fetch('/api/send-image', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          contactId: this.currentContactId,
          mediaData: mediaData,
          mimeType: file.type,
          caption: null
        })
      });

      const result = await response.json();

      if (!response.ok) {
        throw new Error(result.error || 'Failed to send image');
      }

      // Create a local message representation
      const localMessage = {
        id: result.messageId || 'temp-img-' + Date.now(),
        timestamp: result.timestamp || Date.now(),
        contactId: this.currentContactId,
        isFromMe: true,
        isForwarded: false,
        content: { 
          type: 'image', 
          mime_type: file.type,
          media_data: mediaData
        }
      };

      // Add to local store and display
      if (!this.messages.has(this.currentContactId)) {
        this.messages.set(this.currentContactId, []);
      }

      const messages = this.messages.get(this.currentContactId);
      if (!messages.some(m => m.id === localMessage.id)) {
        messages.push(localMessage);
        this.appendMessage(localMessage);
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
      const response = await fetch('/api/react', {
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

      console.log('Reaction sent successfully');
    } catch (err) {
      console.error('Failed to send reaction:', err);
      alert('Failed to send reaction: ' + err.message);
    }
  }

  // Update send button state
  updateSendButton() {
    const input = document.getElementById('message-input');
    const sendButton = document.getElementById('send-button');
    sendButton.disabled = !input.value.trim() || !this.currentContactId;
  }

  // Auto-resize textarea (expands up to max-height)
  autoResizeTextarea(textarea) {
    textarea.style.height = 'auto';
    textarea.style.height = Math.min(textarea.scrollHeight, 150) + 'px';
  }

  // Bind UI events
  bindEvents() {
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

    // Message input
    const input = document.getElementById('message-input');
    const sendButton = document.getElementById('send-button');

    // Update send button state on input
    input.addEventListener('input', () => {
      this.updateSendButton();
      this.autoResizeTextarea(input);
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

    // Close reaction pickers when clicking outside
    document.addEventListener('click', (e) => {
      if (!e.target.closest('.reaction-button-container')) {
        document.querySelectorAll('.reaction-picker.show').forEach(el => el.classList.remove('show'));
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
    document.getElementById('main-container').classList.remove('chat-open');
    document.getElementById('chat-view').classList.add('hidden');
    document.getElementById('no-chat-selected').classList.remove('hidden');
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
    try {
      const response = await fetch('/api/usage');
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
    try {
      const response = await fetch(`/api/usage/${encodeURIComponent(contactId)}`);
      const usage = await response.json();
      this.updateConversationUsageDisplay(usage);
    } catch (err) {
      console.error('Failed to fetch conversation usage:', err);
    }
  }

  // Update conversation usage display in chat header
  updateConversationUsageDisplay(usage) {
    const costEl = document.getElementById('chat-cost');
    if (costEl) {
      costEl.textContent = this.formatCost(usage.costUsd || 0);
    }
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
      const response = await fetch(`/api/link-preview?url=${encodeURIComponent(url)}`);
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

  // Convert URLs in text to clickable links (escapes non-URL text for safety)
  linkifyText(text) {
    if (!text) return '';
    
    const urlRegex = /(https?:\/\/[^\s<>\[\](){}|\\^`\x00-\x1f\x7f]+)/gi;
    const parts = text.split(urlRegex);
    
    return parts.map((part, index) => {
      // Even indices are non-URL text, odd indices are URLs (due to capture group)
      if (index % 2 === 0) {
        // Non-URL text - escape it
        return this.escapeHtml(part);
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

  // Load link previews for a message element
  async loadLinkPreviews(messageEl, urls) {
    const container = messageEl.querySelector('.link-previews-container');
    if (!container || urls.length === 0) return;

    for (const url of urls) {
      const preview = await this.fetchLinkPreview(url);
      if (preview && !preview.error) {
        const cardHtml = this.renderLinkPreviewCard(preview, url);
        if (cardHtml) {
          container.insertAdjacentHTML('beforeend', cardHtml);
        }
      }
    }
  }
}

// Initialize app
const app = new WhatsAppClient();
