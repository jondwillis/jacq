---
name: fix-ci
description: Fix CI failures using GitHub CLI. Use when CI is broken and needs debugging.
---

# Fix CI

Let's fix whatever error we can find in ci using the `gh` cli.

## Steps
- Figure out which PR we have for the current branch
- Figure out which action is broken
- If nothing is broken, bail.
- Fetch the logs for the action
- Make a quick plan on what needs to be fixed
- Fix the error
