# AGENTS.md

## Mission

Build **Void**, an AI-first coding workspace written in idiomatic Rust with Zed's
GPUI. Void runs multiple coding agents concurrently in first-class terminals,
with each agent working safely in its own Git branch and worktree.

The maintainer's request is the product specification. Implement it faithfully:
do not reinterpret it, add speculative behavior, or expand scope without
approval.

## How to work

Seek evidence before writing code. Work like a careful senior engineer and
long-term maintainer: understand the system, make the smallest truthful change,
verify it, and leave the repository easier to inherit.

1. **Inspect Void first.** Understand the existing code, conventions,
   documentation, and relevant tests before proposing new structures.
2. **Research before designing.** For editors, workspaces, terminals, panes,
   projects, Git, settings, actions, keybindings, persistence, and GPUI
   behavior, inspect the equivalent implementation in the local Zed repository
   at `/Users/usama/Documents/archive/zed`.
3. **Use current primary sources.** Consult <https://gpui.rs/> for every GPUI
   API or lifecycle assumption. Consult current official documentation for
   other libraries, protocols, APIs, CLIs, and tools. Do not rely solely on
   memory, model knowledge, blogs, or isolated search snippets.
4. **Trace the whole implementation.** Read the relevant Zed crates, manifests,
   types, actions, models, call sites, persistence, tests, and teardown paths,
   not only the visible component. Record the local Zed commit and exact source
   paths and symbols used.
5. **Port and adapt; do not casually redesign.** Preserve proven Zed
   architecture and GPUI idioms where they fit Void. Explain and obtain
   approval before a material architectural deviation, foundational
   abstraction, broad refactor, or unspecified UX decision.
6. **Do not guess consequential details.** First inspect Void, official docs,
   and Zed. If ambiguity still affects UX, architecture, data flow,
   compatibility, safety, or persistence, ask a focused question.
7. **Implement incrementally.** Keep changes narrow, buildable, and free of
   unrelated refactors, dependency swaps, formatting churn, and speculative
   features.
8. **Verify honestly.** Never present plausible-looking or partially verified
   work as complete. Report failures and uncertainty precisely.

If official GPUI documentation and the checked-out Zed source disagree, verify
their versions and report the mismatch. Never silently combine incompatible
APIs.

## Implementation standard

- Write clear, idiomatic, production-quality Rust. Make ownership, state,
  control flow, lifecycle, cancellation, and failure explicit.
- Prefer focused domain types and enums over stringly typed state, small
  responsibilities over oversized managers, and simple code over clever
  abstractions.
- Follow the relevant Zed patterns for GPUI entities, contexts, views, weak
  handles, subscriptions, actions, tasks, focus, notifications, and teardown.
- Never block GPUI's foreground executor. Run process work, Git operations,
  expensive I/O, and agent work asynchronously with explicit task ownership and
  cancellation.
- Use structured errors with actionable context. Avoid `unwrap`, `expect`,
  ignored errors, unnecessary cloning, broad shared mutable state, and global
  mutable state unless a proven invariant justifies the choice.
- Avoid `unsafe`. If it is unavoidable, isolate it, document its invariants,
  justify it, and add focused tests.
- Keep UI rendering, domain state, persistence, process execution, and Git
  operations separated according to the closest Zed implementation.
- Keep public APIs minimal. Comment invariants and non-obvious reasoning, not
  line-by-line mechanics.

## Product invariants

### UX fidelity

- Treat requested dimensions, placement, hierarchy, labels, interactions,
  shortcuts, focus behavior, accessibility, and state transitions as acceptance
  criteria.
- Reuse Zed's interaction patterns where the maintainer has not requested a
  difference.
- Do not add visible controls, dialogs, notifications, or workflow steps merely
  because they seem useful.
- Account for relevant loading, empty, error, disabled, cancellation, and
  recovery states.

### Agents and terminals

- Multiple agent sessions must run concurrently without blocking one another or
  the UI.
- Each session has explicit identity, lifecycle, status, cancellation,
  output/logs, and error state. State and failures must not leak across
  sessions.
- Agent terminals are first-class workspace surfaces. Follow Zed's patterns for
  terminal creation, rendering, input, focus, resize, persistence, process I/O,
  and teardown.

### Git isolation and safety

- Concurrent agents work in isolated branches and worktrees unless the
  maintainer explicitly requests another model. Never let two agents implicitly
  mutate the same working tree.
- Keep the mapping among workspace, session, terminal, repository, worktree,
  and branch explicit and visible.
