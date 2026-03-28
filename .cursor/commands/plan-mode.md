# Plan mode

You are in **plan mode**. The user invoked this command to explore and settle on an approach before significant implementation.

## Default behavior (until the user says "implementation-mode")

1. **Listen first** — Use the user’s messages as the primary source of truth. Ask questions when goals, constraints, or context are unclear; label assumptions when you must infer.
2. **Analyze code; do not implement** — You **may** read, search, and inspect the existing codebase (and run **read-only** checks such as listing files or non-destructive commands the user approves) to ground the plan in reality. You **must not** implement: **no** edits, **no** new/deleted files, **no** patches, **no** dependency installs, **no** migrations or other steps that change the project or environment. Pure analysis only.
3. **Plan** — Produce and refine a **plan**: viable approaches, tradeoffs, ordered steps, what to verify after each step, risks, and what “done” means overall.
4. **Phased implementation** — When useful, **split implementation into multiple phases** (e.g. foundation → feature → polish). For **each phase**, state: goal, ordered tasks, **which files** are in scope (subset of the full inventory), how to verify the phase is complete, and dependencies on prior phases. A single phase is fine for small work; many phases are fine for large or risky work.
5. **File-level change list** — Include a **full** inventory of all file impact across the whole effort. Also tie each file (or group) to the **phase** where it first changes when you use phases. For each path (or glob if truly unknown), state **create**, **modify**, or **delete**, and one line on **what** would change. If a path is not yet known, say what discovery step would locate it, then list it once identified. Do not omit files the user would need to touch.
6. **Iterate** — When they push back or add detail, update the plan, phases, and file inventory; keep all concise.

## When the user says "implementation-mode"

They are **leaving** strict plan-only behavior and **authorizing implementation**.

1. **Acknowledge** — Briefly confirm you’re switching to implementation.
2. **Anchor** — If the plan is clear, restate the minimal checklist and **file list** for the work ahead (for **phased** plans, state **which phase** you are executing first unless they specified otherwise). If something critical is still ambiguous, ask **one or two** blocking questions first, then proceed.
3. **Execute** — Implement according to the agreed plan and their instructions. If the plan has **phases**, default to **one phase at a time** (complete and verify before starting the next) unless they ask to run multiple phases in one go.

If they say "implementation-mode" but also narrow scope (“only fix X”), follow the narrower instruction.

## Output style

- Use **short** plan summaries, **numbered** steps, **phases** (when split), the **file change table or bullet list** (with phase tags if phased), and explicit **open questions** when needed.
- Avoid long essays; make the plan easy to edit in the next message.
