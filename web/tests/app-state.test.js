import test from 'node:test';
import assert from 'node:assert/strict';
import {
  upsertDraft,
  getDraftPreview,
  toggleStarredMessage,
  isMessageStarred,
  filterMessagesByQuery,
  getVisibleContacts,
  getReminderStatus,
  getReplyState,
  getContactDisplayName,
  isContactSnoozed,
  parseChecklistInput,
  parseLabelsInput,
  removeQuickReply,
  toggleChecklistItem,
  upsertChecklistItems,
  upsertQuickReply,
  buildConversationBrief,
  getChecklistSummary,
  getComposerSendReadiness,
  getContextualReminderPresets,
  getPriorityInfo,
  getTimezoneInfo,
  getAppearanceThemeCatalog,
  getComposerAssistState,
  getComposerProfilePresets,
  getComposerReminderPresets,
  getContactActionSnapshot,
  getContrastRatio,
  getMessageSnippet,
  buildVisitorDashboard,
  createDemoWorkspace,
  resolveAppearanceTheme,
  simulateTranslation,
  getSmartReplyOptions,
  suggestDemoReply,
} from '../public/app-state.js';

test('upsertDraft stores trimmed text and removes empty drafts', () => {
  const withDraft = upsertDraft({}, 'alice', '  Need to reply later  ', 1000);
  assert.deepEqual(withDraft, {
    alice: {
      text: 'Need to reply later',
      updatedAt: 1000,
    },
  });

  const cleared = upsertDraft(withDraft, 'alice', '   ', 2000);
  assert.deepEqual(cleared, {});
});

test('getDraftPreview returns a user-facing summary', () => {
  const drafts = {
    chat_1: { text: 'This is a saved draft message', updatedAt: 123 },
  };

  assert.equal(getDraftPreview(drafts, 'chat_1'), 'Draft: This is a saved draft message');
  assert.equal(getDraftPreview(drafts, 'missing'), '');
});

test('toggleStarredMessage adds and removes starred messages', () => {
  const message = {
    id: 'msg-1',
    contactId: 'chat_1',
    timestamp: 111,
    content: { type: 'text', body: 'Remember the passport' },
  };

  const starred = toggleStarredMessage({}, message, 5000);
  assert.equal(isMessageStarred(starred, 'msg-1'), true);
  assert.equal(starred['msg-1'].snippet, 'Remember the passport');

  const unstarred = toggleStarredMessage(starred, message, 6000);
  assert.equal(isMessageStarred(unstarred, 'msg-1'), false);
  assert.deepEqual(unstarred, {});
});

test('filterMessagesByQuery supports text search and starred-only filtering', () => {
  const messages = [
    { id: '1', content: { type: 'text', body: 'Book the airport taxi' } },
    { id: '2', content: { type: 'text', body: 'Pick up groceries' } },
  ];

  assert.deepEqual(
    filterMessagesByQuery(messages, 'airport', { starredLookup: {} }).map(message => message.id),
    ['1'],
  );

  assert.deepEqual(
    filterMessagesByQuery(messages, '', { starredOnly: true, starredLookup: { '2': { id: '2' } } }).map(message => message.id),
    ['2'],
  );
});

