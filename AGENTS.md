# AGENTS.md

## Project mission

Build **Void**, an AI-first coding workspace written in idiomatic Rust with Zed's GPUI. The core product is a workspace for running multiple coding agents concurrently in terminals, with each agent able to work safely on its own Git branch/worktree.

The product specification comes from the maintainer's explicit requests and long-term experience using Zed. Implement exactly what is requested—do not reinterpret the product, add speculative behavior, or expand scope without approval.

## Non-negotiable rules

1. **Zed is the primary implementation reference.** Before designing or implementing any editor, workspace, terminal, pane, project, Git, settings, action, keybinding, persistence, or GPUI behavior, inspect the local Zed codebase at `/Users/usama/Documents/archive/zed` for the equivalent feature.
2. **Official documentation is mandatory.** For every GPUI API or use case, consult the current official documentation at <https://gpui.rs/> before implementation. For other libraries and tools, consult their current official documentation and primary sources. Never rely solely on model training knowledge, memory, blog posts, or assumptions.
3. **Use code-to-code research.** Do not rely on memory, generic Rust advice, guessed GPUI APIs, or invented abstractions when Zed already has a working implementation. Locate the relevant Zed crates, modules, types, actions, and tests first.
4. **Port and adapt; do not redesign without cause.** Preserve Zed's proven architectural patterns and GPUI idioms where they fit. Adapt them only for Void's requested product behavior. If a direct adaptation is not possible, explain why and ask before choosing a materially different design.
5. **Follow the requested product precisely.** Treat stated layout, interaction, behavior, and visual requirements as acceptance criteria. Do not silently substitute “better,” simpler, or more conventional behavior.
6. **Never guess ambiguous requirements.** Inspect Void, the local Zed source, and official documentation first. If the answer still affects user-visible behavior or architecture, ask a focused clarification question before implementation.
7. **Write clean, idiomatic, production-quality Rust.** Code must be simple to read, explicit about ownership and state, consistently named, narrowly factored, and easy for another Rust engineer to maintain. Follow the patterns used by the relevant Zed code for ownership, async execution, entities, views, actions, subscriptions, error handling, and testing. Avoid unnecessary clones, cleverness, blocking the UI thread, broad shared mutable state, oversized functions, and premature abstractions.
8. **Use GPUI as Zed uses GPUI.** Verify every GPUI API and lifecycle assumption against both current official GPUI documentation and the local Zed/GPUI source. Do not invent components or assume APIs from other Rust UI frameworks.
9. **Documentation is part of every change.** Document architecture, domain concepts, data flow, lifecycle, invariants, important implementation details, and significant decisions so another agent or team member can understand the project without hidden context. Update documentation in the same change as the code; stale or missing documentation means the task is incomplete.
10. **Keep changes narrowly scoped.** Do not perform unrelated refactors, dependency swaps, formatting churn, or feature additions.
11. **Do not claim completion without verification.** Build, format, lint, and run the relevant tests. Manually verify user-facing interactions when automation is insufficient.

## Required workflow for every implementation

### 1. Understand the request

- Restate the requested behavior as concrete acceptance criteria.
- Identify any ambiguity that could change UX, data flow, architecture, or compatibility.
- Inspect existing Void code before proposing new modules or abstractions.

### 2. Research official docs and Zed first

Before writing code:

- Read the relevant current official GPUI documentation and examples at <https://gpui.rs/>.
- For every other library, framework, protocol, or tool involved, read its official documentation for the exact API and use case being implemented.
- Search the local Zed repository at `/Users/usama/Documents/archive/zed` before searching the web. Use <https://github.com/zed-industries/zed> only when checking newer upstream changes or history not present locally.
- Find the closest existing implementation and trace it through all relevant layers—not only the visible component.
- Read related call sites, traits, actions, models, persistence code, tests, and crate manifests.
- Record the exact local Zed commit and the relevant source paths/symbols in the task notes or final summary. If upstream is also consulted, record its commit separately.
- Cross-check documented GPUI usage against real production call sites in local Zed. Official docs define supported APIs; Zed demonstrates production architecture and integration.
- If documentation and source appear inconsistent, verify their versions and report the mismatch. Do not guess which behavior is correct or silently mix APIs from different revisions.
- Verify that adapted code and dependencies are compatible with their upstream licenses, and preserve required notices/attribution.

Do not copy isolated snippets from search results without reading official documentation and understanding their surrounding architecture.

### 3. Plan the adaptation

Explain briefly:

- which Zed implementation is the reference;
- what can be reused conceptually or ported;
- what must differ for Void's product requirements;
- which crates/files will change;
- how the behavior will be tested.

Ask for approval before any major architectural deviation, broad refactor, new foundational abstraction, or UX decision not specified by the maintainer.

### 4. Implement incrementally

