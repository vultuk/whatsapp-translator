---
name: reflect
description: "Spawn three parallel review subagents over the active transcript, surface learnings, and route each to a concrete edit on an existing skill. Use when the user says reflect."
---


# Reflect

## Host compatibility

Follow [pstack host compatibility](../poteto-mode/compatibility.md) throughout this workflow. Use host-native tools and available subagents, inheriting model settings unless the user has explicitly configured supported overrides. A missing independent reviewer is a coverage gap, not a successful review. Work only within the requested scope; external messages, PRs, pushes, merges, deployments, deletion, and schedules require applicable user authorization. Preserve unrelated changes and required legal, safety, and API-contract information.


Mine the current conversation for durable learnings, then route them into skill edits.

Invoke when the user says "reflect" or "/reflect". Skip when the conversation is trivial, off-topic, or already covered by an existing skill the parent followed correctly. One-offs are not learnings.

## 1. Locate the active transcript

Use only this conversation's visible context or a transcript explicitly supplied by the host or user for this task. Do not search other chats or private host storage. If no accessible transcript exists, prepare a concise digest from this conversation and state that limitation.

## 2. Review through three lenses

Use independent read-only reviewers for judgment, tooling, and divergent perspectives when host-native delegation is available. Inherit model settings unless explicitly configured. Give them the matching files in `references/` and the transcript or digest. They may read only relevant, authorized sources, and must not edit skills or external systems. If delegation is unavailable, use the same three lenses sequentially and report the reduced independence.

## 3. Synthesize

Use `references/synthesizer.md` to combine findings into Accepted / Rejected / Backlog. Use an independent synthesizer when available; otherwise synthesize in the current agent and disclose this. Verify citations through authorized read operations only.

## 4. Structural enforcement check

Move any Accepted item that a lint rule, script, metadata flag, or runtime check would enforce more reliably to Backlog. See the **encode-lessons-in-structure** principle skill.

## 5. Apply

Present the synthesizer's full Accepted / Rejected / Backlog output and wait for explicit approval before applying any Accepted edit. The user picks the subset and may redirect routings. Skill changes affect every future agent in the org. Do not auto-apply.

Draft Backlog items in the response. File them externally only when the user explicitly requests it.

Follow each approved row's Routing exactly:

- Trivial existing-skill edit (a one-line bullet, a tightened sentence, a stale fact corrected): the parent does it directly.
- Substantive existing-skill edit (a new section, a new pattern table, more than ~10 lines): use an available skill-authoring workflow and its draft / test / iterate loop; otherwise draft a portable SKILL.md with name, description, instructions, and proportionate verification.
- `tune description: <skill path>` (the skill exists but didn't trigger when it should have): use an available authoring workflow to evaluate and refine the description; do not claim an unavailable optimization loop ran.
- `new skill via create-skill: <kebab-name>`: use the host’s available authoring workflow or portable skill format; identify this as a local fallback, not the Cursor built-in.

If your environment ships a SKILL.md validator, run it on every touched skill before declaring done.

## 6. Summarize for the user

Short list, no preamble:

- Edits applied: `<skill path>`. What changed, one line each.
- New skills created: `<skill path>`. One line each (rare).
- Backlog filed to the devex tracker: `<issue title>` (`<tags>`). One line each.
- Dropped: one line per rejected finding + reason from the synthesizer.