test('getVisibleContacts prioritizes drafts, unread filter, and search matching', () => {
  const contacts = [
    { id: 'alice', name: 'Alice', phone: '111', type: 'private', unreadCount: 0, lastMessageTime: 10 },
    { id: 'project', name: 'Project Team', phone: null, type: 'group', unreadCount: 3, lastMessageTime: 20 },
  ];

  const drafts = {
    alice: { text: 'Send travel plan', updatedAt: 999 },
  };

  const visibleWithSearch = getVisibleContacts({
    contacts,
    drafts,
    searchQuery: 'travel',
    filters: { unreadOnly: false, groupsOnly: false, draftsOnly: false },
    messagePreviewByContact: {},
    metadataByContact: {},
  }).map(contact => contact.id);

  assert.deepEqual(visibleWithSearch, ['alice']);

  const unreadOnly = getVisibleContacts({
    contacts,
    drafts,
    searchQuery: '',
    filters: { unreadOnly: true, groupsOnly: false, draftsOnly: false },
    messagePreviewByContact: {},
    metadataByContact: {},
  }).map(contact => contact.id);

  assert.deepEqual(unreadOnly, ['project']);

  const draftsOnly = getVisibleContacts({
    contacts,
    drafts,
    searchQuery: '',
    filters: { unreadOnly: false, groupsOnly: false, draftsOnly: true },
    messagePreviewByContact: {},
    metadataByContact: {},
  }).map(contact => contact.id);

  assert.deepEqual(draftsOnly, ['alice']);
});

test('getVisibleContacts supports pinned and notes-only inbox filters', () => {
  const contacts = [
    { id: 'vip', name: 'VIP Client', phone: '555', type: 'private', unreadCount: 0, lastMessageTime: 30 },
    { id: 'family', name: 'Family', phone: '777', type: 'group', unreadCount: 2, lastMessageTime: 40 },
  ];

  const metadataByContact = {
    vip: { pinnedAt: 100, notes: 'Needs the pricing recap before Friday' },
    family: { pinnedAt: null, notes: '' },
  };

  assert.deepEqual(
    getVisibleContacts({
      contacts,
      drafts: {},
      searchQuery: '',
      filters: { pinnedOnly: true },
      messagePreviewByContact: {},
      metadataByContact,
    }).map(contact => contact.id),
    ['vip'],
  );

  assert.deepEqual(
    getVisibleContacts({
      contacts,
      drafts: {},
      searchQuery: '',
      filters: { notesOnly: true },
      messagePreviewByContact: {},
      metadataByContact,
    }).map(contact => contact.id),
    ['vip'],
  );

  assert.deepEqual(
    getVisibleContacts({
      contacts,
      drafts: {},
      searchQuery: 'pricing recap',
      filters: {},
      messagePreviewByContact: {},
      metadataByContact,
    }).map(contact => contact.id),
    ['vip'],
  );
});

test('parseLabelsInput trims, deduplicates, and preserves visitor-defined labels', () => {
  assert.deepEqual(
    parseLabelsInput(' VIP, travel plans, vip,   Family  , , follow-up '),
    ['VIP', 'travel plans', 'Family', 'follow-up'],
  );
});

test('snoozed conversations stay out of the default inbox until the snooze expires', () => {
  const now = 1_710_000_000_000;
  const contacts = [
    { id: 'snoozed', name: 'Later', phone: '111', type: 'private', unreadCount: 0, lastMessageTime: 20 },
    { id: 'active', name: 'Now', phone: '222', type: 'private', unreadCount: 1, lastMessageTime: 30 },
  ];
  const metadataByContact = {
    snoozed: { snoozedUntil: now + 60_000 },
  };

  assert.equal(isContactSnoozed(metadataByContact.snoozed, now), true);

  assert.deepEqual(
    getVisibleContacts({
      contacts,
      drafts: {},
      searchQuery: '',
      filters: {},
      messagePreviewByContact: {},
      metadataByContact,
      now,
    }).map(contact => contact.id),
    ['active'],
  );

  assert.deepEqual(
    getVisibleContacts({
      contacts,
      drafts: {},
      searchQuery: '',
      filters: { snoozedOnly: true },
      messagePreviewByContact: {},
      metadataByContact,
      now,
    }).map(contact => contact.id),
    ['snoozed'],
  );
});

