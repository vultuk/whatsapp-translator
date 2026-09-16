import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

const __dirname = dirname(fileURLToPath(import.meta.url));
const appJs = readFileSync(join(__dirname, '..', 'public', 'app.js'), 'utf8');
const indexHtml = readFileSync(join(__dirname, '..', 'public', 'index.html'), 'utf8');

function extractMethodBody(source, methodName) {
  const methodPattern = new RegExp(`(?:async\\s+)?${methodName}\\s*\\([^)]*\\)\\s*\\{`);
  const match = methodPattern.exec(source);
  const start = match?.index ?? -1;
  assert.notEqual(start, -1, `${methodName} should exist`);

  const braceStart = source.indexOf('{', start);
  assert.notEqual(braceStart, -1, `${methodName} should have a body`);

  let depth = 0;
  for (let index = braceStart; index < source.length; index += 1) {
    const char = source[index];
    if (char === '{') depth += 1;
    if (char === '}') {
      depth -= 1;
      if (depth === 0) {
        return source.slice(braceStart + 1, index);
      }
    }
  }

  assert.fail(`${methodName} body should terminate`);
}

test('foreground topic recovery refreshes a still-open socket session without marking messages read', async () => {
  const AsyncFunction = Object.getPrototypeOf(async function () {}).constructor;
  const recover = new AsyncFunction('document', 'WebSocket', extractMethodBody(appJs, 'refreshAfterBecomingVisible'));
  for (const readyState of [1, 3]) {
    const calls = [];
    const app = {ws: {readyState}, demoMode: false, authExpired: false,
      connectWebSocket() { calls.push('connect'); },
      topics: {async load() { calls.push('topics-and-selected-page'); }},
      loadMessages() { assert.fail('A foreground topic refresh must not mark a conversation read'); },
    };
    await recover.call(app, {visibilityState: 'visible'}, {CLOSED: 3});
    assert.deepEqual(calls, readyState === 3 ? ['connect', 'topics-and-selected-page'] : ['topics-and-selected-page']);
    calls.length = 0;
    await recover.call(app, {visibilityState: 'hidden'}, {CLOSED: 3});
    app.authExpired = true;
    await recover.call(app, {visibilityState: 'visible'}, {CLOSED: 3});
    app.authExpired = false; app.demoMode = true;
    await recover.call(app, {visibilityState: 'visible'}, {CLOSED: 3});
    assert.deepEqual(calls, []);
  }
  assert.match(extractMethodBody(appJs, 'bindEvents'), /this\.refreshAfterBecomingVisible\(\)/);
});

test('workspace export carries portable local state but not auth', () => {
  const exportBody = extractMethodBody(appJs, 'getWorkspaceStateExport');

  assert.match(exportBody, /app:\s*'whatsapp-translator'/);
  assert.match(exportBody, /schemaVersion:\s*1/);
  for (const stateKey of [
    'drafts',
    'starredMessages',
    'contactMetadata',
    'inboxPreferences',
    'quickReplies',
    'appearancePreferences',
    'recentEmojis',
  ]) {
    assert.match(exportBody, new RegExp(`${stateKey}:`));
  }
  assert.doesNotMatch(exportBody, /authToken|wa_auth_token/);
});

test('workspace import validates schema and refreshes local UI state', () => {
  const importBody = extractMethodBody(appJs, 'applyWorkspaceStateImport');

  assert.match(importBody, /payload\.app !== 'whatsapp-translator'/);
  assert.match(importBody, /payload\.schemaVersion !== 1/);
  assert.match(importBody, /this\.persistDrafts\(\)/);
  assert.match(importBody, /this\.persistStarredMessages\(\)/);
  assert.match(importBody, /this\.persistContactMetadata\(\)/);
  assert.match(importBody, /this\.persistQuickReplies\(\)/);
  assert.match(importBody, /this\.persistAppearancePreferences\(\)/);
  assert.match(importBody, /this\.persistRecentEmojis\(\)/);
  assert.match(importBody, /this\.persistInboxPreferences\(\)/);
  assert.match(importBody, /this\.syncInboxControls\(\)/);
  assert.match(importBody, /this\.restoreDraftForCurrentContact\(\)/);
  assert.match(importBody, /this\.renderQuickReplies\(\)/);
  assert.match(importBody, /this\.updateWorkspaceUI\(\)/);
  assert.doesNotMatch(importBody, /authToken|wa_auth_token/);
});

