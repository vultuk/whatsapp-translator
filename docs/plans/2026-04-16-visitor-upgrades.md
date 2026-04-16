# Visitor Upgrades Implementation Plan

> **For Hermes:** Use subagent-driven-development skill to implement this plan task-by-task.

**Goal:** Make the WhatsApp Translator web UI feel much more usable for a real day-to-day visitor by adding search, saved drafts, and saved/starred messages.

**Architecture:** Keep the changes frontend-first so they work immediately without backend schema migrations. Persist user-specific UI state in `localStorage`, factor the state logic into a reusable helper module, and wire the existing `app.js` UI to that module.

**Tech Stack:** Vanilla JavaScript, static HTML/CSS, Node built-in test runner.

---

## Planned features

1. **Per-chat draft persistence** — unsent text is saved automatically per conversation, restored when you come back, and shown in the contact preview.
2. **Starred messages** — any message can be starred/unstarred, with a quick "starred only" view in the current chat.
3. **In-chat search** — search messages in the open conversation with instant filtering and visible match counts.

## Files to touch

- Modify: `web/public/index.html`
- Modify: `web/public/styles.css`
- Modify: `web/public/app.js`
- Create: `web/public/app-state.js`
- Modify: `web/package.json`
- Create: `web/tests/app-state.test.js`

## Test strategy

- Add pure helper functions to `web/public/app-state.js`
- Test draft persistence logic, starred message toggling/filtering, and in-chat search matching via `node --test`
- Run the Node test suite from `web/`