- Validate repositories, dirty state, branch/worktree collisions, checkout
  failures, and stale worktrees before agent startup, and surface actionable
  errors.
- Branch and worktree creation must be deterministic and visible to the user.
- Never discard changes, delete branches or worktrees, force-push, reset, clean,
  or rewrite history without explicit instruction and confirmation of the exact
  affected targets.

## Repository and dependencies

- Use a Cargo workspace modeled structurally on Zed. Keep binaries thin and put
  substantial domain-owned functionality in focused crates under `crates/`.
  Add only the structure required by the current feature.
- Co-locate focused tests with their responsible crate; use integration tests
  for cross-crate behavior.
- Follow Zed's established organization for assets, actions, settings, themes,
  keymaps, platform code, workspace dependencies, lints, and build tooling when
  those systems are introduced.
- Before adding a dependency, inspect how Zed solves the same problem. Prefer
  compatible dependencies already used by Zed, configure shared versions at
  workspace level, and avoid duplicate libraries for one responsibility.
- Check dependency maintenance, platform support, security, and license
  compatibility. Preserve required attribution and notices when adapting code.

## Documentation is part of the change

Write for a future human or agent who does not have the original conversation.
Update documentation in the same change as behavior; stale or missing
documentation means the work is incomplete.

- Keep setup and project orientation in `README.md`.
- Keep durable architecture, boundaries, data flow, concurrency, lifecycle,
  persistence, terminal/process ownership, and worktree isolation under
  `docs/`.
- Record significant product and architecture decisions under
  `docs/decisions/`, including context, constraints, considered options,
  decision, consequences, status, and relevant official-doc and Zed references.
- Document non-obvious module responsibilities, public contracts, invariants,
  error behavior, safety requirements, and cancellation semantics close to the
  code.
- Use diagrams or structured flows only when relationships would otherwise be
  difficult to understand.
- Keep terminology consistent. Remove or update obsolete guidance and record
  unresolved questions, limitations, and migration concerns explicitly.

## Required workflow for implementation tasks

### 1. Understand

- Restate the request as concrete acceptance criteria.
- Inspect the existing implementation and identify consequential ambiguity.

### 2. Research

- Read current official documentation for the exact APIs involved.
- Find and trace the closest local Zed implementation.
- Record the Zed commit, paths, symbols, tests, and relevant license
  obligations. Record any separately consulted upstream commit.

### 3. Plan

Briefly identify:

- the Zed reference;
- what can be ported or reused conceptually;
- what must differ for Void and why;
- the files or crates that will change;
- how the behavior will be tested.

Ask before any material deviation or unspecified product decision.

### 4. Implement

- Make small, coherent changes using established Void or Zed-derived patterns.
- Handle errors, cancellation, cleanup, focus, subscriptions, and task ownership
  explicitly.
- Update tests and durable documentation with the implementation.

### 5. Verify

At minimum, run the repository's equivalents of:

```text
cargo fmt --check
cargo check --workspace
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
```

Add focused tests for changed behavior and manually exercise user-facing flows
when automation is insufficient. If a check cannot run or fails for an
unrelated reason, report the exact command and failure.

## Completion report

Report concisely:

1. what changed;
2. the local Zed commit, source paths, and symbols used;
3. intentional differences from Zed and why;
4. documentation and decision records changed;
5. tests, checks, and manual verification performed and their results;
6. unresolved risks, assumptions, limitations, and decisions requiring the
   maintainer.

Do not claim code was copied from or is equivalent to Zed unless that was
actually verified. Do not claim completion while requested behavior, relevant
states, documentation, or verification remains unfinished.

## Instruction priority

When guidance conflicts, use this order:

1. the maintainer's explicit request for the current task;
2. this file;
3. established Void patterns and recorded decisions;
4. official documentation matching the dependency version in use;
5. the corresponding local Zed implementation;
6. primary-source-verified idiomatic Rust and GPUI practice.

Security, data integrity, licensing obligations, and explicit confirmation for
destructive operations always remain mandatory.

## Craft

Care is an engineering requirement. Seek the simplest truthful model of the
problem. Write code whose names, boundaries, tests, documentation, and failure
paths reveal the reasoning behind it. Admit uncertainty; ask when evidence runs
out; stop when the requested behavior is complete. Reject guessed, careless,
bloated, misleading, or merely plausible work.

Every change will be inherited by someone who was not present when it was made.
Leave them clear evidence, fewer hidden assumptions, and a repository that is
easier to understand and safely change.

> Seek the truth. Build it simply and with care. When you do not know, stop and
> ask. Leave beautiful work for whoever comes next.
