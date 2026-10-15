# ChatGPT/Codex compatibility

These rules adapt the retained upstream playbooks. Apply them before following any playbook instruction.

- User and project instructions govern scope, approvals, storage, builds, GUI use, and cleanup. Importing or invoking this style does not authorize external messages, PRs, merges, releases, deployments, destructive cleanup, or schedules.
- Translate Cursor Task/poteto-agent calls to available host-native delegation. Inherit model settings. Missing independent reviewers must be reported, not simulated as verified review.
- The [plugin catalog](../../skills/README.md#skills) lists bundled pstack, principle, and cursor-team-kit skills. Read a named skill before applying it. Cursor’s built-in create-skill, Origin, and Cursor-specific rules remain external integrations; use supported host authoring/tools or report the gap. Never claim an unavailable integration ran.
- Use the host's supported question, task tracking, browser, and computer tools. Prefer the project's existing skill location, otherwise `.agents/skills/`. Cursor transcript paths are not Codex transcript paths. Do not invent replacements or claim absent transcripts establish inactivity.
- Upstream playbooks that say to open a PR, merge, send a message, or publish apply only when the user authorized that action. Otherwise finish local reviewable work and report the pending action.
- Cleanup requires checking ownership, live processes, uncommitted/untracked/ignored data, and remote preservation under the project's storage policy. Never force-remove a worktree. The retained worktree audit helper reads Cursor transcript locations and cannot establish Codex task ownership; use it only as partial diagnostic evidence, never as deletion authority.

## Bundled helper requirements

The `.agents/skills/poteto-mode/scripts/` directory retains upstream helpers and tests. Bun is required for orchestration and PR watching; Node.js is required for the plan checker; Git and authenticated `gh` are required for GitHub reads/actions; `jq` is required to build the worktree audit's PR inventory; ripgrep (`rg`) is required for its recent-chat lookup. Some playbooks mention optional Origin tooling. No service connection or credentials are bundled.

From `.agents/skills/poteto-mode/scripts/`, install with `bun install --frozen-lockfile`, respecting the project's install runner and preflight policy. Run `bun run test` and `bun run typecheck` for fixture tests and TypeScript checks. No mixed package manager or shared node_modules symlink.

The upstream bootstrap may install dependencies automatically on first helper invocation. Complete installation explicitly through the required project runner before invoking wrappers, or run the wrapper through that runner so any bootstrap install is covered. Scripts can modify local orchestration state or GitHub state depending on command; inspect the relevant playbook and command before use. Do not run live drive/merge/cleanup commands merely to test an import.

The retained examples and helper tests do not prove host integration, live PR operations, cleanup safety, or complete autonomous workflows.

## Explicit role preferences

When project instructions explicitly link `.agents/pstack-models.md`, read its user-approved role preferences before delegation and validate them against current host capabilities. Otherwise inherit the parent model and effort. The map is not automatically loaded or a host settings API.