test('getVisibleContacts supports reminder and label-focused triage views', () => {
  const now = 1_710_000_000_000;
  const contacts = [
    { id: 'vip', name: 'VIP Client', phone: '555', type: 'private', unreadCount: 0, lastMessageTime: 30 },
    { id: 'trip', name: 'Trip Planner', phone: '777', type: 'private', unreadCount: 2, lastMessageTime: 40 },
  ];
  const metadataByContact = {
    vip: {
      reminderText: 'Send the pricing breakdown',
      reminderAt: now - 5_000,
      labels: ['VIP', 'follow-up'],
    },
    trip: {
      reminderText: 'Share the itinerary tomorrow',
      reminderAt: now + 86_400_000,
      labels: ['travel'],
    },
  };

  assert.equal(getReminderStatus(metadataByContact.vip, now), 'due');
  assert.equal(getReminderStatus(metadataByContact.trip, now), 'upcoming');

  assert.deepEqual(
    getVisibleContacts({
      contacts,
      drafts: {},
      searchQuery: '',
      filters: { dueRemindersOnly: true },
      messagePreviewByContact: {},
      metadataByContact,
      now,
    }).map(contact => contact.id),
    ['vip'],
  );

  assert.deepEqual(
    getVisibleContacts({
      contacts,
      drafts: {},
      searchQuery: '',
      filters: { labelsOnly: true },
      messagePreviewByContact: {},
      metadataByContact,
      now,
    }).map(contact => contact.id),
    ['vip', 'trip'],
  );

  assert.deepEqual(
    getVisibleContacts({
      contacts,
      drafts: {},
      searchQuery: 'pricing breakdown vip',
      filters: {},
      messagePreviewByContact: {},
      metadataByContact,
      now,
    }).map(contact => contact.id),
    ['vip'],
  );
});

test('getReplyState identifies reply queues, drafts, and waiting states', () => {
  const now = 1_710_000_000_000;
  const incomingMessage = { id: 'm1', timestamp: now - 10_000, isFromMe: false, content: { type: 'text', body: 'Can you confirm?' } };
  const outgoingMessage = { id: 'm2', timestamp: now - 5_000, isFromMe: true, content: { type: 'text', body: 'On it' } };

  assert.equal(getReplyState({ contact: { id: 'vip', unreadCount: 0 }, messages: [incomingMessage], drafts: {} }), 'needs-reply');
  assert.equal(getReplyState({ contact: { id: 'vip', unreadCount: 0 }, messages: [outgoingMessage], drafts: {} }), 'waiting');
  assert.equal(getReplyState({ contact: { id: 'vip', unreadCount: 0 }, messages: [incomingMessage], drafts: { vip: { text: 'Draft reply', updatedAt: now } } }), 'drafting');
  assert.equal(getReplyState({ contact: { id: 'vip', unreadCount: 4 }, messages: [], drafts: {} }), 'needs-reply');
  assert.equal(getReplyState({ contact: { id: 'vip', unreadCount: 4 }, messages: [], drafts: {}, metadata: { snoozedUntil: now + 60_000 }, now }), 'snoozed');
});

test('getVisibleContacts supports alias search and reply queue filters', () => {
  const now = 1_710_000_000_000;
  const contacts = [
    { id: 'host', name: 'Maria Lopez', phone: '111', type: 'private', unreadCount: 1, lastMessageTime: 30 },
    { id: 'vendor', name: 'Office Vendor', phone: '222', type: 'private', unreadCount: 0, lastMessageTime: 40 },
  ];
  const metadataByContact = {
    host: { alias: 'Madrid Airbnb host' },
    vendor: { alias: 'Printer supplier' },
  };

  assert.equal(getContactDisplayName(contacts[0], metadataByContact.host), 'Madrid Airbnb host');

  assert.deepEqual(
    getVisibleContacts({
      contacts,
      drafts: {},
      searchQuery: 'airbnb madrid',
      filters: {},
      messagePreviewByContact: {},
      metadataByContact,
      messagesByContact: {
        host: [{ id: 'host-1', isFromMe: false, timestamp: now - 1_000, content: { type: 'text', body: 'Can you send the check-in code?' } }],
        vendor: [{ id: 'vendor-1', isFromMe: true, timestamp: now - 2_000, content: { type: 'text', body: 'Invoice paid, thanks' } }],
      },
      now,
    }).map(contact => contact.id),
    ['host'],
  );

  assert.deepEqual(
    getVisibleContacts({
      contacts,
      drafts: {},
      searchQuery: '',
      filters: { needsReplyOnly: true },
      messagePreviewByContact: {},
      metadataByContact,
      messagesByContact: {
        host: [{ id: 'host-1', isFromMe: false, timestamp: now - 1_000, content: { type: 'text', body: 'Can you send the check-in code?' } }],
        vendor: [{ id: 'vendor-1', isFromMe: true, timestamp: now - 2_000, content: { type: 'text', body: 'Invoice paid, thanks' } }],
      },
      now,
    }).map(contact => contact.id),
    ['host'],
  );
});

