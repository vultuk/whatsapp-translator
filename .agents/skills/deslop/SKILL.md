---
name: deslop
description: "Remove AI-generated code slop and clean up code style"
---


# Remove AI code slop

## Host compatibility

Follow [pstack host compatibility](../poteto-mode/compatibility.md) throughout this workflow. Use host-native tools and available subagents, inheriting model settings unless the user has explicitly configured supported overrides. A missing independent reviewer is a coverage gap, not a successful review. Work only within the requested scope; external messages, PRs, pushes, merges, deployments, deletion, and schedules require applicable user authorization. Preserve unrelated changes and required legal, safety, and API-contract information.


Check the diff against main and remove AI-generated slop introduced in the branch.

## Focus Areas

- Extra comments that are unnecessary or inconsistent with local style
- Defensive checks or try/catch blocks that are abnormal for trusted code paths
- Casts to `any` used only to bypass type issues
- Deeply nested code that should be simplified with early returns
- Other patterns inconsistent with the file and surrounding codebase

## Guardrails

- Keep behavior unchanged unless fixing a clear bug.
- Prefer minimal, focused edits over broad rewrites.
- Keep the final summary concise (1-3 sentences).
