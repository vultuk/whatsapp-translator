import test from 'node:test';
import assert from 'node:assert/strict';
import {
  upsertDraft,
  getDraftPreview,
  toggleStarredMessage,
  isMessageStarred,
  filterMessagesByQuery,
  getVisibleContacts,
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
  }).map(contact => contact.id);

  assert.deepEqual(visibleWithSearch, ['alice']);

  const unreadOnly = getVisibleContacts({
    contacts,
    drafts,
    searchQuery: '',
    filters: { unreadOnly: true, groupsOnly: false, draftsOnly: false },
    messagePreviewByContact: {},
  }).map(contact => contact.id);

  assert.deepEqual(unreadOnly, ['project']);

  const draftsOnly = getVisibleContacts({
    contacts,
    drafts,
    searchQuery: '',
    filters: { unreadOnly: false, groupsOnly: false, draftsOnly: true },
    messagePreviewByContact: {},
  }).map(contact => contact.id);

  assert.deepEqual(draftsOnly, ['alice']);
});
