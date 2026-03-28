---
name: requirements
description: >-
  Elicits, structures, and validates software requirements before design or
  implementation. Use when starting a new feature or project, entering plan
  mode, scoping work, or when the user asks for requirements, user stories,
  acceptance criteria, constraints, or a lightweight PRD.
---

# Requirements

## When to apply

Use this skill whenever the goal or scope is unclear, or before significant design or coding. Prefer **questions first**, then one structured artifact the user can approve.

## Workflow

1. **Frame** — Problem, primary users, and why now (one or two sentences each). If unknown, ask.
2. **Scope** — In scope, **explicit non-goals**, and dependencies on other systems or teams.
3. **Behavior** — Capabilities as **testable** statements (given/when/then or bullet acceptance criteria). Separate **must-have** vs **should-have** vs **later** when helpful.
4. **Quality bar** — Non-functionals that matter: performance, security, accessibility, reliability, observability, offline, i18n, etc. Only items the user cares about; skip generic lists.
5. **Constraints** — Hard limits: languages, platforms, deadlines, compliance, existing APIs, “must not break X.”
6. **Risks & unknowns** — Open questions, assumptions to validate, spike work if any.
7. **Done** — Definition of done: what demos, tests, or sign-off closes the requirement.

Work in **plan mode** until the user confirms the requirements summary; do not implement large changes based on guessed requirements.

## Elicitation rules

- Ask **short, targeted** questions; batch related questions instead of one-at-a-time ping-pong when possible.
- Reflect back **misunderstandings** explicitly (“You said A; did you mean B for edge case C?”).
- Prefer **concrete examples** (sample inputs, user flows, failure cases) over abstract adjectives.
- If the user hands a solution (“use library X”), still capture the **underlying need** it serves.

## Output template

Deliver requirements in this shape (adjust headings if the project is tiny):

```markdown
# [Feature or project name]

## Context
- Problem:
- Users:
- Success (how we know it worked):

## Scope
- In scope:
- Out of scope:
- Depends on:

## Functional requirements
- FR1: … (acceptance: …)
- FR2: …

## Non-functional requirements
- NFR1: …

## Constraints
- …

## Open questions
- …

## Definition of done
- …
```

## After approval

Once the user accepts the requirements doc, treat it as the source of truth for subsequent design and implementation unless they explicitly revise it.
