---
description: Set up an isolated worktree to investigate a beads issue, gather context from its dependency graph, and produce a plan-skeleton + triage verdict (ready / needs-info / blocked). Use when the user says "investigate bd-XXXX", "let's look at bd-XXXX", or pastes a beads issue ID and asks what's needed to work on it.
---

# Investigate-Beads Skill

This skill takes a beads issue from "user pointed at it" to "isolated worktree on its own branch, with a plan skeleton and a triage verdict committed to it." It is the beads-issue counterpart to `triage.md` (which handles GitHub issues).

It does **not** implement the fix or finalize the design. It produces enough context to start a focused design session — or to recommend that the issue isn't ready yet.

## When to use

User says any of:
- "investigate bd-XXXX" / "let's look at bd-XXXX"
- "what would it take to work on bd-XXXX"
- pastes a beads ID and asks for context / scoping

**Do not** use for:
- Beads issues you've already scoped (just edit on `main` or an existing branch)
- GitHub-originated issues — use `/triage` instead, which handles the GH side and files a beads issue if needed
- Issues you're about to implement immediately in the current session — `br update <id> --status in_progress` and start working; this skill's overhead only earns its keep when the issue needs context-gathering before scoping

## Outcome: three durable artifacts

Every investigation produces:

1. **A worktree branch** `beads/<id>-<slug>` at `.worktrees/<id>-<slug>/`, with one commit containing the plan skeleton (and any investigative artifacts).
2. **A plan skeleton** at `claude-notes/plans/YYYY-MM-DD-<slug>.md` on that branch.
3. **A triage verdict** in the plan, plus design questions for the user — one of:
   - **Ready to design** — context clear, draft phases sketched, design questions ready for alignment.
   - **Needs more info** — specific questions that have to be answered before scoping makes sense.
   - **Not ready / blocked** — prerequisites missing, or `discovered-from` chain suggests the original problem was solved differently and the issue should be closed/deferred.

Investigative artifacts (small repros, exploratory snippets, notes you took while reading the dependency graph) live alongside the plan under `claude-notes/plans/<slug>-investigation/` and are committed with it.

## Steps

### 1. Pre-flight: verify HEAD is green

```bash
cargo xtask verify --skip-hub-build
```

Same rationale as `/triage`: catches "the issue is already broken at HEAD" vs. "you introduced it" confusion later, and surfaces environment problems before the user is invested. If it fails for a fresh-clone reason, fix it (usually `npm install` from repo root) and re-run.

If `verify` fails for a non-bootstrap reason, **stop and tell the user.** Don't investigate on a broken HEAD.

### 2. Read the issue

```bash
br show <id> --json
```

Read the description, status, type, priority, dates. Note who created it and when — old issues often have stale assumptions worth flagging.

### 3. Walk the dependency graph

This is the step that earns the skill its keep. A beads issue's *meaning* is usually richer than its description; the graph carries why-it-was-filed and what-blocks-what.

```bash
br dep tree <id>           # blocks / parent-child / discovered-from edges
```

For each linked issue, read it the same way. In particular:

- **`discovered-from` chain**: trace it. The originating issue (or session) usually has the context that explains *why* this one was filed — what the parent was trying to do when it surfaced this. Often the most informative single piece of context.
- **`blocks` edges (incoming)**: things that depend on this one. If the dependents are open, they pin the urgency. If they're closed, this issue may already have been addressed differently.
- **`related`**: same area of the codebase; useful for "how is this normally done here."

### 4. Read the referenced plan + code

If the description references a plan file (`claude-notes/plans/...`), read it. If it points at code paths (`crates/foo/src/bar.rs:line`), read those.

Spot-check the area: does the code the issue points at still exist with the same shape? Beads issues age — a six-month-old issue may have been overtaken by a refactor.

### 5. Create the worktree

Branch convention is `beads/<id>-<slug>` where `<slug>` is a short kebab-case form of the issue title (3–5 words, lowercase). The worktree directory mirrors the branch name.

```bash
git worktree add -b beads/<id>-<slug> .worktrees/<id>-<slug> main
```

Then add the beads redirect (the `.beads/` directory already exists from git):

```bash
echo "../../../.beads" > .worktrees/<id>-<slug>/.beads/redirect
```

Verify with `br where` from inside the worktree.

### 6. npm install (until `bd-7giz` lands)

Same as `/triage`: fresh worktrees have no `node_modules/`, and `cargo xtask verify` doesn't bootstrap it.

```bash
cd .worktrees/<id>-<slug>
npm install
cargo xtask verify --skip-hub-build  # confirm green at branch HEAD
```

When `bd-7giz` (`cargo xtask setup`) lands, replace `npm install` with that command and update this skill.

### 7. Write the plan skeleton

Create `claude-notes/plans/YYYY-MM-DD-<slug>.md` using the template below. Put any investigative scratch (small fixtures, exploratory grep output you want to preserve) under `claude-notes/plans/<slug>-investigation/`.

The plan **is a skeleton, not a finished plan.** Phases are draft headings with rough work items; the design questions section is where the real thinking still has to happen *with the user*.

### 8. Plan-skeleton template

