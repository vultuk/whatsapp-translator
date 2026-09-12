export function messageTonePath(contactId = null) {
  return '/api/settings/message-tone' + (contactId === null ? '' : `?${new URLSearchParams({contactId})}`);
}

export function notificationSoundOptions(tone, catalog) {
  const sound = catalog.find(item => item.id === tone);
  return { silent: tone !== 'default', filename: sound?.filename || null };
}

export function setupMessageTones(app) {
  let catalogPromise, previewAudio, notificationAudio, generation = 0;
  let demoGlobal = 'default';
  const demoOverrides = new Map();
  const special = [
    {id: 'default', title: 'System default', detail: 'This device’s standard notification sound'},
    {id: 'silent', title: 'Silent', detail: 'Show notifications without a sound'},
  ];
  const element = (tag, text, className) => {
    const node = document.createElement(tag);
    if (text) node.textContent = text;
    if (className) node.className = className;
    return node;
  };
  const button = (text, action, className) => {
    const node = element('button', text, className); node.type = 'button';
    node.addEventListener('click', action); return node;
  };
  async function catalog() {
    if (!catalogPromise) catalogPromise = fetch('/sounds/message-tones.json').then(async response => {
      if (!response.ok) throw new Error('The ringtone library couldn’t be loaded. Please try again.');
      return response.json();
    }).catch(error => { catalogPromise = null; throw error; });
    return catalogPromise;
  }
  async function settings(contactId = null, update) {
    if (app.demoMode) {
      if (update) {
        if (contactId === null) demoGlobal = update.tone;
        else if (update.tone === null) demoOverrides.delete(contactId);
        else demoOverrides.set(contactId, update.tone);
      }
      const tone = contactId === null ? demoGlobal : demoOverrides.get(contactId) ?? null;
      return {tone, globalTone: demoGlobal, effectiveTone: tone ?? demoGlobal};
    }
    const response = await app.apiFetch(messageTonePath(contactId), update ? {
      method: 'PUT', headers: {'Content-Type': 'application/json'}, body: JSON.stringify(update),
    } : undefined);
    if (!response.ok) throw new Error('The ringtone couldn’t be loaded or saved. Please try again.');
    return response.json();
  }
  const stopPreview = () => { previewAudio?.pause(); previewAudio = null; };
  async function preview(tone, sounds) {
    stopPreview();
    const sound = sounds.find(item => item.id === tone);
    if (!sound?.filename) return;
    previewAudio = new Audio(`/sounds/${sound.filename}`);
    await previewAudio.play();
  }
  const dialog = element('dialog', null, 'message-tone-dialog');
  dialog.setAttribute('aria-labelledby', 'message-tone-title');
  document.body.append(dialog);
  let saving = false;
  dialog.addEventListener('cancel', event => { if (saving) event.preventDefault(); });
  dialog.addEventListener('close', () => { generation += 1; stopPreview(); });

  async function open(contactId = null) {
    if (dialog.open) return;
    const current = ++generation;
    const title = element('h3', contactId === null ? 'Global ringtone' : 'Conversation ringtone');
    title.id = 'message-tone-title';
    const subtitle = element('p', contactId === null
      ? 'The default sound for your messages, across your connected devices.'
      : 'Give this conversation its own sound, or follow your global choice.', 'message-tone-hint');
    const status = element('p', 'Loading ringtones…', 'message-tone-status'); status.setAttribute('role', 'status');
    const list = element('div', null, 'message-tone-list');
    list.setAttribute('role', 'radiogroup'); list.setAttribute('aria-label', 'Message ringtone');
    const cancel = button('Cancel', () => dialog.close(), 'modal-button secondary');
    const save = button('Save ringtone', async () => {
      if (saving) return;
      saving = true; save.disabled = cancel.disabled = true; stopPreview();
      list.querySelectorAll('button,input').forEach(control => { control.disabled = true; });
      status.textContent = 'Saving ringtone…';
      try {
        await settings(contactId, {tone: selected});
        await refreshLabel(contactId);
        dialog.close();
      } catch (error) {
        status.textContent = error.message;
      } finally {
        saving = false; save.disabled = cancel.disabled = false;
        list.querySelectorAll('button,input').forEach(control => { control.disabled = false; });
      }
    }, 'modal-button primary');
    save.disabled = true;
    const actions = element('div', null, 'message-tone-actions'); actions.append(cancel, save);
    dialog.replaceChildren(title, subtitle, list, status, actions); dialog.showModal();
    let selected;
    try {
      const [sounds, value] = await Promise.all([catalog(), settings(contactId)]);
      if (current !== generation || !dialog.open) return;
      selected = value.tone;
      const globalTitle = [...sounds, ...special].find(tone => tone.id === value.globalTone)?.title || 'System default';
      const options = [...(contactId === null ? [] : [{id: null, title: 'Use global ringtone', detail: `Currently ${globalTitle}`}]), ...sounds, ...special];
      for (const option of options) {
        const row = element('div', null, 'message-tone-row');
        const label = element('label', null, 'message-tone-choice');
        const radio = element('input'); radio.type = 'radio'; radio.name = 'message-ringtone';
        radio.value = option.id ?? 'global'; radio.checked = selected === option.id;
        const copy = element('span'); copy.append(element('strong', option.title), element('small', option.detail));
        label.append(radio, copy); row.append(label);
        const tone = option.id ?? value.globalTone;
        const play = () => preview(tone, sounds).catch(error => { status.textContent = `Preview couldn’t play: ${error.message}`; });
        radio.addEventListener('change', () => { selected = option.id; save.disabled = selected === value.tone; status.textContent = ''; play(); });
        if (sounds.some(sound => sound.id === tone)) {
          const previewButton = button('▶', play, 'message-tone-preview');
          previewButton.setAttribute('aria-label', `Preview ${option.title}`); row.append(previewButton);
        }
        list.append(row);
      }
      status.textContent = 'Tap a tone to listen. Cancel keeps your current ringtone.';
      list.querySelector('input:checked')?.focus();
    } catch (error) {
      if (current === generation && dialog.open) status.textContent = error.message;
    }
  }

  async function refreshLabel(contactId = null) {
    const id = contactId === null ? 'global-message-tone-summary' : 'conversation-message-tone-summary';
    const node = document.getElementById(id);
    if (!node) return;
    node.textContent = 'Choose…';
    try {
      const [sounds, value] = await Promise.all([catalog(), settings(contactId)]);
      if (contactId !== null && app.settingsContactId !== contactId) return;
      const tone = [...sounds, ...special].find(item => item.id === value.effectiveTone)?.title || 'System default';
      node.textContent = value.tone === null ? `Global · ${tone}` : tone;
    } catch { node.textContent = 'Choose…'; }
  }

  document.getElementById('global-message-tone')?.addEventListener('click', () => open());
  document.getElementById('conversation-message-tone')?.addEventListener('click', () => {
    const contactId = app.settingsContactId;
    if (contactId) open(contactId);
  });
  return {
    refreshLabel,
    async notificationSound(contactId) {
      try {
        const [sounds, value] = await Promise.all([catalog(), settings(contactId)]);
        return notificationSoundOptions(value.effectiveTone, sounds);
      } catch { return {silent: true, filename: null}; }
    },
    playNotification(filename) {
      if (!filename) return;
      notificationAudio?.pause();
      notificationAudio = new Audio(`/sounds/${filename}`);
      // Browsers may require a prior user interaction before playing audio.
      notificationAudio.play().catch(() => {});
    },
  };
}
