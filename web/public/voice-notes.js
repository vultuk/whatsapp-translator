// Translation previews stay separate from the send action. Prepared IDs survive HTTP retries.
export const VOICE_OPTIONS = [['auto', 'Automatic · match pitch'], ['masculine', 'Masculine'], ['feminine', 'Feminine'], ['neutral', 'Neutral']];
export function voicePayload(contactId, mediaData, reply) {
  return { contactId, mediaData, replyTo: reply?.messageId || null, replyToSender: reply?.senderJid || null, replyToText: reply?.text || null };
}
export function setupVoiceNotes(app) {
  const demoPreferences = new Map();
  const api = async (path, body, method = 'POST') => {
    if (app.demoMode) {
      if (path.startsWith('/api/voice/settings/')) {
        if (body) demoPreferences.set(path, body);
        return demoPreferences.get(path) || {voice:'auto'};
      }
      throw new Error('Connect a backend to translate and send real voice notes.');
    }
    const response = await app.apiFetch(path, { method, headers: {'Content-Type':'application/json'}, ...(body === undefined ? {} : {body:JSON.stringify(body)}) });
    const value = await response.json();
    if (!response.ok) throw new Error(value.error || 'Voice operation failed. Please try again.');
    return value;
  };
  const el = (tag, text, cls) => { const node = document.createElement(tag); if (text) node.textContent = text; if (cls) node.className = cls; return node; };
  const button = (text, action) => { const node = el('button', text, 'voice-button'); node.type = 'button'; node.onclick = action; return node; };
  const audio = (encoded, label) => {
    const box = el('div'); box.append(el('p', label));
    const player = el('audio'); player.controls = true; player.preload = 'metadata'; player.src = `data:audio/mpeg;base64,${encoded}`;
    player.setAttribute('aria-label', label);
    player.onplay = () => document.querySelectorAll('audio').forEach(other => { if (other !== player) other.pause(); });
    box.append(player); return box;
  };
  async function preference(container, scope) {
    const box = el('div', null, 'voice-preference');
    const label = el('label', scope === 'outgoing' ? 'My translated voice ' : 'Contact’s translated voice ');
    const select = el('select'); for (const [value, title] of VOICE_OPTIONS) { const option = el('option', title); option.value = value; select.append(option); }
    label.append(select); box.append(label, el('p', 'Automatic approximately matches vocal pitch; uncertain recordings use a neutral voice.', 'voice-hint'));
    const status = el('p', '', 'voice-hint'); status.setAttribute('role','status'); box.append(status); container.prepend(box);
    select.disabled = true;
    try { select.value = (await api(`/api/voice/settings/${encodeURIComponent(scope)}`, undefined, 'GET')).voice; select.disabled = false; }
    catch (error) { status.textContent = error.message; }
    const sampleBox = el('div');
    const previewVoice = button('Preview voice', async () => {
      previewVoice.disabled = select.disabled = true;
      try {
        const result = await api(`/api/voice/sample/${encodeURIComponent(select.value)}`);
        sampleBox.replaceChildren(audio(result.audioData, select.value === 'auto' ? 'Neutral fallback sample · AI-generated' : 'Voice sample · AI-generated'));
      } catch (error) { status.textContent = error.message; }
      finally { previewVoice.disabled = select.disabled = false; }
    });
    box.append(previewVoice, sampleBox);
    select.onchange = async () => {
      sampleBox.querySelectorAll('audio').forEach(player => player.pause()); sampleBox.replaceChildren();
      select.disabled = true;
      try { await api(`/api/voice/settings/${encodeURIComponent(scope)}`, {voice:select.value}, 'PUT'); status.textContent = 'Voice saved'; window.dispatchEvent(new CustomEvent('voice-preference-saved', {detail:scope})); }
      catch (error) { status.textContent = error.message; }
      finally { select.disabled = false; }
    };
  }
  function preview(container, note) {
    container.replaceChildren(el('h4', `Translated into ${note.targetLanguage}`), el('p', 'AI-generated voice', 'voice-hint'), audio(note.audioData, 'Translated voice'), audio(note.originalData, 'Original recording'));
    const details = el('details'); details.append(el('summary', 'Review transcript'), el('p', note.transcript), el('p', note.translation)); container.append(details);
    if (note.originalFollowUp) container.append(el('p', 'Your original recording will follow the translation.', 'voice-hint'));
  }
  const dialog = el('dialog', null, 'voice-dialog'); dialog.setAttribute('aria-label', 'Translated voice note'); document.body.append(dialog);
  let recorder, stream, timer, chunks = [], busy = false;
  const cleanRecording = () => { clearInterval(timer); if (recorder?.state === 'recording') { recorder.onstop = null; recorder.stop(); } stream?.getTracks().forEach(track => track.stop()); stream = null; recorder = null; };
  dialog.addEventListener('cancel', event => { if (busy) event.preventDefault(); });
  dialog.addEventListener('close', () => { cleanRecording(); dialog.querySelectorAll('audio').forEach(player => player.pause()); });
  const recordButton = button('Record voice note', async () => {
    if (!app.currentContactId) return;
    const contactId = app.currentContactId;
    const reply = app.replyingTo ? {...app.replyingTo} : null;
    dialog.replaceChildren(el('h3', 'Translated voice note'));
    const prefs = el('div'); dialog.append(prefs); await preference(prefs, 'outgoing');
    const status = el('p', 'Record up to three minutes. Listen to the translation before sending.'); status.setAttribute('role', 'status');
    const content = el('div', null, 'voice-preview');
    const controls = el('div', null, 'voice-actions');
    const close = button('Close', () => dialog.close());
    const record = button('Start recording', async () => {
      if (recorder?.state === 'recording') { recorder.stop(); return; }
      try {
        if (!navigator.mediaDevices?.getUserMedia || typeof MediaRecorder === 'undefined') throw new Error('Recording requires HTTPS or localhost and a browser with microphone support.');
        stream = await navigator.mediaDevices.getUserMedia({audio:true});
        if (!dialog.open) { cleanRecording(); return; }
        const mimeType = ['audio/webm;codecs=opus', 'audio/mp4', 'audio/ogg;codecs=opus'].find(type => MediaRecorder.isTypeSupported(type));
        recorder = new MediaRecorder(stream, mimeType ? {mimeType} : undefined); chunks = [];
        recorder.ondataavailable = event => { if (event.data.size) chunks.push(event.data); };
        recorder.onerror = () => { cleanRecording(); record.textContent = 'Start recording'; status.textContent = 'Recording failed. Please try again.'; };
        recorder.onstop = async () => {
          clearInterval(timer); stream?.getTracks().forEach(track => track.stop());
          busy = true; record.disabled = close.disabled = true; status.textContent = 'Preparing translated voice…';
          try {
            const blob = new Blob(chunks, {type:recorder.mimeType});
            if (blob.size > 16*1024*1024) throw new Error('Recording is too large. Please use a shorter note.');
            const encoded = await new Promise((resolve, reject) => { const reader = new FileReader(); reader.onload = () => resolve(String(reader.result).split(',')[1]); reader.onerror = reject; reader.readAsDataURL(blob); });
            const note = await api('/api/voice/prepare', voicePayload(contactId, encoded, reply));
            preview(content, note); record.hidden = true;
            status.textContent = 'Listen to the translated recording, then send when ready.';
            const send = button('Send translated voice note', async () => {
              busy = true; send.disabled = close.disabled = true; again.disabled = true; status.textContent = 'Sending…';
              try {
                const result = await api('/api/voice/send', {preparationId:note.id});
                status.textContent = result.warning || 'Voice note sent'; if (app.currentContactId === contactId && app.replyingTo?.messageId === reply?.messageId) app.clearReply(); send.remove(); again.remove();
                await app.loadContacts();
                if (app.currentContactId === contactId) await app.loadMessages(contactId);
              } catch (error) { status.textContent = error.message; send.textContent = 'Check send result'; send.disabled = false; }
              finally { busy = false; close.disabled = false; }
            });
            const again = button('Record again', () => { content.replaceChildren(); send.remove(); again.remove(); record.hidden = false; record.textContent = 'Start recording'; status.textContent = 'Ready to record'; });
            controls.prepend(send, again);
          } catch (error) { status.textContent = error.message; record.textContent = 'Record again'; }
          finally { busy = false; record.disabled = close.disabled = false; }
        };
        recorder.start(); record.textContent = 'Stop and translate'; let seconds = 0; status.textContent = 'Recording · 0s / 180s';
        timer = setInterval(() => { seconds++; status.textContent = `Recording · ${seconds}s / 180s`; if (seconds >= 180 && recorder?.state === 'recording') recorder.stop(); }, 1000);
      } catch (error) { status.textContent = error.message; cleanRecording(); }
    });
    controls.append(record, close); dialog.append(status, content, controls); dialog.showModal();
  });
  recordButton.id = 'voice-record-button'; recordButton.title = 'Record a translated voice note'; recordButton.textContent = '🎙'; recordButton.setAttribute('aria-label', 'Record translated voice note');
  document.getElementById('message-input')?.before(recordButton);

  window.addEventListener('voice-preference-saved', event => {
    if (event.detail !== app.currentContactId) return;
    document.querySelectorAll('.message-audio').forEach(node => {
      node.hidden = false; delete node.dataset.voiceMounted;
      if (node.nextElementSibling?.classList.contains('voice-inline')) node.nextElementSibling.remove();
    });
  });
  const readyIDs = new Set();
  window.addEventListener('voice-ready', event => {
    readyIDs.add(event.detail);
    for (const btn of document.querySelectorAll('button.voice-translate-action')) {
      if (btn.dataset.voiceMessageId === event.detail && !btn.disabled) btn.click();
    }
  });
  // Mount on actual message IDs, including lazy-loaded audio. Observer does not perform AI requests.
  const scan = () => {
    document.querySelectorAll('.message-audio').forEach(node => {
      if (node.dataset.voiceMounted === 'true') return;
      const owner = node.closest('[data-message-id]'); const id = owner?.dataset.messageId;
      if (!id || owner.querySelector('.voice-translate-action')) return;
      node.dataset.voiceMounted = 'true';
      const box = el('div', null, 'voice-inline');
      const translate = button('Translate voice', async () => {
        translate.disabled = true; translate.textContent = 'Translating voice…';
        try { const note = await api(`/api/voice/translate/${encodeURIComponent(id)}`); preview(box, note); node.hidden = true; }
        catch (error) { translate.disabled = false; translate.textContent = 'Retry voice translation'; status.textContent = error.message; }
      });
      translate.classList.add('voice-translate-action');
      translate.dataset.voiceMessageId = id;
      const status = el('p', '', 'voice-hint'); status.setAttribute('role','status'); box.append(translate, status); node.after(box);
      // Keep a marker after the preview replaces its button.
      box.classList.add('voice-translate-action');
      const message = [...app.messages.values()].flat().find(message => message.id === id);
      if (readyIDs.has(id) || message?.isTranslated) translate.click();
    });
  };
  new MutationObserver(scan).observe(document.getElementById('messages-list') || document.body, {childList:true, subtree:true}); scan();
  for (const [id, getScope] of [['settings-modal', () => app.settingsContactId], ['appearance-modal', () => 'outgoing']]) {
    const modal = document.getElementById(id); if (!modal) continue;
    new MutationObserver(() => {
      if (modal.classList.contains('hidden')) { modal.querySelectorAll('audio').forEach(player => player.pause()); return; }
      const scope = getScope(); if (!scope) return;
      modal.dataset.voiceScope = scope; modal.querySelector('.voice-preference')?.remove();
      preference(modal.querySelector('.modal-body'), scope);
    }).observe(modal, {attributes:true, attributeFilter:['class']});
  }
}
