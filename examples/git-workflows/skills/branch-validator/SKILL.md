---
name: branch-validator
description: Validate branch naming conventions. Use when checking branch names before creating PRs or committing.
---

# Branch Name Validator

Validates branch naming conventions to ensure consistency across the repository.

## When to Use

- Before creating a pull request
- When setting up a new branch
- As part of git-commit workflow to validate current branch
- In CI/CD pipelines for branch name validation

## Branch Naming Conventions

Branches should follow the pattern: `prefix/description`

### Valid Prefixes
- `feature/` - New features
- `fix/` or `bugfix/` - Bug fixes
- `chore/` - Maintenance tasks
- `refactor/` - Code refactoring
- `docs/` - Documentation changes
- `test/` - Test-related changes
- `hotfix/` - Critical production fixes
- `release/` - Release preparation
- `perf/` - Performance improvements
- `style/` - Code style changes
- `ci/` - CI/CD changes
- `build/` - Build system changes

### Description Rules
- Lowercase alphanumeric characters, hyphens, and underscores
- 3-50 characters in length
- No consecutive hyphens or underscores
- Cannot start or end with hyphen or underscore

### Examples
- ✅ `feature/add-user-authentication`
- ✅ `fix/login-error-handling`
- ✅ `chore/update-dependencies`
- ❌ `Feature/NewFeature` (uppercase)
- ❌ `feature/new feature` (spaces)
- ❌ `feature/ab` (too short)

## Usage

The validator can be invoked programmatically:

```bash
python3 skills/branch-validator/branch-name-validator.py <branch-name>
```

Exit codes:
- `0`: Valid branch name
- `1`: Invalid branch name
- `2`: Invalid usage

## Integration

This validator is used by:
- `create-pr` skill: Validates branch name before PR creation
- `git-commit` skill: Optionally validates branch name before committing
- CI/CD: Can be integrated into pre-commit hooks or CI checks
