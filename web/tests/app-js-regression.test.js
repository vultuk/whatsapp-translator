import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

const __dirname = dirname(fileURLToPath(import.meta.url));
const appJs = readFileSync(join(__dirname, '..', 'public', 'app.js'), 'utf8');

function extractMethodBody(source, methodName) {
  const marker = `async ${methodName}()`;
  const start = source.indexOf(marker);
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

test('AI compose drafts generated text instead of auto-sending it', () => {
  const sendWithAiBody = extractMethodBody(appJs, 'sendWithAI');

  assert.match(sendWithAiBody, /this\.apiFetch\('\/api\/ai-compose'/);
  assert.match(sendWithAiBody, /this\.handleDraftInput\(\)/);
  assert.doesNotMatch(sendWithAiBody, /this\.sendMessage\(\)/);
  assert.doesNotMatch(sendWithAiBody, /ClaudAI Says/);
});