test('workspace import export controls are present in global settings', () => {
  assert.match(indexHtml, /id="workspace-export-button"/);
  assert.match(indexHtml, /id="workspace-import-button"/);
  assert.match(indexHtml, /id="workspace-import-input"/);
});

test('AI compose drafts generated text instead of auto-sending it', () => {
  const sendWithAiBody = extractMethodBody(appJs, 'sendWithAI');

  assert.match(sendWithAiBody, /this\.apiFetch\('\/api\/ai-compose'/);
  assert.match(sendWithAiBody, /this\.handleDraftInput\(\)/);
  assert.doesNotMatch(sendWithAiBody, /this\.sendMessage\(\)/);
  assert.doesNotMatch(sendWithAiBody, /ClaudAI Says/);
});

test('drafts persist silently without a composer banner', () => {
  const draftInputBody = extractMethodBody(appJs, 'handleDraftInput');
  const restoreDraftBody = extractMethodBody(appJs, 'restoreDraftForCurrentContact');

  assert.match(draftInputBody, /this\.persistDrafts\(\)/);
  assert.match(restoreDraftBody, /getDraftText\(this\.drafts, this\.currentContactId\)/);
  assert.doesNotMatch(indexHtml, /id="draft-banner"/);
  assert.doesNotMatch(indexHtml, /Discard draft/);
  assert.doesNotMatch(appJs, /updateDraftBanner|draft-clear-button/);
});

test('sidebar identity header omits profile avatar and connection status', () => {
  assert.doesNotMatch(indexHtml, /id="user-initial"/);
  assert.doesNotMatch(indexHtml, /id="status-indicator"/);
  assert.doesNotMatch(appJs, /getElementById\('user-initial'\)/);
  assert.doesNotMatch(appJs, /updateConnectionIndicator/);
});

test('disconnected bridge shows cached contacts instead of permanent connecting state', () => {
  const disconnectedBody = extractMethodBody(appJs, 'handleDisconnected');
  const cachedWorkspaceBody = extractMethodBody(appJs, 'showCachedWorkspaceDisconnected');

  assert.match(disconnectedBody, /await this\.fetchCurrentQRCode\(\)/);
  assert.match(disconnectedBody, /await this\.loadContacts\(\)/);
  assert.match(disconnectedBody, /this\.contacts\.length > 0/);
  assert.match(disconnectedBody, /this\.showCachedWorkspaceDisconnected\(\)/);
  assert.match(disconnectedBody, /this\.showConnecting\(\)/);
  assert.match(cachedWorkspaceBody, /main-container'\)\?\.classList\.remove\('hidden'\)/);
  assert.match(cachedWorkspaceBody, /WhatsApp disconnected/);
  assert.match(cachedWorkspaceBody, /Cached inbox available/);
});

test('qr pairing view is not hidden by disconnected status races', () => {
  const constructorBody = extractMethodBody(appJs, 'constructor');
  const showConnectingBody = extractMethodBody(appJs, 'showConnecting');
  const showQrBody = extractMethodBody(appJs, 'showQRCode');
  const connectedBody = extractMethodBody(appJs, 'handleConnected');
  const logoutBody = extractMethodBody(appJs, 'handleLogout');

  assert.match(constructorBody, /this\.qrData = null/);
  assert.match(showConnectingBody, /if \(this\.qrData\)/);
  assert.match(showConnectingBody, /this\.showQRCode\(this\.qrData\)/);
  assert.match(showQrBody, /this\.qrData = qrData/);
  assert.match(connectedBody, /this\.qrData = null/);
  assert.match(logoutBody, /this\.qrData = null/);
});

test('composer send controls require a live bridge connection', () => {
  const updateSendButtonBody = extractMethodBody(appJs, 'updateSendButton');

  assert.match(updateSendButtonBody, /this\.currentContactId && this\.connected/);
});

test('contact previews prefer translated message text when available', () => {
  const getMessagePreview = new Function('message', extractMethodBody(appJs, 'getMessagePreview'));

  assert.equal(
    getMessagePreview({
      isFromMe: false,
      isTranslated: true,
      translatedText: 'Perfect, can you arrive after 18:00?',
      content: { type: 'text', body: 'Perfecto, puede llegar despues de las 18:00?' },
    }),
    'Perfect, can you arrive after 18:00?'
  );
  assert.equal(
    getMessagePreview({
      isFromMe: true,
      isTranslated: true,
      originalText: '18:30 works for us.',
      translatedText: '18:30 nos va bien.',
      content: { type: 'text', body: '18:30 works for us.' },
    }),
    'You: 18:30 works for us.'
  );
});

test('hidden inbox filters are cleared and cannot constrain the contact list', () => {
  const syncInboxControlsBody = extractMethodBody(appJs, 'syncInboxControls');
  const getFilteredContactsBody = extractMethodBody(appJs, 'getFilteredContacts');

  assert.match(syncInboxControlsBody, /inboxFilterControlsAvailable/);
  assert.match(syncInboxControlsBody, /normalizeInboxFilters/);
  assert.match(syncInboxControlsBody, /this\.persistInboxPreferences\(\)/);
  assert.match(getFilteredContactsBody, /effectiveFilters/);
  assert.match(getFilteredContactsBody, /controlsAvailable:\s*this\.inboxFilterControlsAvailable\(\)/);
});

test('conversation settings modal only exposes translator-facing fields', () => {
  assert.match(indexHtml, /id="contact-alias"/);
  assert.match(indexHtml, /id="conversation-timezone"/);
  assert.match(indexHtml, /id="language-override"/);
  assert.match(indexHtml, /id="translation-style"/);
  assert.match(indexHtml, /id="send-original-follow-up"/);

  for (const removedField of [
    'conversation-priority',
    'conversation-notes',
    'conversation-checklist',
    'conversation-labels',
    'conversation-reminder-text',
    'conversation-reminder-at',
    'conversation-snooze-until',
    'settings-pinned',
  ]) {
    assert.doesNotMatch(indexHtml, new RegExp(`id="${removedField}"`));
  }

  const saveSettingsBody = extractMethodBody(appJs, 'saveConversationSettings');
  assert.match(saveSettingsBody, /conversation-timezone/);
  assert.match(saveSettingsBody, /send-original-follow-up/);
  assert.match(saveSettingsBody, /sendOriginalFollowUp/);
  assert.doesNotMatch(saveSettingsBody, /conversation-priority/);
  assert.doesNotMatch(saveSettingsBody, /conversation-checklist/);
  assert.doesNotMatch(saveSettingsBody, /conversation-reminder/);
});

test('send response adds the confirmed original follow-up to the local conversation', () => {
  const sendMessageBody = extractMethodBody(appJs, 'sendMessage');

  assert.match(sendMessageBody, /result\.originalFollowUpSent/);
  assert.match(sendMessageBody, /result\.originalMessageId/);
  assert.match(sendMessageBody, /result\.originalTimestamp/);
});

test('loaded reaction records are merged instead of rendered as standalone messages', () => {
  const handleMessageBody = extractMethodBody(appJs, 'handleMessage');
  const normalizeBody = extractMethodBody(appJs, 'normalizeLoadedMessages');
  const loadMessagesBody = extractMethodBody(appJs, 'loadMessages');
  const loadOlderMessagesBody = extractMethodBody(appJs, 'loadOlderMessages');
  const renderContentBody = extractMethodBody(appJs, 'renderContent');
  const getMessagePreviewBody = extractMethodBody(appJs, 'getMessagePreview');

  assert.match(handleMessageBody, /case 'reaction':\s*this\.handleReactionMessage\(data\.message\)/);
  assert.match(normalizeBody, /this\.applyReactionToMessages\(/);
  assert.match(normalizeBody, /this\.isDisplayableMessage\(message\)/);
  assert.match(loadMessagesBody, /this\.normalizeLoadedMessages\(contactId,\s*messages\)/);
  assert.match(loadOlderMessagesBody, /this\.normalizeLoadedMessages\(contactId,\s*olderMessages,\s*existingMessages\)/);
  assert.doesNotMatch(renderContentBody, /case 'reaction'/);
  assert.doesNotMatch(getMessagePreviewBody, /case 'reaction'/);
});

test('static and generated markup do not use inline event attributes', () => {
  const renderedSources = `${indexHtml}\n${appJs}`;

  assert.doesNotMatch(
    renderedSources,
    /\son(?:click|error|change|input|submit|keydown|load)=/i
  );
});


test('confirmed reactions use one actor and preserve snapshot removals against older pages', () => {
  const app = {};
  app.getReactionActor = new Function('reactionMsg', extractMethodBody(appJs, 'getReactionActor'));
  app.applyReactionToMessage = new Function('targetMessage', 'reactionMsg', extractMethodBody(appJs, 'applyReactionToMessage'));
  const message = {reactions: {}, reactionStates: {me: {id: 'remove', timestamp: 102, emoji: ''}}};
  const reaction = (id, timestamp, emoji) => ({id, timestamp, isFromMe: true, content: {emoji}});
  app.applyReactionToMessage(message, reaction('z-old', 101, '❤️'));
  assert.deepEqual(message.reactions, {});
  app.applyReactionToMessage(message, reaction('a-new', 103, '👍'));
  app.applyReactionToMessage(message, reaction('a-new', 103, '👍'));
  assert.deepEqual(message.reactions, {'👍': ['me']});
  app.applyReactionToMessage(message, reaction('b-new', 104, '❤️'));
  assert.deepEqual(message.reactions, {'❤️': ['me']});
  app.applyReactionToMessage(message, reaction('c-new', 105, ''));
  assert.deepEqual(message.reactions, {});
});

test('a history response captured before a live edit cannot restore its old text', () => {
  const original = {id:'one',contactId:'chat',timestamp:1,content:{type:'text',body:'Get'}};
  const edited = {...original,content:{...original.content,body:'Grr',edited_at_ms:20}};
  const app = {messages:new Map([['chat',[edited]]]),prepareMessageForCache:()=>{},isReactionMessage:()=>false,isDisplayableMessage:()=>true};
  app.normalizeLoadedMessages = new Function('contactId','rawMessages','additionalReactionTargets',extractMethodBody(appJs,'normalizeLoadedMessages'));
  assert.deepEqual(app.normalizeLoadedMessages('chat',[original],[]),[edited]);
});

test('incoming embedded quotes reach web rendering without an original history message', () => {
  const app = {
    getMessageSenderJid: () => '447700900123@s.whatsapp.net',
    formatMessageTime: () => '09:41', renderContent: () => '<p>Thanks!</p>', renderReactions: () => '',
    escapeHtml: text => String(text || '').replaceAll('&', '&amp;').replaceAll('<', '&lt;').replaceAll('>', '&gt;').replaceAll('"', '&quot;'),
  };
  app.prepareMessageForCache = new Function('message', extractMethodBody(appJs, 'prepareMessageForCache'));
  app.renderMessage = new Function('message', 'isMessageStarred', extractMethodBody(appJs, 'renderMessage'));
  const quote = {messageId: 'outside-history', senderName: 'Alex', text: 'Yep I can <bring lunch>'};
  const message = app.prepareMessageForCache({id:'reply',contactId:'family@g.us',isFromMe:false,chatType:'group',senderName:'Sam',content:{type:'text',body:'Thanks!',reply_context:quote}});
  const restored = JSON.parse(JSON.stringify(message));
  const html = app.renderMessage(restored, () => false);
  assert.match(html, /class="quoted-sender">Alex</);
  assert.match(html, /class="quoted-text">Yep I can &lt;bring lunch&gt;</);
  assert.match(html, /data-message-id="reply"/);
  assert.equal(restored.replyContext.messageId, 'outside-history');
});