test('upsertQuickReply deduplicates snippets and removeQuickReply deletes by id', () => {
  const initial = upsertQuickReply([], '  On my way now  ', 100);
  assert.equal(initial.length, 1);
  assert.equal(initial[0].text, 'On my way now');

  const deduped = upsertQuickReply(initial, 'on my way now', 200);
  assert.equal(deduped.length, 1);
  assert.equal(deduped[0].updatedAt, 200);

  const expanded = upsertQuickReply(deduped, 'Thanks — I will send it shortly.', 300, 2);
  const trimmed = upsertQuickReply(expanded, 'Got it, thank you!', 400, 2);
  assert.deepEqual(trimmed.map(reply => reply.text), ['Got it, thank you!', 'Thanks — I will send it shortly.']);

  assert.deepEqual(removeQuickReply(trimmed, trimmed[0].id), [trimmed[1]]);
});

test('priority info normalizes visitor triage preferences and surfaces important conversations', () => {
  assert.deepEqual(getPriorityInfo({ priority: 'URGENT' }), {
    value: 'urgent',
    label: 'Urgent',
    rank: 0,
    isImportant: true,
  });

  assert.deepEqual(getPriorityInfo({ priority: 'low' }), {
    value: 'low',
    label: 'Low',
    rank: 3,
    isImportant: false,
  });

  assert.deepEqual(getPriorityInfo({}), {
    value: 'normal',
    label: 'Normal',
    rank: 2,
    isImportant: false,
  });
});

test('getVisibleContacts supports important-only inbox triage and priority-aware search', () => {
  const contacts = [
    { id: 'host', name: 'Airport host', phone: '111', type: 'private', unreadCount: 0, lastMessageTime: 20 },
    { id: 'friend', name: 'Dinner plans', phone: '222', type: 'private', unreadCount: 0, lastMessageTime: 10 },
  ];

  const metadataByContact = {
    host: { priority: 'urgent' },
    friend: { priority: 'low' },
  };

  assert.deepEqual(
    getVisibleContacts({
      contacts,
      drafts: {},
      searchQuery: '',
      filters: { importantOnly: true },
      messagePreviewByContact: {},
      metadataByContact,
    }).map(contact => contact.id),
    ['host'],
  );

  assert.deepEqual(
    getVisibleContacts({
      contacts,
      drafts: {},
      searchQuery: 'urgent airport',
      filters: {},
      messagePreviewByContact: {},
      metadataByContact,
    }).map(contact => contact.id),
    ['host'],
  );
});