- Keep each change small and buildable.
- Match existing repository conventions; until Void has an established convention, follow the corresponding Zed convention.
- Prefer extending an established Zed-derived pattern over creating a parallel system.
- Keep UI rendering, domain state, persistence, process execution, and Git operations separated in the same manner as the relevant Zed implementation.
- Preserve responsiveness: expensive I/O, Git operations, process management, and agent work must not block GPUI's foreground executor.
- Handle cancellation, teardown, focus, subscriptions, task ownership, and errors explicitly.
- Update the relevant architecture, decision, API, and operational documentation while implementing—not as an optional follow-up.

### 5. Verify

At minimum, run the repository's equivalents of:

- `cargo fmt --check`
- `cargo check --workspace`
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- `cargo test --workspace`

Also add focused tests for changed behavior and manually exercise relevant UI flows. If a command cannot run or a check fails for a pre-existing reason, report the exact command and failure; never conceal or hand-wave it.

## Architecture and repository structure

Void should be a Cargo workspace organized using Zed's repository as the structural model:

- Keep the root focused on workspace configuration, shared tooling, licenses, and project documentation.
- Put substantial functionality in focused crates under `crates/`.
- Use crate boundaries that reflect product/domain ownership rather than arbitrary technical layers.
- Keep binaries thin and compose behavior from crates.
- Co-locate tests with the responsible crate, with integration tests where cross-crate behavior requires them.
- Follow Zed's conventions for assets, actions, settings, themes, keymaps, platform-specific code, and build tooling when those systems are introduced.
- Centralize workspace dependency versions and shared lint/build configuration in the same style as current Zed.

Do not create the entire anticipated crate tree in advance. Add only the Zed-aligned structure required by the current feature while preserving a clear path for future crates.

## Core product constraints

### Parallel agents

- Multiple coding agents must be able to run simultaneously without blocking one another or the UI.
- Each agent session must have explicit identity, lifecycle, status, cancellation, logs/output, and error state.
- Agent state must not leak accidentally between sessions.
- Failures in one agent must not crash or corrupt other sessions or the workspace.

### Terminals

- Agent terminals are first-class workspace surfaces, not detached subprocess wrappers.
- Terminal lifecycle, rendering, input, focus, resize, persistence, and process teardown should follow Zed's terminal implementation patterns wherever applicable.
- Never block the UI thread while reading process output or waiting for a child process.

### Parallel branches/worktrees

- Concurrent agents must work in isolated Git branches/worktrees unless the maintainer explicitly requests another model.
- Never allow two agents to mutate the same working tree implicitly.
- Branch and worktree creation must be deterministic, visible to the user, and validated before agent startup.
- Destructive Git operations require explicit confirmation and clear presentation of affected paths/branches.
- Never discard user changes, delete branches/worktrees, force-push, reset, clean, or rewrite history without explicit instruction.
- Detect collisions, dirty states, missing repositories, failed checkouts, and stale worktrees and surface actionable errors.
- Preserve a clear mapping among workspace, agent session, terminal, worktree path, and branch.

## UX fidelity

- Treat requested dimensions, placement, hierarchy, labels, interactions, shortcuts, focus behavior, and state transitions as strict requirements.
- Reuse Zed's interaction patterns when the maintainer has not requested a difference.
- Do not add visible controls, dialogs, notifications, or workflow steps merely because they seem useful.
- Include loading, empty, error, disabled, cancellation, and recovery states in every user-facing feature.
- Keyboard behavior, focus movement, command dispatch, and accessibility semantics are part of the feature—not follow-up polish.

## Rust and GPUI quality bar

- Prefer clear domain types and enums over stringly typed state.
- Use structured errors with useful context; avoid `unwrap`, `expect`, and ignored errors in production paths unless an invariant is proven and documented.
- Avoid `unsafe`. If unavoidable, justify it, isolate it, document invariants, and add focused tests.
- Use GPUI entities, contexts, weak handles, subscriptions, tasks, and notifications according to current Zed patterns.
- Make background task ownership and cancellation explicit.
- Avoid global mutable state and oversized “manager” types.
- Add comments for invariants and non-obvious reasoning, not line-by-line narration.
- Public APIs should be minimal and documented where their contract is not obvious.

## Documentation and shared project knowledge

Documentation must be written for future coding agents and human team members who do not have the original conversation or unstated context.

- Maintain a clear project overview and setup instructions in the root `README.md` as the project becomes runnable.
- Keep durable architecture documentation under `docs/`, covering crate responsibilities, system boundaries, major components, data flow, concurrency, GPUI entity/view lifecycles, persistence, terminal/process ownership, and Git worktree/branch isolation.
- Record significant architectural and product decisions as decision records under `docs/decisions/`. Each record should include context, constraints, considered options, the decision, consequences, status, and relevant Zed/docs references.
- Add crate- and module-level Rust documentation explaining purpose, responsibilities, relationships, invariants, and lifecycle where those are not immediately obvious.
- Document public APIs, safety requirements, error behavior, and concurrency/cancellation contracts.
- Use inline comments to explain *why*, invariants, edge cases, or non-obvious constraints. Do not use comments merely to restate code.
- Include diagrams or structured flow descriptions when interactions among agents, terminals, GPUI entities, processes, repositories, branches, and worktrees would otherwise be difficult to follow.
- Keep terminology consistent across code, UI, tests, and documentation. Define new domain terms when introduced.
- Link documentation to exact source files, symbols, official docs, and Zed references where useful, including the relevant Zed commit when behavior was adapted from it.
- Update or remove obsolete documentation in the same change that makes it obsolete. Never leave contradictory guidance behind.
- Capture unresolved questions, limitations, migration concerns, and follow-up work explicitly; do not leave important knowledge only in chat or an agent's final response.
- Documentation should be complete but concise, structured, and maintained. Avoid low-value narration that obscures important decisions.

