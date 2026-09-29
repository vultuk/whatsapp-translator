import test from 'node:test';
import assert from 'node:assert/strict';
import {ViewedMessageTracker, canSendReadReceipt, isMessageVisible} from './public/read-receipts.js';
const incoming = (id, chat = 'family@g.us') => ({id, contactId: chat, isFromMe: false, content: {type: 'text'}});

test('read receipts require continuous visible dwell and an active app', async () => {
  const sent = [];
  const tracker = new ViewedMessageTracker({send: async (chat, ids) => sent.push([chat, ids])});
  tracker.sample([incoming('one')], {active: true, now: 0});
  tracker.sample([incoming('one')], {active: false, now: 800});
  await tracker.flush(); assert.deepEqual(sent, []);
  tracker.sample([incoming('one')], {active: true, now: 1000});
  tracker.sample([], {active: true, now: 1300});
  tracker.sample([incoming('one')], {active: true, now: 2000});
  tracker.sample([incoming('one')], {active: true, now: 2699});
  await tracker.flush(); assert.deepEqual(sent, []);
  tracker.sample([incoming('one')], {active: true, now: 2700});
  await tracker.flush(); assert.deepEqual(sent, [['family@g.us', ['one']]]);
  tracker.sample([incoming('one')], {active: true, now: 5000});
  await tracker.flush(); assert.equal(sent.length, 1);
});

test('visible messages are batched by chat; failed requests survive restart without adding unseen messages', async () => {
  let saved;
  const tracker = new ViewedMessageTracker({send: async () => {throw new Error('offline');}, save: value => {saved = value;}});
  const messages = [incoming('one'), incoming('two'), incoming('three', 'other@s.whatsapp.net')];
  tracker.sample(messages, {active: true, now: 0});
  tracker.sample(messages, {active: true, now: 1000});
  await assert.rejects(tracker.flush());
  const sent = [];
  const restarted = new ViewedMessageTracker({pending: saved, send: async (chat, ids) => sent.push([chat, ids])});
  restarted.sample([incoming('unseen')], {active: false, now: 10000});
  await restarted.flush();
  assert.deepEqual(sent, [['family@g.us', ['one', 'two']], ['other@s.whatsapp.net', ['three']]]);
});

test('outgoing, deleted and reaction events cannot create receipts', () => {
  assert.equal(canSendReadReceipt({...incoming('own'), isFromMe: true}), false);
  for (const type of ['reaction', 'revoked', 'protocol', 'unknown']) assert.equal(canSendReadReceipt({...incoming(type), content: {type}}), false);
  assert.equal(canSendReadReceipt(incoming('status', 'status@broadcast')), false);
});

test('visibility clips to the message viewport and allows long messages without counting offscreen rows', () => {
  const view = {top: 100, bottom: 500, left: 0, right: 400};
  const rect = (top, bottom) => ({top, bottom, left: 20, right: 300, width: 280, height: bottom - top});
  assert.equal(isMessageVisible(rect(20, 80), view), false);
  assert.equal(isMessageVisible(rect(480, 600), view), false);
  assert.equal(isMessageVisible(rect(150, 230), view), true);
  assert.equal(isMessageVisible(rect(150, 1000), view), true);
});
