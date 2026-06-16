import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

const __dirname = dirname(fileURLToPath(import.meta.url));
const appJs = readFileSync(join(__dirname, '..', 'public', 'app.js'), 'utf8');
const indexHtml = readFileSync(join(__dirname, '..', 'public', 'index.html'), 'utf8');

function extractMethodBody(source, methodName) {
  const methodPattern = new RegExp(`(?:async\\s+)?${methodName}\\s*\\([^)]*\\)\\s*\\{`);
  const match = methodPattern.exec(source);
  const start = match?.index ?? -1;
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

test('workspace export carries portable local state but not auth', () => {
  const exportBody = extractMethodBody(appJs, 'getWorkspaceStateExport');

  assert.match(exportBody, /app:\s*'whatsapp-translator'/);
  assert.match(exportBody, /schemaVersion:\s*1/);
  for (const stateKey of [
    'drafts',
    'starredMessages',
    'contactMetadata',
    'inboxPreferences',
    'quickReplies',
    'appearancePreferences',
    'recentEmojis',
  ]) {
    assert.match(exportBody, new RegExp(`${stateKey}:`));
  }
  assert.doesNotMatch(exportBody, /authToken|wa_auth_token/);
});

test('workspace import validates schema and refreshes local UI state', () => {
  const importBody = extractMethodBody(appJs, 'applyWorkspaceStateImport');

  assert.match(importBody, /payload\.app !== 'whatsapp-translator'/);
  assert.match(importBody, /payload\.schemaVersion !== 1/);
  assert.match(importBody, /this\.persistDrafts\(\)/);
  assert.match(importBody, /this\.persistStarredMessages\(\)/);
  assert.match(importBody, /this\.persistContactMetadata\(\)/);
  assert.match(importBody, /this\.persistQuickReplies\(\)/);
  assert.match(importBody, /this\.persistAppearancePreferences\(\)/);
  assert.match(importBody, /this\.persistRecentEmojis\(\)/);
  assert.match(importBody, /this\.persistInboxPreferences\(\)/);
  assert.match(importBody, /this\.syncInboxControls\(\)/);
  assert.match(importBody, /this\.restoreDraftForCurrentContact\(\)/);
  assert.match(importBody, /this\.renderQuickReplies\(\)/);
  assert.match(importBody, /this\.updateWorkspaceUI\(\)/);
  assert.doesNotMatch(importBody, /authToken|wa_auth_token/);
});

test('workspace import export controls are present in global settings', () => {
  assert.match(indexHtml, /id="workspace-export-button"/);
  assert.match(indexHtml, /id="workspace-import-button"/);
  assert.match(indexHtml, /id="workspace-import-input"/);
});

test('AI compose drafts generated text instead of auto-sending it', () => {
  const sendWithAiBody = extractMethodBody(appJs, 'sendWithAI');

  assert.match(sendWithAiBody, /this\.apiFetch\('\/api\/ai-compose'/);
  assert.match(sendWithAiBody, /this\.handleDraftInput\(\)/);
  assert.doesNotMatch(sendWithAiBody, /this\.sendMessage\(\)/);
  assert.doesNotMatch(sendWithAiBody, /ClaudAI Says/);
});

test('static and generated markup do not use inline event attributes', () => {
  const renderedSources = `${indexHtml}\n${appJs}`;

  assert.doesNotMatch(
    renderedSources,
    /\son(?:click|error|change|input|submit|keydown|load)=/i
  );
});
