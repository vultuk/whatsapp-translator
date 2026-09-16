import { mergeMessageUpdate } from './send-recovery.js';

export function topicsForChat(catalog, contactId) {
  return (catalog.topics || []).filter(topic => topic.contactId === contactId);
}

export function topicPagePath(id, cursor = null) {
  const query = new URLSearchParams({limit: '50'});
  if (cursor) { query.set('before', String(cursor.timestamp)); query.set('before_id', cursor.id); }
  return `/api/topics/${encodeURIComponent(id)}/messages?${query}`;
}

export function validateTopicPage(topic, page) {
  if (!topic || !Array.isArray(page.messages) || !page.messages.every(message => message.contactId === topic.contactId)) {
    throw new Error('Topic messages did not match this conversation.');
  }
  return page;
}

export function setupTopics(app) {
  const state = {catalog: {topics: [], settings: [], available: false}, selectedId: null, messages: [], hasMore: false};
  let catalogGeneration = 0, pageGeneration = 0, lifecycleGeneration = 0, loading = false, error = null;
  const bar = document.createElement('div');
  bar.className = 'topic-filter-bar';
  const select = document.createElement('select'); select.setAttribute('aria-label', 'Filter messages by topic');
  const manage = document.createElement('button'); manage.type = 'button'; manage.textContent = 'Topics';
  const earlier = document.createElement('button'); earlier.type = 'button'; earlier.textContent = 'Earlier topic messages'; earlier.hidden = true;
  const status = document.createElement('span'); status.setAttribute('role', 'status');
  const retry = document.createElement('button'); retry.type = 'button'; retry.textContent = 'Retry'; retry.hidden = true;
  bar.append(select, manage, earlier, status, retry);
  document.querySelector('.chat-main-column')?.prepend(bar);
  const dialog = document.createElement('dialog'); dialog.className = 'message-tone-dialog topic-management-dialog';
  document.body.append(dialog);

  function render() {
    const topics = topicsForChat(state.catalog, app.currentContactId);
    select.replaceChildren(new Option('All messages', ''));
    topics.forEach(topic => select.add(new Option(`${topic.title} (${topic.messageCount})`, topic.id)));
    select.value = state.selectedId || '';
    earlier.hidden = !state.selectedId || !state.hasMore;
    earlier.disabled = loading;
    const setting = state.catalog.settings.find(item => item.contactId === app.currentContactId);
    status.textContent = error || (loading ? 'Loading topic…' : setting?.pendingCount ? `Organising ${setting.pendingCount} messages…` : setting?.failedCount ? 'Some messages need retrying. Open Topics.' : '');
    retry.hidden = !error;
  }

  async function load() {
    if (app.demoMode) { render(); return; }
    const generation = ++catalogGeneration;
    try {
      const response = await app.apiFetch('/api/topics');
      if (!response.ok) throw new Error('Couldn’t load topics.');
      const catalog = await response.json();
      if (generation !== catalogGeneration) return;
      state.catalog = catalog;
      if (state.selectedId && !topicsForChat(catalog, app.currentContactId).some(topic => topic.id === state.selectedId)) {
        state.selectedId = null; state.messages = []; pageGeneration += 1;
        app.clearReply(); app.refreshCurrentConversationView();
      }
      error = null; render();
      if (state.selectedId) await loadPage();
    } catch { if (generation === catalogGeneration) { error = 'Couldn’t load topics.'; render(); } }
  }

  async function loadPage(older = false) {
    const topic = topicsForChat(state.catalog, app.currentContactId).find(item => item.id === state.selectedId);
    if (!topic || (older && (!state.hasMore || loading))) return;
    const generation = ++pageGeneration;
    loading = true; error = null; render();
    try {
      const response = await app.apiFetch(topicPagePath(topic.id, older ? state.messages[0] : null));
      if (!response.ok) throw new Error('Couldn’t load this topic.');
      const page = validateTopicPage(topic, await response.json());
      if (generation !== pageGeneration || state.selectedId !== topic.id || app.currentContactId !== topic.contactId) return;
      const cached = app.messages.get(topic.contactId) || [];
      const messages = page.messages.filter(message => {
        const latest = cached.find(item => item.id === message.id);
        return !latest || (latest.content?.edited_at_ms || 0) <= (message.content?.edited_at_ms || 0);
      });
      let next = older ? state.messages : [];
      for (const message of messages) next = mergeMessageUpdate(next, message);
      state.messages = next; state.hasMore = page.hasMore;
      let all = cached;
      for (const message of messages) all = mergeMessageUpdate(all, message);
      app.messages.set(topic.contactId, all);
      app.refreshCurrentConversationView();
    } catch { if (generation === pageGeneration) { error = 'Couldn’t load this topic.'; } }
    finally { if (generation === pageGeneration) { loading = false; render(); } }
  }

  function selectChat() {
    pageGeneration += 1; state.selectedId = null; state.messages = []; state.hasMore = false; loading = false; error = null;
    render(); void load();
  }

  function update(message) {
    const existing = state.messages.find(item => item.id === message.id && item.contactId === message.contactId);
    if (!existing) return;
    if ((message.content?.edited_at_ms || 0) > (existing.content?.edited_at_ms || 0)) {
      state.messages = state.messages.filter(item => item.id !== message.id);
      pageGeneration += 1; loading = false; render();
    } else {
      state.messages = mergeMessageUpdate(state.messages, message);
    }
  }

  async function openManagement() {
    const contactId = app.currentContactId;
    if (!contactId) return;
    await load();
    if (app.currentContactId !== contactId) return;
    dialog.replaceChildren();
    const title = document.createElement('h2'); title.textContent = 'Organise by topic';
    const explanation = document.createElement('p'); explanation.textContent = 'Follow one discussion at a time in Chats and Messages. Topics stay separate by chat.';
    const label = document.createElement('label'); label.className = 'checkbox-row';
    const checkbox = document.createElement('input'); checkbox.type = 'checkbox';
    const setting = state.catalog.settings.find(item => item.contactId === contactId);
    checkbox.checked = setting?.enabled === true;
    checkbox.disabled = !state.catalog.available && !checkbox.checked;
    label.append(checkbox, document.createTextNode(' Organise this chat with AI'));
    const note = document.createElement('p'); note.className = 'form-hint'; note.textContent = 'Off by default. Sends the latest 200 text messages and captions to your configured AI, then processes new messages and edits. Adds AI usage. Other people’s WhatsApp stays unchanged.';
    const feedback = document.createElement('p'); feedback.setAttribute('role', 'status');
    if (setting?.failedCount) feedback.textContent = `${setting.failedCount} messages couldn’t be organised. Save with topics enabled to retry.`;
    if (!state.catalog.available) feedback.textContent = 'OpenAI is unavailable on this server.';
    const cancel = document.createElement('button'); cancel.type = 'button'; cancel.textContent = 'Cancel'; cancel.onclick = () => dialog.close();
    const save = document.createElement('button'); save.type = 'button'; save.textContent = 'Save'; save.disabled = !state.catalog.available && !setting?.enabled;
    const importButton = document.createElement('button'); importButton.type = 'button'; importButton.textContent = 'Organise last 7 days from all chats'; importButton.disabled = !state.catalog.available;
    const importNote = document.createElement('p'); importNote.className = 'form-hint'; importNote.textContent = 'Initial import uses text and captions already stored on the server, skips messages already organised, and enables topics for the included chats so new messages stay organised.';
    const startImport = document.createElement('button'); startImport.type = 'button'; startImport.textContent = 'Start import'; startImport.hidden = true;
    const lifecycle = lifecycleGeneration;
    let saving = false;
    function busy(value) { saving = value; save.disabled = value; checkbox.disabled = value; cancel.disabled = value; importButton.disabled = value; startImport.disabled = value; }
    importButton.onclick = async () => {
      busy(true);
      try {
        const response = await app.apiFetch('/api/topics/import');
        if (!response.ok) throw new Error('Couldn’t preview the import. Please try again.');
        const preview = await response.json();
        if (lifecycle !== lifecycleGeneration) return;
        feedback.textContent = preview.messageCount ? `${preview.messageCount} messages across ${preview.chatCount} chats will be sent to your configured AI. This adds AI usage and enables topics for those chats.` : 'No unorganised text messages or captions from the last 7 days are stored on this server.';
        startImport.hidden = !preview.messageCount;
      } catch (failure) { feedback.textContent = failure.message; }
      finally { busy(false); }
    };
    startImport.onclick = async () => {
      busy(true);
      try {
        const response = await app.apiFetch('/api/topics/import', {method: 'POST'});
        if (!response.ok) throw new Error('Couldn’t start the import. Please try again.');
        const result = await response.json();
        if (lifecycle !== lifecycleGeneration) return;
        await load();
        feedback.textContent = `Queued ${result.messageCount} messages across ${result.chatCount} chats. Topics will appear as processing finishes.`;
        checkbox.checked = state.catalog.settings.find(item => item.contactId === contactId)?.enabled === true;
        startImport.hidden = true;
      } catch (failure) { feedback.textContent = failure.message; }
      finally { busy(false); }
    };
    dialog.oncancel = event => { if (saving) event.preventDefault(); };
    save.onclick = async () => {
      busy(true); catalogGeneration += 1;
      try {
        const response = await app.apiFetch(`/api/contacts/${encodeURIComponent(contactId)}/topics`, {
          method: 'PUT', headers: {'Content-Type': 'application/json'}, body: JSON.stringify({enabled: checkbox.checked}),
        });
        if (!response.ok) throw new Error('Couldn’t save topic settings. Please try again.');
        await response.json(); if (lifecycle !== lifecycleGeneration) return; await load(); dialog.close();
      } catch (failure) { feedback.textContent = failure.message; }
      finally { busy(false); }
    };
    dialog.append(title, explanation, label, note, importButton, importNote, feedback, startImport, cancel, save); dialog.showModal();
  }

  select.onchange = () => {
    state.selectedId = topicsForChat(state.catalog, app.currentContactId).some(topic => topic.id === select.value) ? select.value : null;
    pageGeneration += 1; loading = false; state.messages = []; state.hasMore = false;
    app.clearReply(); app.refreshCurrentConversationView(); render();
    if (state.selectedId) void loadPage();
  };
  manage.onclick = () => void openManagement();
  earlier.onclick = () => void loadPage(true);
  retry.onclick = () => { if (state.selectedId) void loadPage(); else void load(); };
  render();
  return {
    get selectedId() { return state.selectedId; },
    get messages() { return state.messages; },
    load, selectChat, update, loadOlder: () => loadPage(true),
    reset: () => {lifecycleGeneration += 1;catalogGeneration += 1; pageGeneration += 1; state.catalog={topics:[],settings:[],available:false};state.selectedId=null;state.messages=[];loading=false;dialog.close();render();},
  };
}
