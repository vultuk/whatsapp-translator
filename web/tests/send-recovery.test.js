import test from 'node:test';
import assert from 'node:assert/strict';
import {webcrypto} from 'node:crypto';
import {createReliableFetch, mergeMessageUpdate, reconnectDelay} from '../public/send-recovery.js';

function memoryStorage() {
  const values = new Map();
  return {getItem: key => values.get(key) || null, setItem: (key,value) => values.set(key,value)};
}

test('a retry after network loss and reload uses the same durable send identity', async () => {
  const storage = memoryStorage(); const keys = [];
  const failed = createReliableFetch({storage,cryptoImpl:webcrypto,fetchImpl:async (_,options)=>{keys.push(options.headers.get('Idempotency-Key'));throw Error('lost response');}});
  await assert.rejects(failed('/api/send',{method:'POST',body:'{"contactId":"one","text":"Hello"}'}),/uncertain/);
  const resumed = createReliableFetch({storage,cryptoImpl:webcrypto,fetchImpl:async (_,options)=>{keys.push(options.headers.get('Idempotency-Key'));return new Response('{}',{headers:{'X-Delivery-State':'confirmed'}});}});
  await resumed('/api/send',{method:'POST',body:'{"text":"Hello","contactId":"one"}'});
  assert.equal(keys[0],keys[1]);
  await resumed('/api/send',{method:'POST',body:'{"text":"Hello","contactId":"one"}'});
  assert.notEqual(keys[1],keys[2], 'a new deliberate send after confirmation gets a new key');
});

test('an uncertain server outcome retains identity and failed persistence prevents network sending', async () => {
  const storage = memoryStorage(); const keys = [];
  const request = createReliableFetch({storage,cryptoImpl:webcrypto,fetchImpl:async (_,options)=>{keys.push(options.headers.get('Idempotency-Key'));return new Response('{}',{status:409,headers:{'X-Delivery-State':'uncertain'}});}});
  const options={method:'POST',body:'{"preparationId":"same-note"}'};
  await request('/api/voice/send',options); await request('/api/voice/send',options);
  assert.equal(keys[0],keys[1]);
  let calls = 0;
  const broken = createReliableFetch({storage:{getItem:()=>null,setItem:()=>{throw Error('disk full');}},cryptoImpl:webcrypto,fetchImpl:async()=>{calls++;}});
  await assert.rejects(broken('/api/send',options),/disk full/);
  assert.equal(calls,0);
});

test('translation updates replace one message without appending duplicates and backoff is bounded', () => {
  const messages=[{id:'one',timestamp:1,originalText:'Hola'},{id:'two',timestamp:2}];
  const updated=mergeMessageUpdate(messages,{...messages[0],translatedText:'Hello'});
  assert.equal(updated.length,2); assert.equal(updated[0].translatedText,'Hello');
  assert.deepEqual([1,2,3,10].map(reconnectDelay),[2000,4000,8000,30000]);
});
