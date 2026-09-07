import test from 'node:test';
import assert from 'node:assert/strict';
import { voicePayload } from '../public/voice-notes.js';

test('voice preparation preserves the selected recipient and reply identity', () => {
  assert.deepEqual(voicePayload('recipient', 'encoded-recording', {messageId:'reply', senderJid:'sender', text:'Quoted text'}), {
    contactId:'recipient', mediaData:'encoded-recording', replyTo:'reply', replyToSender:'sender', replyToText:'Quoted text'
  });
  assert.equal(voicePayload('recipient', 'data', null).replyTo, null);
});