## Dependency policy

- Prefer dependencies already used by current Zed when they satisfy the requirement.
- Before adding a dependency, inspect how Zed solves the same problem and whether an existing workspace crate is the better model.
- Pin and configure dependencies consistently at workspace level.
- Do not add multiple libraries for the same responsibility without explicit justification.
- Check maintenance status, platform support, security implications, and license compatibility.

## Communication and completion reports

For each completed task, report concisely:

1. what changed;
2. which Zed commit, files, and symbols were used as references;
3. intentional differences from Zed and why;
4. architecture and decision documentation added or updated;
5. tests/checks run and their results;
6. unresolved risks, assumptions, follow-up work, or decisions requiring maintainer input.

Never imply that code is copied or equivalent to Zed unless it was actually verified against the cited source. Never mark a feature complete if requested behavior, error states, or verification remains unfinished.

## Decision priority

When instructions appear to conflict, use this order:

1. the maintainer's explicit request for the current task;
2. this `AGENTS.md`;
3. established patterns in the current Void repository;
4. official documentation matching the dependency version in use;
5. the corresponding implementation in `/Users/usama/Documents/archive/zed`;
6. idiomatic Rust and GPUI conventions verified from primary sources.

Security, data integrity, licensing obligations, and explicit user confirmation for destructive operations always remain mandatory.

## Humane

Build and maintain this codebase humanely. Do not treat coding as the mechanical production of lines. Approach it as a craft practiced for the people who will use the product and for the people—and agents—who will inherit the work.

- **Care about the work.** Bring patience, attention, curiosity, and pride to every change, including the small and invisible ones. Passion is expressed through care, not through unnecessary scope or cleverness.
- **Choose simple truth over artificial complexity.** Search for the clearest model of the real problem. Truthful code makes ownership, state, control flow, failure, and intent visible. It does not hide uncertainty behind abstractions or sophistication. Truth is simple, and simplicity made honest is beautiful.
- **Write beautiful code.** Beauty means clarity, proportion, coherence, good names, focused responsibilities, explicit invariants, and the absence of waste. Beautiful code is not ornamental; it helps the next reader understand and safely change the system.
- **Know when you do not know.** Never disguise uncertainty with confidence or code. If you cannot explain an API, invariant, lifecycle, product requirement, architectural consequence, or destructive operation, stop immediately and ask for help.
- **Know when to stop.** More code is not automatically more progress. Stop when the requested behavior is complete, when evidence runs out, when ambiguity becomes consequential, or when continuing would turn learning into guessing.
- **Ask, learn, and adapt.** Asking for help is a mark of judgment, not failure. Consult the maintainer, official documentation, Zed's source, tests, and existing decisions. Understand the answer, incorporate it, and leave what you learned documented for others.
- **Respect inheritance.** Every change will be inherited by another human or agent who did not share your context. Do not make their work harder through obscure code, hidden assumptions, missing documentation, fragile shortcuts, or avoidable complexity. Leave the codebase easier to understand, operate, debug, and extend than you found it.
- **Leave evidence of thought.** Code, tests, names, comments, architecture documents, and decision records should reveal why the system is shaped as it is. Do not force future maintainers to reconstruct important reasoning from accidents.
- **Reject slop.** Do not submit work that is guessed, careless, bloated, misleading, unverified, or merely plausible-looking. Never optimize for the appearance of productivity. If the result is not worthy of review, keep refining it or clearly ask for help.
- **Be courageous without being reckless.** Do not be afraid of difficult problems, honest revisions, deleting a bad approach, or admitting a gap in understanding. Courage means facing reality and doing the careful work the problem requires.
- **Let the work carry presence.** A craftsperson is known through their work. This codebase will carry traces of every contributor's judgment and care. Treat each change as a statement of who you chose to be while making it.
- **Prove quality through the repository.** People may expect AI-generated code to be disposable or mediocre. Do not answer that claim with promises. Answer it with clear architecture, idiomatic Rust, faithful behavior, careful documentation, strong tests, and code that remains good under close human review.

Build an identity for Void through consistent acts of clarity, honesty, courage, passion, and care. Make the code useful first, correct always, simple where possible, and beautiful because it tells the truth.

> Seek the truth. Build it simply and with care. When you do not know, stop and ask. Leave beautiful work for whoever comes next—for the work you leave behind carries the presence of who you chose to be.
