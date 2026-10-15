---
name: no-comments
description: "Review scoped comments, remove supported redundancy, preserve contracts and safety constraints, and propose enforceable alternatives."
---

# No comments

Review comments in the caller's files or diff and remove unnecessary narration while preserving information the code cannot convey. Use the requested scope; otherwise inspect the current diff against the actual base branch, including working-tree changes.

## Review

Use a host-native read-only reviewer when available, or a separate review pass when it is not. Ask it to identify redundant narration, stale claims, commented-out dead code, and possible root-cause improvements. Each finding must cite a file and line and explain why the information is unnecessary or wrong. This replaces the unavailable Cursor Comment Sicko agent with an explicit review brief.

Preserve legal/license headers, public API contracts, required suppressions, external platform/protocol constraints, and issue/RFC links carrying rationale. If a comment or suppression protects correctness or safety, investigate before changing it. Ambiguity is a reason to retain and flag it, not deletion evidence. Read relevant code and use how or why when needed.

## Apply within scope

Review every finding before editing. Remove only supported redundant or stale comments. Do not change product behavior merely to eliminate a comment. When the user authorized a code fix, use the smallest verified root-cause correction; use architect for a substantive design question. Keep out-of-scope fixes as recommendations.

For constraints that could be enforced by types, tests, or lint, propose the concrete mechanism. Preserve the constraint comment until an approved mechanism actually enforces it. Do not remove safety suppressions merely to make a report look clean.

## Verify and report

Inspect the resulting diff, run proportionate checks for any behavior or tooling change, and report removed comments, retained constraints, proposed encodings, and unresolved findings. State whether independent review was available.
