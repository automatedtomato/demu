---
name: pre-merge-review
description: Comprehensive security and quality review of changed code. Run before every commit and before merging to main. Blocks on CRITICAL or HIGH issues.
origin: mesh-ant
---

# Pre-Merge Review

Comprehensive security and quality review of changed code.
**Must be run before every commit and before every merge to main.**

**Never approve code with security vulnerabilities.**

## When to Activate

- **Before every commit** — catch issues before they enter history
- Before opening a pull request
- Before merging a feature branch to main
- When explicitly asked to review changed or unmerged code

---

## Process

### Step 1 — Get changed files

**Before committing** (review staged + unstaged working changes):
```bash
git diff --name-only HEAD
```

**Before merging** (review everything not yet in main):
```bash
git diff --name-only main...HEAD
```

For each changed file, run the checks below.

---

## Check Categories

### CRITICAL — Security Issues

Block merge immediately if any of the following are found.

| Issue | What to look for |
|---|---|
| Hardcoded credentials | Strings matching patterns: `password=`, `secret=`, `api_key=`, `token=`, `Bearer `, `sk-`, `-----BEGIN` |
| API keys / tokens | Any long alphanumeric string assigned to a key-like variable |
| SQL injection | String concatenation or `fmt.Sprintf` used to build SQL queries; raw query interpolation |
| XSS | Unescaped user input rendered into HTML; `template.HTML(userInput)` in Go; `dangerouslySetInnerHTML` in JS |
| Missing input validation | User-facing inputs (HTTP handlers, CLI args, file reads) used without validation |
| Insecure dependencies | New dependencies added without pinned versions; known-vulnerable packages |
| Path traversal | File paths constructed from user input without cleaning (`filepath.Clean`, `path.Clean`) |

---

### HIGH — Code Quality

Report all occurrences. Block merge if pattern is pervasive (> 3 instances).

| Issue | Threshold |
|---|---|
| Long functions | > 50 lines |
| Large files | > 800 lines |
| Deep nesting | > 4 levels of indent |
| Missing error handling | `err` assigned but not checked; `_` used to discard errors from fallible operations |
| Debug logging left in | `console.log`, `fmt.Println`, `log.Println` used in non-test, non-main code without a clear purpose |
| TODO / FIXME comments | Any `TODO`, `FIXME`, `HACK`, `XXX` comment in committed code |
| Missing docs for public APIs | Exported functions, types, or constants without doc comments |

---

### MEDIUM — Best Practices

Report but do not block merge.

| Issue | What to look for |
|---|---|
| Mutation patterns | In-place mutation of shared state where an immutable alternative exists |
| Emoji in code / comments | Any emoji character in source files (not in markdown docs) |
| Missing tests | New exported functions or types with no corresponding test coverage |
| Accessibility issues | In frontend code: missing `aria-*`, unlabelled form inputs, non-semantic HTML |

---

### LOW — Style and Housekeeping

Report for awareness only.

- Inconsistent naming conventions within a file
- Unused imports
- Commented-out code blocks
- Overly long lines (> 120 chars in Go, > 100 in TS/JS)

---

## Report Format

For each issue found, report:

```
[SEVERITY] filename:line_number
Issue: <description of the problem>
Fix:   <concrete suggested fix>
```

Example:

```
[CRITICAL] backend/app/config.py:42
Issue: Hardcoded API key assigned to ZEP_API_KEY variable.
Fix:   Move to environment variable and load via os.environ.get("ZEP_API_KEY").

[HIGH] meshant/loader/loader.go:88
Issue: Error from json.Unmarshal discarded with _.
Fix:   Check and return or wrap the error.

[MEDIUM] meshant/schema/trace.go:31
Issue: Exported TagValue type has no doc comment.
Fix:   Add: // TagValue is ...
```

---

## Verdict

End the review with one of:

### ✅ APPROVED
No CRITICAL or HIGH issues found. Safe to merge.

### ⚠️ APPROVED WITH NOTES
No CRITICAL or HIGH issues, but MEDIUM/LOW items worth addressing before merge.
List them explicitly.

### 🚫 BLOCKED — CRITICAL
One or more CRITICAL security issues found. Do not merge. Fix required.

### 🔴 BLOCKED — HIGH
One or more HIGH quality issues found. Fix before merge.

---

## Blocking Policy

| Severity | Action |
|---|---|
| CRITICAL | Hard block. Do not merge under any circumstances. |
| HIGH | Block. Fix before merge unless user explicitly overrides with documented reason. |
| MEDIUM | Recommend fix. Do not block. |
| LOW | Informational only. |
