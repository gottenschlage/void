# Architecture

## Status

Void has a GPUI application shell, workspace onboarding, a repository sidebar, and its first persistence domain. This document describes only code that exists now and the boundaries established for subsequent work.

## Workspace structure

Void follows Zed's Cargo-workspace organization without pre-creating speculative crates:

- The root `Cargo.toml` owns workspace membership, dependency revisions, package defaults, and shared lints.
- `crates/void` is the native binary and current composition root.
- `crates/workspace` owns application-data paths and SQLite persistence for workspaces, repositories, and Void-managed branches/worktrees.
- Future domain or UI crates should be introduced only when a requested feature has a clear responsibility and a corresponding Zed architecture to reference.

The binary entry point in `crates/void/src/main.rs` stays thin. Startup and the initial workspace flow live in `application.rs`; larger product surfaces should move behind focused boundaries as they emerge.

## Application lifecycle

The current native startup flow is:

1. `main` calls `application::run`.
2. Void resolves the platform application-data directory and opens `<Void application data>/void.db`.
3. `gpui_platform::application()` selects GPUI's platform, renderer, and text backends for the host operating system.
4. Void loads the first persisted workspace and its repository/branch records, then `Application::run` registers `WorkspaceDb` as a GPUI global.
5. Void computes centered initial window bounds and calls `App::open_window`.
6. The window callback creates `VoidRoot` as a GPUI-owned entity through `Context::new`.
7. With no workspace, `VoidRoot` renders a focused name input and persists the submitted workspace asynchronously.
8. With a workspace, `VoidRoot` renders the repository sidebar and branch-selection placeholder.
9. Void activates the application after window creation.
10. If database or initial-window creation fails, Void reports the error and stops rather than panicking.

No process ownership, terminals, or agents exist yet.

## First-launch UI

The absence of a workspace record is the onboarding state; no separate preference or completion flag exists. The first screen asks only for a non-empty workspace name. Submission is available from Enter and the create button, and the database write runs outside rendering. Once the write completes, the same root entity transitions to the main-screen placeholder and displays the persisted name.

The small single-line input in `crates/void/src/text_input.rs` follows GPUI's official input example and implements `EntityInputHandler`, UTF-8/UTF-16 conversion, selection, clipboard actions, platform composition, focus, and accessibility semantics. It is local to the binary until repeated form needs justify a shared UI component.

## Main shell and repository onboarding

The main shell is a compact fixed-width repository sidebar beside a branch-selection placeholder. Its palette, spacing, borders, compact rows, menus, and control states follow Zed's bundled One Dark UI tokens, centralized in `crates/void/src/theme.rs`. The workspace row opens a small deferred popover containing only **Add repository**. Repository rows expand and collapse, switch between closed and open folder icons, and reveal an add-branch button on hover. Active managed branches appear in an indented tree beneath their expanded repository; an expanded repository without active branch records displays **No branches yet**.

Adding a repository uses GPUI's native directory prompt. Validation and the system Git process run on the background executor. Void canonicalizes the selected path, requires it to be the root of a non-bare Git worktree, derives the repository name from the directory, rejects duplicate paths, then persists and displays the repository without restarting. The system Git executable remains authoritative for deciding whether the directory is a repository. Picker, validation, and database errors stay in the sidebar as plain inline feedback.

The add-branch button opens a centered modal. Void lists local Git branches, puts the checked-out branch first, generates an editable branch name, and allows another name to be generated. On confirmation, Void validates the requested name and base ref, reserves the immutable integration number, allocated name, and managed path in SQLite, then runs `git worktree add -b` on the background executor. A Git failure archives the reservation without reusing its integration number. A successful branch appears immediately under its expanded repository.

## Workspace persistence

The `workspace` crate models the approved hierarchy:

```text
workspace 1 ─── * repositories 1 ─── * branches
```

Each branch row is a Void-managed Git branch and its dedicated worktree. Repository and branch positions are mutable sort keys; pinned records sort first. Repository `sequence` allocates immutable, repository-scoped branch numbers. Archived names, paths, and numbers remain reserved.

`VoidPaths` resolves `void.db` and `worktrees/` beneath the platform application-data directory. Branch separators are flattened for one-level worktree directories: `feature/auth` becomes `feature-auth`. Allocation checks both the Git name and flattened path and applies `-2`, `-3`, and later suffixes when needed.

`WorkspaceDb` uses SQLite through Zed's pinned `sqlez` crates. Reads use per-thread connections and writes are serialized by sqlez's background worker. SQLite migrations are append-only once released. Git remains authoritative: persistence reserves Void-owned identity and intent, while the system Git executable validates refs and creates the actual branch/worktree.

See [`decisions/0002-persist-workspace-repository-branches.md`](decisions/0002-persist-workspace-repository-branches.md) for the schema and allocation invariants.

## GPUI dependency boundary

`gpui` provides entities, contexts, rendering, elements, geometry, and application-facing types. `gpui_platform` constructs the correct platform implementation. Both dependencies point to the same pinned Zed Git revision so their internal APIs cannot drift independently.

The platform feature set mirrors Zed's desktop application dependency:

- `font-kit` enables macOS glyph rasterization;
- `wayland` and `x11` enable both supported Linux/FreeBSD windowing backends;
- Windows platform selection is handled by `gpui_platform` without an additional feature.

See [`decisions/0001-pin-gpui-to-zed-revision.md`](decisions/0001-pin-gpui-to-zed-revision.md) for why Void currently uses Git dependencies instead of the published `gpui` crate alone.

## Current invariants

- GPUI, `gpui_platform`, `sqlez`, and `sqlez_macros` must resolve from one Zed revision.
- `void.db` and managed worktrees live beneath the same platform application-data directory.
- Branch integration numbers, allocated names, and worktree paths are never reused after archival.
- Deleting database records never implies deleting Git branches or filesystem worktrees.
- GPUI application and UI state are created and accessed through GPUI contexts.
- Expensive I/O and process work must never be introduced into the render path or block GPUI's foreground executor.
- The binary remains a composition root; substantial product capabilities belong in focused crates once their boundaries are understood.
- Architecture documentation and decision records must change together with the implementation.

## Reference implementation

Verified against local Zed commit `5e549b871fb87d1038d9b1b242bf7d4d4e3b4d8f`:

- `crates/gpui/README.md` — standalone application setup and platform features.
- `crates/gpui/examples/hello_world.rs` — window creation and root rendering.
- `crates/gpui/examples/input.rs` — focus, key actions, platform text input, selection, and painting.
- `crates/gpui/examples/popover.rs` — anchored deferred overlays and outside-click dismissal.
- `crates/gpui/src/app.rs::App::prompt_for_paths` — native asynchronous path selection.
- `crates/gpui_platform/src/gpui_platform.rs::application` — native platform construction.
- `crates/gpui/src/app.rs::App::open_window` — window and root-view creation.
- root `Cargo.toml` — workspace organization, dependency centralization, profiles, and lints.
- root `rust-toolchain.toml` and `rustfmt.toml` — toolchain and formatting policy.
- `crates/db/src/db.rs` — SQLite lifecycle, pragmas, background writes, and GPUI-global database ownership.
- `crates/workspace/src/persistence.rs::WorkspaceDb` — domain migrations and typed persistence.
- `crates/git/src/repository.rs::{Branch, Worktree, CreateWorktreeTarget}` — Git branch/worktree semantics.
- `crates/project/src/git_store.rs` — repository identity and linked-worktree state.
