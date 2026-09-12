export function normalizeConversationSettings(value = {}) {
  return {
    translationEnabled: value.translationEnabled === true,
    languageOverride: value.languageOverride || null,
    translationStyle: value.translationStyle || null,
    sendOriginalFollowUp: value.sendOriginalFollowUp === true,
  };
}

// Only acknowledged server settings drive the UI. A late read cannot replace a
// newer save or a settings event from another connected device.
export class ConversationSettingsClient {
  constructor({request, isDemo = () => false, onChange = () => {}}) {
    this.request = request;
    this.isDemo = isDemo;
    this.onChange = onChange;
    this.values = new Map();
    this.revisions = new Map();
  }
  get(id) { return this.values.get(id); }
  apply(id, value) {
    const settings = normalizeConversationSettings(value);
    this.revisions.set(id, (this.revisions.get(id) || 0) + 1);
    this.values.set(id, settings);
    this.onChange(id, settings);
    return settings;
  }
  async load(id) {
    if (this.isDemo()) return this.get(id) || this.apply(id, {});
    const revision = this.revisions.get(id) || 0;
    const response = await this.request(`/api/contacts/${encodeURIComponent(id)}/settings`);
    if (!response.ok) throw new Error('Could not load conversation settings. Please try again.');
    const settings = await response.json();
    if ((this.revisions.get(id) || 0) !== revision) return this.get(id);
    return this.apply(id, settings);
  }
  async save(id, value) {
    const settings = normalizeConversationSettings(value);
    if (this.isDemo()) return this.apply(id, settings);
    const response = await this.request(`/api/contacts/${encodeURIComponent(id)}/settings`, {
      method: 'PUT', headers: {'Content-Type': 'application/json'}, body: JSON.stringify(settings),
    });
    const saved = await response.json();
    if (!response.ok || saved.success !== true) throw new Error(saved.error || 'Could not save conversation settings. Please try again.');
    return this.apply(id, saved);
  }
}