test('checklists can be parsed from visitor notes, updated, and summarized', () => {
  const parsed = parseChecklistInput('- [ ] Confirm check-in code\n- [x] Book taxi\nBring passport');
  assert.deepEqual(parsed.map(item => ({ text: item.text, done: item.done })), [
    { text: 'Confirm check-in code', done: false },
    { text: 'Book taxi', done: true },
    { text: 'Bring passport', done: false },
  ]);

  const initial = upsertChecklistItems([], '- [ ] Confirm check-in code\n- [x] Book taxi', 100);
  assert.equal(initial.length, 2);
  assert.equal(initial[0].done, false);
  assert.equal(initial[1].done, true);

  const toggled = toggleChecklistItem(initial, initial[0].id, 200);
  assert.equal(toggled[0].done, true);
  assert.equal(toggled[0].updatedAt, 200);

  const updated = upsertChecklistItems(toggled, '- [ ] Confirm check-in code\nBring passport', 300);
  assert.deepEqual(updated.map(item => ({ text: item.text, done: item.done })), [
    { text: 'Confirm check-in code', done: false },
    { text: 'Bring passport', done: false },
  ]);

  assert.deepEqual(getChecklistSummary({ checklist: updated }), {
    total: 2,
    completed: 0,
    open: 2,
    label: '2 open tasks',
  });
});

test('getVisibleContacts supports open-task inbox triage and checklist search', () => {
  const contacts = [
    { id: 'trip', name: 'Trip planner', phone: '111', type: 'private', unreadCount: 0, lastMessageTime: 20 },
    { id: 'done', name: 'Handled', phone: '222', type: 'private', unreadCount: 0, lastMessageTime: 10 },
  ];

  const metadataByContact = {
    trip: {
      checklist: [
        { id: 'a', text: 'Send passport details', done: false },
        { id: 'b', text: 'Confirm airport pickup', done: true },
      ],
    },
    done: {
      checklist: [
        { id: 'c', text: 'Archive receipt', done: true },
      ],
    },
  };

  assert.deepEqual(
    getVisibleContacts({
      contacts,
      drafts: {},
      searchQuery: '',
      filters: { tasksOnly: true },
      messagePreviewByContact: {},
      metadataByContact,
    }).map(contact => contact.id),
    ['trip'],
  );

  assert.deepEqual(
    getVisibleContacts({
      contacts,
      drafts: {},
      searchQuery: 'passport details',
      filters: {},
      messagePreviewByContact: {},
      metadataByContact,
    }).map(contact => contact.id),
    ['trip'],
  );
});

test('getContactActionSnapshot highlights the most urgent visitor context first', () => {
  const now = 1_710_000_000_000;
  const contact = {
    id: 'vip',
    name: 'VIP Client',
    unreadCount: 2,
    lastMessageTime: now - 5_000,
  };

  const snapshot = getContactActionSnapshot({
    contact,
    drafts: {
      vip: { text: 'Need to send the contract recap', updatedAt: now - 2_000 },
    },
    metadata: {
      priority: 'urgent',
      reminderText: 'Send the contract recap',
      reminderAt: now - 60_000,
      checklist: [{ id: 'task-1', text: 'Share contract recap', done: false }],
      labels: ['VIP'],
    },
    messages: [{ id: 'm1', timestamp: now - 5_000, isFromMe: false, content: { type: 'text', body: 'Can you send the recap?' } }],
    now,
  });

  assert.equal(snapshot.headline, 'Reminder due');
  assert.equal(snapshot.replyState, 'drafting');
  assert.equal(snapshot.checklist.open, 1);
  assert.equal(snapshot.priority.value, 'urgent');
  assert.match(snapshot.summary, /Reminder due/);
  assert.ok(snapshot.attentionScore > 150);
});

