---
name: compare-and-contrast
description: When facing a design decision with two or more viable options, present a structured comparison, make a clear recommendation with full reasoning, and wait for user confirmation before implementing.
origin: mesh-ant
---

# Compare and Contrast

Use this skill when a design decision has two or more genuinely viable options
and the choice has meaningful consequences for the project's direction.

## When to Activate

- Two or more options exist and neither is obviously wrong
- The choice encodes assumptions about the project's philosophy or architecture
- The decision is hard to reverse after implementation (type changes, schema changes, core abstractions)
- The user asks you to compare options before deciding
- You surface a design question mid-implementation that wasn't in the original plan

## When NOT to Activate

- One option is clearly superior and reasoning is trivial
- The choice is cosmetic (naming, formatting) with no architectural consequence
- The user has already decided and just wants implementation

---

## Structure

### 1. Option A — [name] (current / default)

Present as a code block or brief definition, then:

**Pros:**
- Concrete advantages relevant to this project's goals

**Cons:**
- Concrete disadvantages, especially where the option encodes hidden assumptions
  or forecloses future choices

### 2. Option B — [name]

Same structure.

### 3. Recommendation: Option [X]

State the choice clearly upfront. Then explain:

**What made you choose it?**
The core reason — not a list of pros, but the single deciding factor.

**Why is it better for this project specifically?**
Ground the reasoning in the project's actual principles, goals, or constraints —
not generic software advice.

**How does this decision make a difference?**
Be concrete. Show what becomes possible or impossible with each option.
If available, give a before/after example of how the data or code changes.

**What does this decision cost?**
State the real trade-off honestly. A good recommendation acknowledges the cost
of the chosen option.

---

## Confirmation Gate

End with:

> **WAITING FOR CONFIRMATION** — Proceed with [Option X]? (yes / modify / different approach)

Do NOT implement anything until the user explicitly confirms.

---

## Example

```
## Source field: `string` vs `[]string`

### Option A — `string` (current)

```go
Source string `json:"source,omitempty"`
```

**Pros:**
- Simple. Flat JSON. Easy to write by hand.

**Cons:**
- Forces collapse of multiple producers into one name.
- Installs a bias that agency is always attributable to a single thing.
- Premature singularization: a quiet ontological commitment.

### Option B — `[]string`

```go
Source []string `json:"source,omitempty"`
```

**Pros:**
- Honest about distributed agency. The rate-limiter AND the queue policy AND
  the system clock can all be named.
- A single source is still trivially representable: `["rate-limiter"]`.
- Defers the question "which one matters?" to analysis, not schema definition.

**Cons:**
- Slightly more verbose JSON: `"source": ["rate-limiter"]`.
- Small breaking change if switched later (string → array is a JSON schema break).

### Recommendation: Option B — `[]string`

**What made you choose it?**
`string` performs a premature singularization of agency at the schema level,
before any trace has been followed.

**Why is it better for this project specifically?**
MeshAnt's core commitment is to resist premature singularization. A plain
string pre-answers "who produced this?" by forcing a single name — exactly
what the framework wants to defer.

**How does this decision make a difference?**
A delay caused by a rule *and* a queue can be written as:
  `"source": ["queue-policy-v3", "rate-limiter"]`
instead of being forced to pick one and silently drop the other.

**What does this decision cost?**
Slightly more verbose JSON. Marginally more complex hand-crafted trace data.
A breaking change if we reverse it later — so `[]string` is also the safer
default to lock in now.

**WAITING FOR CONFIRMATION** — Proceed with `[]string`? (yes / modify / different approach)
```

---

## Principles for MeshAnt decisions specifically

When applying this skill to MeshAnt design questions, ground the comparison in
the project's eight principles (see `docs/principles.md`):

- Does one option perform premature actor definition? (Principle 1)
- Does one option encode an ontological assumption the project is trying to defer? (Principle 2)
- Does one option reduce everything to a single causal attribution? (Principle 3)
- Does one option assign a role before difference has been followed? (Principle 4)
- Does one option collapse multiple observer positions into one? (Principle 5)
- Is one option harder to re-articulate later? (Principle 6)
- Does one option erase friction or asymmetry by smoothing it away? (Principle 7)
- Does one option hide the cut being made? (Principle 8)

These are not checkboxes — use whichever is relevant to the specific decision.
