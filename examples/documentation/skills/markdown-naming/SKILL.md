---
name: markdown-naming
description: Naming conventions for markdown files across the project, including Cursor configuration files and general documentation. Use when creating or renaming any .md or .mdc files.
---

Naming Conventions for Markdown Files

These conventions ensure consistent, discoverable naming for all markdown files across the project. Use kebab-case (lowercase with hyphens) for all filenames.

## Cursor Configuration Files

- Rules (`.cursor/rules/*.mdc`):
  Pattern: {domain}-{purpose}.mdc
  Description: Domain prefix (e.g., design, api, git) with optional purpose for clarity.
  Examples: design.mdc, api-security.mdc, git-workflow.mdc

- Subagents (`.cursor/agents/*.md`):
  Pattern: {role}-{specialization}.md
  Description: Role prefix (e.g., backend, frontend, code) and specialization suffix.
  Examples: backend-eng.md, code-reviewer.md, test-writer.md

- Skills (`.cursor/skills/*/SKILL.md`):
  Pattern: {action}-{target}.md
  Description: Action performed and its target, using kebab-case.
  Examples: create-ticket/SKILL.md, generate-types/SKILL.md, update-status/SKILL.md

## General Guidelines

- Always use kebab-case (lowercase with hyphens)
- Be descriptive but concise (2-4 words typically)
- Avoid abbreviations unless widely understood
- Use consistent patterns within each category