test('buildVisitorDashboard summarizes focus work, reminders, drafts, and open tasks', () => {
  const now = 1_710_000_000_000;
  const contacts = [
    { id: 'vip', name: 'VIP Client', unreadCount: 1, lastMessageTime: now - 1_000 },
    { id: 'trip', name: 'Trip Planner', unreadCount: 0, lastMessageTime: now - 10_000 },
    { id: 'later', name: 'Later', unreadCount: 0, lastMessageTime: now - 20_000 },
  ];

  const dashboard = buildVisitorDashboard({
    contacts,
    drafts: {
      vip: { text: 'Need to send the contract recap', updatedAt: now - 2_000 },
    },
    metadataByContact: {
      vip: {
        priority: 'urgent',
        reminderText: 'Send the contract recap',
        reminderAt: now - 60_000,
        checklist: [{ id: 'task-1', text: 'Share contract recap', done: false }],
      },
      trip: {
        reminderText: 'Share the itinerary',
        reminderAt: now + 3_600_000,
        checklist: [
          { id: 'task-2', text: 'Book the taxi', done: false },
          { id: 'task-3', text: 'Save passport copy', done: true },
        ],
      },
      later: {
        snoozedUntil: now + 86_400_000,
        checklist: [{ id: 'task-4', text: 'Ignore while snoozed', done: false }],
      },
    },
    messagesByContact: {
      vip: [{ id: 'vip-1', timestamp: now - 1_000, isFromMe: false, content: { type: 'text', body: 'Can you send the recap?' } }],
      trip: [{ id: 'trip-1', timestamp: now - 10_000, isFromMe: true, content: { type: 'text', body: 'Working on the itinerary now.' } }],
    },
    now,
  });

  assert.deepEqual(dashboard.stats, {
    needsReply: 1,
    dueReminders: 1,
    drafts: 1,
    openTasks: 2,
  });
  assert.equal(dashboard.focusContacts[0].contact.id, 'vip');
  assert.deepEqual(
    dashboard.upcomingReminders.map(entry => entry.contact.id),
    ['trip'],
  );
  assert.equal(dashboard.snoozedContacts, 1);
});

test('createDemoWorkspace provides a usable visitor demo inbox', () => {
  const now = 1_710_000_000_000;
  const demo = createDemoWorkspace(now);
  const dashboard = buildVisitorDashboard({
    contacts: demo.contacts,
    drafts: demo.drafts,
    metadataByContact: demo.metadataByContact,
    messagesByContact: demo.messagesByContact,
    now,
  });

  assert.equal(demo.contacts.length, 4);
  assert.equal(demo.contacts[0].id, 'demo-casa-azul');
  assert.ok(demo.messagesByContact['demo-casa-azul'].length >= 3);
  assert.equal(dashboard.stats.needsReply, 3);
  assert.equal(dashboard.stats.dueReminders, 1);
  assert.equal(dashboard.stats.drafts, 1);
  assert.ok(dashboard.stats.openTasks >= 4);
  assert.equal(dashboard.snoozedContacts, 1);
});

test('demo reply and translation helpers make the product explorable without a backend', () => {
  const demo = createDemoWorkspace(1_710_000_000_000);
  const contact = demo.contacts.find(entry => entry.id === 'demo-casa-azul');
  const metadata = demo.metadataByContact[contact.id];
  const message = demo.messagesByContact[contact.id].at(-1);
  const reply = suggestDemoReply({ message, metadata, contact });

  assert.match(reply, /18:30/);
  assert.match(reply, /taxi plate/);
  assert.equal(
    simulateTranslation('18:30 works and I will send the taxi plate', 'Spanish'),
    '18:30 nos va bien. Te enviare la matricula del taxi antes de llegar.',
  );
  assert.equal(
    simulateTranslation('Please share the venue pin and station exit', 'Japanese'),
    '会場のピンと駅の出口をすぐに共有します。',
  );
});

test('composer assist builds a translation preview and reply starter', () => {
  const incoming = {
    id: 'm1',
    isFromMe: false,
    content: { type: 'text', body: 'Puede llegar despues de las 18:00?' },
  };

  const state = getComposerAssistState({
    draftText: '18:30 works and I will send the taxi plate',
    metadata: { languageOverride: 'Spanish', translationStyle: 'friendly' },
    contact: { id: 'host', name: 'Sofia' },
    latestIncomingMessage: incoming,
    demoMode: true,
  });

  assert.equal(getMessageSnippet(incoming), 'Puede llegar despues de las 18:00?');
  assert.equal(state.targetLanguage, 'Spanish');
  assert.equal(state.styleSummary, 'friendly tone');
  assert.equal(state.previewLabel, 'Demo translation preview');
  assert.match(state.translatedPreview, /18:30 nos va bien/);
  assert.match(state.suggestedReply, /18:30 still works/);
  assert.equal(state.canUseSuggestedReply, true);
  assert.deepEqual(state.smartReplies.map(reply => reply.id), ['confirm', 'ask-detail', 'buy-time']);
  assert.equal(state.profilePresets.languages.find(preset => preset.value === 'Spanish').active, true);
  assert.equal(state.profilePresets.tones.find(preset => preset.value === 'friendly').active, true);
  assert.equal(state.readiness.scoreLabel, 'Ready');
  assert.equal(state.conversationBrief.nextAction, 'Review and send the saved draft.');
});

