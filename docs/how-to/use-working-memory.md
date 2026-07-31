# Use working memory for multi-phase work

Use a repository-level working-memory file when a task is too large to complete
safely in one context window, spans multiple commits, or requires substantial
research and approval. Its purpose is to let the next agent—or the same agent
after context compaction—resume from verified facts instead of reconstructing
intent from a partial conversation.

Working memory is an operational checkpoint, not a transcript. Keep it factual, current, and useful for the next action.

## When to use it

Create or update `working-memory.md` when work has one or more of these properties:

- several dependent implementation phases;
- research across Void, upstream Zed, and official documentation;
- consequential lifecycle, persistence, Git, process, updater, or safety decisions;
- approval gates or unresolved product questions;
- validation that must be repeated after each phase;
- likely context compaction or handoff to another agent.

A small, isolated change usually does not need working memory.

## Start with evidence

Before proposing phases:

1. Read `AGENTS.md`, the maintainer's request, existing architecture
   documentation, relevant ADRs, and the current working memory.
2. Inspect the working tree and recent commits. Preserve unrelated changes.
3. Trace the existing implementation, call sites, tests, persistence, ownership, and teardown paths.
4. For applicable domains, inspect the pinned local Zed implementation and current official documentation.
5. Record what is implemented, partial, absent, or uncertain. Do not infer product behavior from names or plans.
6. Convert the request into concrete acceptance criteria and identify decisions that require maintainer approval.

Only then divide the work into phases.

## Design phases that can be inherited

Each phase should be a coherent, buildable checkpoint with:

- a specific outcome rather than a broad activity;
- dependencies on earlier phases made explicit;
- the files or domains expected to change;
- relevant upstream sources and symbols;
- intentional differences from upstream;
- focused tests and repository checks;
- approval gates for consequential ambiguity;
- a commit boundary when the phase is complete.

Prefer the smallest dependency order that keeps the repository truthful. For
example, establish authoritative state before moving its views, and establish
safe deletion invariants before changing destructive UI flows.

Do not create phases merely to make a plan look comprehensive. Do not add
speculative abstractions or unrequested product behavior.

## What working memory should contain

Keep these sections current:

### Status

State the current phase, the last completed checkpoint or commit, whether the
tree is clean, and the exact next action. A new agent should understand the
current position from this section alone.

### Goal and acceptance criteria

Restate the maintainer's request as observable outcomes. Record explicit non-goals so later agents do not expand scope.

### Constraints and invariants

Record requirements that must survive every phase, such as compatibility,
migration immutability, cancellation ownership, destructive-operation safety,
public API preservation, or prohibited manual testing.

### Baseline and evidence

Record:

- the starting commit;
- baseline checks and test count;
- relevant repository observations;
- exact pinned upstream commit, paths, symbols, and tests;
- official documentation consulted;
- read-only data inspection when compatibility depends on existing state.

Distinguish inspected facts from assumptions.

### Phase plan

List phases in dependency order. For each phase, state its purpose, likely
scope, validation, and any approval gate. Mark phases as planned, in progress,
blocked, or complete.

### Active work log

Maintain an ordered checklist of meaningful research, implementation,
documentation, validation, and commit steps. Update it while working—not only
at the end—so a handoff can distinguish the current step from planned work.
Record discoveries that changed the plan and the evidence behind them. Do not
record every routine tool invocation or turn the file into a conversation
transcript.

### Completion checkpoints

After each phase, record:

- behavior actually changed;
- lifecycle and error-path implications;
- tests added or changed;
- documentation and ADRs updated;
- exact commands run and their results;
- commit hash and title after committing;
- intentional differences from the reference implementation;
- remaining limitations and manual verification.

Never write “verified” when a command was not run. Record failures and deferrals exactly.

### Decisions and unresolved questions

Capture decisions that affect later work, including rejected alternatives and
why they were rejected. When a decision materially affects architecture,
lifecycle, persistence, compatibility, safety, or product behavior, create or
update a file under `docs/decisions/` in the same phase. Record its context,
constraints, considered options, decision, consequences, status, and primary
references. Keep only the operational summary and ADR link in working memory.

### Next action after compaction

End every active checkpoint with an executable resumption instruction. It
should tell the next agent what to read, what state to confirm, and the first
bounded task to perform.

Example:

> Read `AGENTS.md` and `working-memory.md`, confirm commit `abc1234` and a clean
> tree, then trace terminal release ownership in the listed Void and Zed symbols
> before changing code.

Avoid vague instructions such as “continue cleanup.”

## Update cadence

Update working memory at these moments:

1. after the initial repository and upstream investigation;
2. when the maintainer resolves an approval gate;
3. after a discovery changes the plan or invalidates an assumption;
4. after implementation and focused validation of each phase;
5. after committing, with the real commit hash in the next normal checkpoint
   update;
