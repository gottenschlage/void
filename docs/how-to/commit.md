# Commit the current work

Treat the Git history as durable project memory. Create the commits; do not only suggest commit messages.

## Process

1. Inspect the working tree with `git status`, staged and unstaged diffs, surrounding code, and recent commit messages.
2. Reconstruct what was changed, why it was needed, and how the implementation works.
3. Divide the work into coherent units before committing.

   - Split changes that solve different problems, belong to separate features, or can be understood and reverted independently.
   - Keep tightly coupled changes together.
   - Do not place the entire working tree into one commit merely because the changes were made in the same session.

4. Preserve unrelated or pre-existing user changes. Never discard, overwrite, or include them accidentally.
5. Stage only the files or hunks belonging to the current unit of work. Use patch staging when different units overlap the same file.
6. Run focused validation when practical, then commit each unit in a logical dependency order.

## Commit message

Use this structure:

```text
<Title>

<Body>

Changes:
- ...

Notes:
- ...
```

`Notes` is optional.

### Guidelines

- Use a short, descriptive, sentence-style title.
- Do not use Conventional Commit prefixes such as `feat:`, `fix:`, or `chore:`.
- Treat every commit as a checkpoint memory unit.
- Explain the problem or intent, why the change was needed, and the important implementation decisions.
- Record constraints, trade-offs, compatibility requirements, or non-obvious behavior when they will matter later.
- Summarize meaningful behavior in `Changes`; do not list files or mechanically restate the diff.
- Use `Notes` only for durable context such as limitations, migrations, assumptions, or follow-up work.
- Keep small commits concise. Add detail only when it preserves useful project knowledge.
- Never invent functionality or reasoning that cannot be verified from the code or session context.
- Do not include commit hashes, authors, timestamps, diff statistics, or generated summaries.

## Safety

- Do not commit secrets, credentials, temporary files, build output, or unrelated lockfile changes.
- Do not amend, squash, rebase, or rewrite existing commits unless explicitly requested.
- If there are no valid changes to commit, report that clearly.

After completion, return a short list of the commits created and any changes intentionally left uncommitted.
