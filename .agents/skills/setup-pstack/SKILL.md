---
name: setup-pstack
description: "Configure explicit, supported model and reasoning preferences for pstack roles in the current session or approved project guidance."
---

# Setup pstack

Configure explicit role preferences for the adapted pstack workflows. This is a portable adaptation of Cursor's setup workflow, not a global model-setting API.

## 1. Discover capabilities

Inspect the host's actual available subagent models and supported reasoning settings. Never invent a model identifier or combine a model and effort into an unsupported slug. If capability discovery is unavailable, offer inheritance and explain that overrides cannot be verified.

## 2. Inspect existing project guidance

Read applicable project instructions and any `.agents/pstack-models.md` already explicitly linked by those instructions. Preserve unrelated content. Default every role to `inherit-parent` and the parent's reasoning effort. The file is a documented preference map; it is not automatically discovered or loaded by ChatGPT/Codex.

## 3. Choose preferences with the user

Ask for desired reasoning effort and optional role overrides, using only supported choices. Cover implementation, difficult design, judgment/prose, how exploration/explanation, why investigation/synthesis, reflect reviewers/synthesis, arena candidates/judge, architect candidates, interrogate reviewers, and swarm workers. A panel list determines candidate count, subject to host concurrency limits. Do not describe repeated calls to one model as model diversity.

Show the concrete role table and destination before saving. The user's requested scope controls whether preferences are session-only or persistent. Preserve inherited settings for unspecified roles.

## 4. Save only the approved preferences

For session-only configuration, report the chosen table without writing files. For requested persistence, save `.agents/pstack-models.md` with the approved model identifiers and separate effort values. Explain that the host must be instructed to read this file. Add a small pointer to the existing project guidance only if that persistent convention is authorized; otherwise provide the pointer for review. Do not modify user-global Cursor rules or host settings.

## 5. Verify and hand off

Validate each override against current host capabilities. State what was saved, what consumes it, and any unsupported settings. Do not claim model preferences took effect unless a permitted host invocation verifies them. If the project lacks a verification skill, mention the bundled create-verification-skill as an optional next step without automatically running it.