6. before likely context compaction or handoff;
7. at task completion, with remaining manual work clearly assigned.

Update existing status and plan sections instead of only appending. Historical
checkpoints may retain their old next actions as evidence when they are clearly
marked complete, but the top-level status must expose only the current next
action.

A commit cannot contain its own hash. If working memory is part of the phase
commit, record that hash in the next normal working-memory update. Do not amend
or create an otherwise empty commit solely to insert a hash.

## Remove working memory before a pull request

`working-memory.md` is branch-local scaffolding and must not be included in the
final pull request. Before creating or updating the PR:

1. finish or explicitly defer every active step;
2. transfer lasting architecture, behavior, constraints, limitations, and
   references into code comments, tests, `README.md`, architecture docs, or an
   ADR as appropriate;
3. ensure commit messages preserve useful implementation checkpoints;
4. remove `working-memory.md` and commit that removal with the final PR
   preparation changes;
5. verify the branch diff contains no working-memory file or references that
   depend on it.

Do not delete working memory early: it remains the recovery source until the
implementation, documentation, and validation are complete. Do not paste the
whole file into the PR body; summarize the integrated result according to
[`pr.md`](pr.md).

## Relationship to other project records

Working memory does not replace durable project documentation:

- `README.md` explains setup and current product orientation.
- `docs/architecture.md` explains current boundaries and lifecycle.
- ADRs explain significant durable decisions and their consequences.
- Code and tests define behavior.
- Git commits are immutable implementation checkpoints.
- Working memory explains current task state, evidence, sequencing, and handoff.

When a phase changes architecture or behavior, update the durable documentation
in the same phase. Link to it from working memory rather than duplicating its
full contents.

## Validation and commit discipline

At the end of each implementation phase:

1. run focused tests while iterating;
2. run the repository-required checks before declaring the phase complete;
3. inspect the final diff and documentation consistency;
4. commit only the coherent phase according to [`commit.md`](commit.md);
5. record the commit and verification results in working memory;
6. confirm the working tree is clean or explicitly list intentional leftovers.

Automated checks do not replace native, destructive, accessibility, process, or
signed-update testing. Record those boundaries and provide a maintainer
checklist rather than silently claiming completion.

## Recovery procedure for a new agent

After context compaction or handoff:

1. Read `AGENTS.md` and all of `working-memory.md`.
2. Confirm the current branch, `HEAD`, and working-tree status.
3. Compare the latest checkpoint with recent commits and the actual files.
4. Re-run or inspect the most relevant focused check if state is uncertain.
5. Read the referenced ADRs, upstream sources, and official documentation for the next phase.
6. Resume only the stated next bounded action.
7. Correct working memory immediately if repository evidence disagrees with it.

Do not trust a summary over the repository. Working memory is useful because it
points to evidence, not because it is inherently authoritative.

## Copyable template

```markdown
# Working memory: <task>

## Status

- Current phase: <number and name>
- Last completed commit: `<hash>` — `<title>`
- Working tree: <clean / describe intentional changes>
- Next action: <specific bounded action>

## Goal

<Concrete acceptance criteria and non-goals.>

## Constraints and invariants

- <Safety, compatibility, lifecycle, UX, or scope requirement>

## Baseline

- Starting commit: `<hash>`
- Existing behavior: <implemented / partial / absent>
- Baseline verification: <commands and results>

## Research evidence

- Void: `<path>::<symbol>` — <relevance>
- Zed commit: `<hash>`
- Zed: `<path>::<symbol>` — <what can be adapted>
- Official documentation: <URL and relevant contract>
- Intentional difference: <difference and reason>

## Phase plan

1. [ ] **<Phase>** — <outcome, scope, validation>
2. [ ] **<Phase>** — <outcome, scope, validation>

## Active work log

- [x] <Meaningful completed research or implementation step and evidence>
- [ ] **Current:** <bounded step in progress>
- [ ] <Next planned step>

## Decisions and approval gates

- Decided: <decision and reason; link ADR when durable>
- Needs approval: <focused consequential question>

## Phase <n> completion checkpoint

- Changed: <verified behavior>
- Lifecycle/error handling: <ownership, cancellation, cleanup, failures>
- Tests/docs: <what changed>
- Verification:
  - `<command>` — <result>
- Commit: `<hash>` — `<title>`
- Remaining limitations: <manual or deferred work>

Next action after compaction: <read, confirm, then perform one bounded action>.
```

## Quality test

Before relying on the file for handoff, ask:

- Can a new agent identify the exact current state in under two minutes?
- Can every important claim be checked against a file, command result, commit, or primary source?
- Are completed, deferred, blocked, and unverified work clearly distinct?
- Are destructive or product-level decisions explicitly approved?
- Is the next action small enough to begin without reconstructing the whole conversation?

If not, revise the working memory before continuing.
