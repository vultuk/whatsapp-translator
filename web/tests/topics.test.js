import test from 'node:test';
import assert from 'node:assert/strict';
import { topicsForChat, topicPagePath, validateTopicPage } from '../public/topics.js';

test('identical topic titles remain separate by source chat', () => {
  const catalog = {topics: [
    {id: 'family-plans', contactId: 'family@g.us', title: 'Weekend plans'},
    {id: 'friends-plans', contactId: 'friends@g.us', title: 'Weekend plans'},
  ]};
  assert.deepEqual(topicsForChat(catalog, 'family@g.us').map(t => t.id), ['family-plans']);
  assert.deepEqual(topicsForChat(catalog, 'unknown@g.us'), []);
});

test('topic pages reject cross-chat results before they can change the reply destination', () => {
  const topic = {id:'one',contactId:'family@g.us'};
  assert.throws(() => validateTopicPage(topic,{messages:[{id:'other',contactId:'friends@g.us'}]}), /did not match/);
  const page = {messages:[{id:'good',contactId:'family@g.us'}],hasMore:false};
  assert.equal(validateTopicPage(topic,page),page);
});

test('topic pagination preserves timestamp ties and encodes opaque IDs', () => {
  const path = topicPagePath('a/b+%2F',{id:'message+1',timestamp:100});
  assert.equal(path,'/api/topics/a%2Fb%2B%252F/messages?limit=50&before=100&before_id=message%2B1');
});