test('composer assist still explains the route without a draft', () => {
  const state = getComposerAssistState({
    metadata: { targetLanguage: 'Japanese' },
    demoMode: false,
  });

  assert.equal(state.targetLanguage, 'Japanese');
  assert.equal(state.previewLabel, 'Translation route');
  assert.equal(state.hasDraft, false);
  assert.equal(state.canUseSuggestedReply, false);
  assert.equal(state.showPreview, true);
});

test('smart reply options give distinct one-tap composer choices', () => {
  const replies = getSmartReplyOptions({
    message: { content: { body: 'Please share the venue pin and station exit' } },
    metadata: { languageOverride: 'Japanese' },
    contact: { id: 'planner', name: 'Akari' },
  });

  assert.equal(replies.length, 3);
  assert.equal(replies[0].label, 'Confirm');
  assert.match(replies[1].text, /location details/);
  assert.match(replies[2].text, /Akari/);
});

test('composer profile and reminder presets expose fast setup choices', () => {
  const profile = getComposerProfilePresets({
    targetLanguage: 'French',
    translationStyle: 'formal',
  });
  const reminders = getComposerReminderPresets(new Date('2026-04-27T10:20:00Z').getTime());

  assert.equal(profile.languages.find(preset => preset.value === 'French').active, true);
  assert.equal(profile.tones.find(preset => preset.value === 'formal').active, true);
  assert.deepEqual(reminders.map(preset => preset.id), ['later-today', 'tomorrow', 'next-week']);
  assert.equal(new Date(reminders[0].reminderAt).getHours(), 14);
  assert.equal(new Date(reminders[1].reminderAt).getHours(), 9);
});

test('contextual reminders promote follow-up timing detected in the conversation', () => {
  const now = new Date('2026-04-27T10:20:00Z').getTime();
  const latestIncomingMessage = {
    id: 'incoming',
    isFromMe: false,
    content: { type: 'text', body: 'Can you confirm the quote tomorrow morning?' },
  };

  const presets = getContextualReminderPresets({ latestIncomingMessage, now });

  assert.equal(presets[0].id, 'context-tomorrow');
  assert.equal(presets[0].label, 'Tomorrow from chat');
  assert.equal(new Date(presets[0].reminderAt).getHours(), 9);
});

test('composer readiness warns about missed tasks and quiet-hour contacts', () => {
  const readiness = getComposerSendReadiness({
    draftText: 'Thanks, I will check.',
    metadata: {
      timezone: 'Asia/Tokyo',
      checklist: [{ id: 'task-1', text: 'Share venue pin', done: false }],
    },
    latestIncomingMessage: {
      id: 'incoming',
      isFromMe: false,
      content: { type: 'text', body: 'Which station exit should we use?' },
    },
    now: new Date('2026-04-27T18:30:00Z').getTime(),
  });

  assert.equal(readiness.warnings, 2);
  assert.equal(readiness.checks.some(check => check.id === 'tasks' && check.status === 'warning'), true);
  assert.equal(readiness.checks.some(check => check.id === 'timezone' && check.status === 'warning'), true);
});

