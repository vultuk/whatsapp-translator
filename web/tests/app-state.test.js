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
  parseLabelsInput,
  removeQuickReply,
  upsertQuickReply,
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
