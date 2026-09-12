import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync, existsSync } from 'node:fs';
import { messageTonePath, notificationSoundOptions } from '../public/message-tones.js';

const catalog = JSON.parse(readFileSync(new URL('../public/sounds/message-tones.json', import.meta.url)));

test('ringtone requests preserve exact contact identities including plus and separators', () => {
  const id = 'family+one&two%three@g.us';
  const path = new URL(messageTonePath(id), 'https://example.test');
  assert.equal(path.searchParams.get('contactId'), id);
  assert.equal([...path.searchParams.keys()].length, 1);
  assert.equal(messageTonePath(), '/api/settings/message-tone');
});

test('custom tones suppress the browser default and silent never plays an asset', () => {
  assert.deepEqual(notificationSoundOptions('aurora', catalog), {silent: true, filename: 'bb-aurora.wav'});
  assert.deepEqual(notificationSoundOptions('silent', catalog), {silent: true, filename: null});
  assert.deepEqual(notificationSoundOptions('default', catalog), {silent: false, filename: null});
  assert.deepEqual(notificationSoundOptions('unknown', catalog), {silent: true, filename: null});
  assert.equal(catalog.length, 6);
  for (const tone of catalog) assert.ok(existsSync(new URL(`../public/sounds/${tone.filename}`, import.meta.url)));
});