test('conversation brief turns latest messages and tasks into a next action', () => {
  const brief = buildConversationBrief({
    contact: { id: 'trip', name: 'Trip group', unreadCount: 1 },
    metadata: {
      checklist: [{ id: 'task-1', text: 'Confirm venue pin', done: false }],
    },
    messages: [
      {
        id: 'outgoing',
        isFromMe: true,
        content: { type: 'text', body: 'I will check the location.' },
      },
      {
        id: 'incoming',
        isFromMe: false,
        content: { type: 'text', body: 'Which station exit should we use?' },
      },
    ],
  });

  assert.equal(brief.hasQuestion, true);
  assert.equal(brief.nextAction, 'Answer the latest question directly.');
  assert.equal(brief.openTasks[0].text, 'Confirm venue pin');
  assert.match(brief.summary, /Which station exit/);
});

test('resolveAppearanceTheme falls back safely and honors system dark mode', () => {
  assert.deepEqual(
    resolveAppearanceTheme({ theme: 'unknown', mode: 'maybe' }, { systemMode: 'dark' }),
    {
      theme: 'whatsapp',
      mode: 'system',
      resolvedMode: 'dark',
      definition: {
        label: 'WhatsApp',
        themeColor: '#111b21',
        accentColor: '#00a884',
        surfaceColor: '#202c33',
        textColor: '#e9edef',
      },
      dataTheme: 'whatsapp-dark',
    },
  );
});

test('resolveAppearanceTheme returns the selected light theme variant explicitly', () => {
  assert.deepEqual(
    resolveAppearanceTheme({ theme: 'ocean', mode: 'light' }, { systemMode: 'dark' }),
    {
      theme: 'ocean',
      mode: 'light',
      resolvedMode: 'light',
      definition: {
        label: 'Ocean',
        themeColor: '#f4f7fb',
        accentColor: '#0f6cbd',
        surfaceColor: '#ffffff',
        textColor: '#102a43',
      },
      dataTheme: 'ocean-light',
    },
  );
});

test('appearance theme catalog includes additional recognizable developer themes', () => {
  const catalog = getAppearanceThemeCatalog();
  assert.deepEqual(
    catalog.map(theme => theme.id),
    ['whatsapp', 'ocean', 'sunset', 'github', 'dracula', 'nord', 'linear', 'vercel'],
  );
});

test('all appearance theme variants meet a readable text contrast threshold', () => {
  const catalog = getAppearanceThemeCatalog();

  for (const theme of catalog) {
    for (const mode of ['light', 'dark']) {
      const variant = theme[mode];
      assert.ok(
        getContrastRatio(variant.textColor, variant.surfaceColor) >= 4.5,
        `${theme.id}-${mode} surface contrast should be at least 4.5`,
      );
      assert.ok(
        getContrastRatio(variant.textColor, variant.themeColor) >= 4.5,
        `${theme.id}-${mode} page contrast should be at least 4.5`,
      );
    }
  }
});

test('timezone info surfaces visitor-friendly local time and quiet hours context', () => {
  const morningUtc = Date.parse('2026-04-16T07:30:00Z');
  const quietUtc = Date.parse('2026-04-16T23:30:00Z');

  assert.deepEqual(getTimezoneInfo({ timezone: 'Europe/Madrid' }, morningUtc), {
    timezone: 'Europe/Madrid',
    localTime: '09:30',
    label: '09:30 local time',
    status: 'working-hours',
    statusLabel: 'Working hours',
  });

  assert.deepEqual(getTimezoneInfo({ timezone: 'America/Los_Angeles' }, quietUtc), {
    timezone: 'America/Los_Angeles',
    localTime: '16:30',
    label: '16:30 local time',
    status: 'working-hours',
    statusLabel: 'Working hours',
  });

  assert.deepEqual(getTimezoneInfo({ timezone: 'Asia/Tokyo' }, quietUtc), {
    timezone: 'Asia/Tokyo',
    localTime: '08:30',
    label: '08:30 local time',
    status: 'morning',
    statusLabel: 'Morning',
  });
});
