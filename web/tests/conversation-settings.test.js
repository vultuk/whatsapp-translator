import test from 'node:test';
import assert from 'node:assert/strict';
import {ConversationSettingsClient} from '../public/conversation-settings.js';
import {getComposerAssistState} from '../public/app-state.js';

test('people and groups default off and keep their own saved language when disabled', async () => {
  const client = new ConversationSettingsClient({isDemo: () => true});
  for (const id of ['person@s.whatsapp.net', 'family@g.us']) assert.equal((await client.load(id)).translationEnabled, false);
  await client.save('family@g.us', {translationEnabled: true, languageOverride: 'Hungarian'});
  assert.equal(client.get('person@s.whatsapp.net').translationEnabled, false);
  await client.save('family@g.us', {...client.get('family@g.us'), translationEnabled: false});
  assert.equal(client.get('family@g.us').languageOverride, 'Hungarian');
  const off = getComposerAssistState({draftText: 'Good morning', metadata: client.get('family@g.us'), demoMode: true});
  assert.equal(off.translationEnabled, false);
  assert.equal(off.translatedPreview, '');
  assert.equal(off.previewLabel, 'Send as written');
  assert.equal(off.readiness.checks[0].label, 'Draft will send as written.');
});

test('failed saves do not change the displayed setting and requests preserve exact conversation identity', async () => {
  const requests = [];
  const client = new ConversationSettingsClient({request: async (path, options) => {
    requests.push({path, body: JSON.parse(options.body)});
    return new Response('{"error":"Could not save"}', {status: 503});
  }});
  const id = 'family+one%2Ftwo@g.us';
  client.apply(id, {translationEnabled: true, languageOverride: 'Hungarian'});
  await assert.rejects(client.save(id, {translationEnabled: false}), /Could not save/);
  assert.equal(client.get(id).translationEnabled, true);
  assert.equal(requests[0].path, `/api/contacts/${encodeURIComponent(id)}/settings`);
  assert.equal(requests[0].body.translationEnabled, false);
});

test('a late read cannot undo a settings event from another device', async () => {
  let respond;
  const client = new ConversationSettingsClient({request: () => new Promise(resolve => {respond = resolve;})});
  const loading = client.load('family@g.us');
  client.apply('family@g.us', {translationEnabled: false, languageOverride: 'Hungarian'});
  respond(new Response('{"translationEnabled":true,"languageOverride":"Hungarian"}'));
  assert.equal((await loading).translationEnabled, false);
  assert.equal(client.get('family@g.us').translationEnabled, false);
});

test('old server responses default off even when they contain a foreign language', async () => {
  const client = new ConversationSettingsClient({request: async () => new Response('{"languageOverride":"Hungarian"}')});
  const settings = await client.load('person@s.whatsapp.net');
  assert.equal(settings.translationEnabled, false);
  const live = getComposerAssistState({draftText: 'Hello', metadata: {...settings, translationEnabled: true}});
  assert.equal(live.translatedPreview, '', 'live UI must not invent a translated message');
});
