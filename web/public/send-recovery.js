export function isSendRequest(path, method = 'GET') {
  return method === 'POST' && (['/api/send', '/api/send-image', '/api/send-images', '/api/react', '/api/voice/send'].includes(path)
    || (path.startsWith('/api/photo-albums/') && path.endsWith('/send')));
}

function canonical(value) {
  if (Array.isArray(value)) return value.map(canonical);
  if (value && typeof value === 'object') return Object.fromEntries(Object.keys(value).sort().map(key => [key, canonical(value[key])]));
  return value;
}

export function createReliableFetch({fetchImpl = fetch, storage = localStorage, cryptoImpl = crypto} = {}) {
  const registryKey = 'babel-send-recovery-v1';
  return async (url, options = {}) => {
    if (!isSendRequest(url, options.method)) return fetchImpl(url, options);
    if (!cryptoImpl.subtle) throw new Error('Use HTTPS or localhost to send with duplicate protection.');
    const canonicalBody = options.body ? JSON.stringify(canonical(JSON.parse(options.body))) : '';
    const digest = await cryptoImpl.subtle.digest('SHA-256', new TextEncoder().encode(`${url}\n${canonicalBody}`));
    const fingerprint = Array.from(new Uint8Array(digest), byte => byte.toString(16).padStart(2, '0')).join('');
    const entries = JSON.parse(storage.getItem(registryKey) || '{}');
    const key = entries[fingerprint] || cryptoImpl.randomUUID();
    entries[fingerprint] = key;
    // Do not send if the retry identity cannot be durably saved.
    storage.setItem(registryKey, JSON.stringify(entries));
    const headers = new Headers(options.headers || {});
    headers.set('Idempotency-Key', key);
    let response;
    try { response = await fetchImpl(url, {...options, headers}); }
    catch { throw new Error('Delivery is uncertain. Retry the same action to check its result without sending twice.'); }
    const state = response.headers.get('X-Delivery-State');
    if (state === 'confirmed' || state === 'failed' || (!state && response.ok)) {
      const current = JSON.parse(storage.getItem(registryKey) || '{}');
      if (current[fingerprint] === key) { delete current[fingerprint]; storage.setItem(registryKey, JSON.stringify(current)); }
    }
    return response;
  };
}

export const reconnectDelay = attempt => Math.min(30000, 1000 * 2 ** Math.min(Math.max(attempt, 1), 5));

export function mergeMessageUpdate(messages, message) {
  return [...messages.filter(existing => existing.id !== message.id), message].sort((a, b) => a.timestamp - b.timestamp);
}