```markdown
# <Issue title> (bd-XXXX)

**Date:** YYYY-MM-DD
**Beads:** bd-XXXX
**Worktree:** `.worktrees/<id>-<slug>` (branch `beads/<id>-<slug>`, based on `main` @ `<short-sha>`)
**Status:** Investigation — pending design alignment with user. **Do not start implementation until the user gives the go-ahead.**

## Triage verdict

One of:
- **Ready to design.** Context is clear; this plan sketches phases and lists design questions. Once those are settled, ready to implement.
- **Needs more info.** Specific questions (below) must be answered before this can be scoped.
- **Not ready / blocked.** Prerequisites unmet (list them), OR `discovered-from` context suggests this is overtaken / should be closed. Recommendation: <close | defer | wait on bd-YYYY>.

State the verdict in one sentence. The rest of the plan justifies it.

## Issue context

Quote or paraphrase the issue description. Note status, priority, type, age.

## Dependency graph

What the `dep tree` looks like, and what each edge tells us:

- **discovered-from**: <parent> — the original session was working on X when this surfaced because <Y>.
- **blocks**: <dependent issues, open or closed> — implies <urgency / no-longer-relevant / etc.>
- **related**: <neighbors> — useful as <model for how this kind of work usually looks here>.

If the graph is empty, say so explicitly — it changes the calculus (no incoming pressure, no clear context).

## What the code looks like today

Spot-check report: do the file paths in the description still exist? Has the area been refactored since the issue was filed? Is the symptom the issue describes still reproducible at HEAD?

If reproducible at HEAD, capture the smallest repro under `claude-notes/plans/<slug>-investigation/`.

If NOT reproducible (the issue may have been incidentally fixed), say so and recommend close.

## Proposed phases (draft)

Skeleton only — actual phase contents wait on the design discussion.

- Phase 0 — Test plan (TDD: failing tests written first).
- Phase 1 — <core change>
- Phase 2 — <integration>
- ...
- Phase N — Docs

## Open design questions for the user

Concrete, answerable questions that will let us turn the skeleton into a real plan. Examples:

1. **Scope.** Is this change limited to <X> or should it also cover <Y>?
2. **API surface.** Should we expose <thing> publicly, or keep it internal?
3. **Behavior under <edge case>.** What's the expected behavior when ...?

If the verdict is "not ready / blocked," replace this section with a "What's missing" list — what would have to land first.

## Risks / tradeoffs (draft)

If anything is already obvious from the investigation (e.g. "this touches a stage that has no tests", "this conflicts with bd-YYYY's direction"), note it. If you're not sure yet, say so.
```

### 9. Plan-skeleton commit

```bash
cd .worktrees/<id>-<slug>
git add -A
git commit -m "Investigate bd-XXXX: <one-line summary>"
```

Captures the plan skeleton + any investigative artifacts. Do not leave investigative files uncommitted — they are part of the record.

### 10. Beads issue: update status, do NOT close

After the investigation:

```bash
br update <id> --status in_progress
```

Even if the verdict is "not ready / blocked," leave the issue in `in_progress` — it has a worktree and a plan now, which is *progress*. Closing should only happen when the plan recommends close (overtaken / not reproducible) AND the user agrees.

If you discovered any incidental work (a separate gap that should be its own issue), file each as its own bd issue and link with `--deps related:<this-id>` or `--deps discovered-from:<this-id>`.

### 11. Beads JSONL changes go on `main`, not the worktree branch

Per `.claude/rules/worktrees.md`: with the redirect active, `br update`/`br create` writes to the main repo's `.beads/issues.jsonl`. That JSONL change is not visible from the worktree's `git status` and **must be committed from the main repo**, not from the worktree branch.

```bash
cd /path/to/main/repo  # not the worktree
git add .beads/
git commit -m "sync beads (bd-XXXX investigation)"
```

### 12. Hand back to the user

Report:
- the worktree path and branch
- the plan-skeleton path
- the verdict in one line
- the design questions verbatim (so the user can respond inline without opening the file)

The user takes it from there: answers the questions to turn the skeleton into a real plan, says "not now," or asks for more investigation.

## Anti-patterns

- **Skipping the dependency graph.** Reading only the issue description loses the "why was this filed" context that `discovered-from` carries. The graph is the highest-leverage step.
- **Writing a finished plan instead of a skeleton.** Real design happens in conversation; if the skeleton already pins the answer, the user has no room to redirect.
- **Closing "not ready" issues unilaterally.** Always make the close recommendation a question for the user, never a unilateral action.
- **Skipping pre-flight verify.** Same trap as `/triage`: hides bootstrap problems inside the investigation.
- **Forwarded TODOs in the open-questions section.** Each question should be specific and answerable. "Figure out the design" is not a design question.
- **Putting investigative artifacts in `/tmp`.** They are part of the durable record; commit them under `claude-notes/plans/<slug>-investigation/`.
- **Auto-spawning a worktree for a 5-minute lookup.** If the user just wants to know what an issue *is*, summarize from `br show` and stop. The skill's worktree overhead earns its keep when the investigation needs to write code (repros, fixtures), not when it's purely descriptive.
