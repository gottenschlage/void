# Create or update the pull request

Treat the pull request as the durable record of one complete unit of work. Create or update the PR; do not only draft its text.

## Understand the work

Before writing the PR:

1. Identify the current branch, its base branch, and whether a pull request already exists.
2. Read the relevant codebase, the full branch diff, surrounding architecture, tests, documentation, and configuration.
3. Read every commit message and body on the branch, plus relevant recent history when needed.
4. Reconstruct:

   - the original problem and why the work was needed,
   - the intended user or system behavior,
   - the implementation approach,
   - important architectural decisions and constraints,
   - how the commits combine into one completed unit of work,
   - any lasting limitations, migrations, or trade-offs.

5. Verify the narrative against the current code. Do not rely on commit titles alone and never invent behavior.

## Remove transient working memory

Before creating or updating the pull request:

1. Read `working-memory.md` and transfer every lasting decision, constraint,
   limitation, reference, and verification boundary into the appropriate code,
   tests, documentation, ADR, or commit message.
2. Confirm completed and deferred work can be reconstructed without the
   working-memory file.
3. Remove `working-memory.md`, commit the removal with the final PR preparation
   changes, and verify it is absent from the branch diff.

Working memory is branch-local execution scaffolding, not a durable project
artifact. Do not remove it until implementation and validation are complete,
and do not replace durable documentation with a copy of the working-memory
file in the PR body. Follow
[`use-working-memory.md`](use-working-memory.md) throughout multi-phase work.

## Behavior

- If no PR exists, create one.
- If a PR already exists, update its title and body to match the current implementation.
- Preserve useful existing context, but remove stale, duplicated, or superseded information.
- Describe the branch as one integrated unit of work rather than listing or narrating each commit separately.

## Title

Write a short, descriptive, sentence-style title.

Do not use prefixes such as `feat:`, `fix:`, or `chore:`.

## Body

Use this structure:

```markdown
## TL;DR

A concise summary of what was implemented and why it matters.

## Context

The problem, need, or limitation that led to this work. Include the intended behavior and important constraints.

## Implementation

Explain the chosen approach, how the main parts work together, and the significant decisions or trade-offs.

## Changes

- Meaningful implemented change
- Meaningful implemented change

## Notes

Durable context such as limitations, migrations, compatibility concerns, assumptions, or follow-up work.
```

Omit `Notes` when there is nothing useful to preserve.

### Guidelines

- Write for reviewers and future coding agents.
- Explain what changed, why it changed, how it is intended to be used, and why this implementation was chosen.
- Keep it concise, but retain information that would otherwise be lost from the code.
- Group related changes by behavior or subsystem.
- Do not include file lists, commit lists, diff statistics, boilerplate checklists, or a testing section unless explicitly requested or materially important.
- Do not include claims that cannot be verified from the code, commits, or existing PR context.

Use the GitHub CLI to create or update the PR, then open it with:

```bash
gh pr view --web
```

After completion, report whether the PR was created or updated and provide its URL. If a repository, remote, authentication, or branch requirement blocks the operation, report the exact blocker without fabricating success.
