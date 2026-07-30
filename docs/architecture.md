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
8. With a workspace, `VoidRoot` renders the repository sidebar, branch header, and branch-content placeholder.
9. Void activates the application after window creation.
10. If database or initial-window creation fails, Void reports the error and stops rather than panicking.

No process ownership, terminals, or agents exist yet.

## First-launch UI

The absence of a workspace record is the onboarding state; no separate preference or completion flag exists. The first screen asks only for a non-empty workspace name. Submission is available from Enter and the create button, and the database write runs outside rendering. Once the write completes, the same root entity transitions to the main-screen placeholder and displays the persisted name.

The small single-line input in `crates/void/src/text_input.rs` follows GPUI's official input example and implements `EntityInputHandler`, UTF-8/UTF-16 conversion, selection, clipboard actions, platform composition, focus, and accessibility semantics. It is local to the binary until repeated form needs justify a shared UI component.

## Main shell and repository onboarding

The main shell is a compact fixed-width repository sidebar beside a branch-selection placeholder. Its layout, JetBrains Mono typography, neutral dark palette, spacing, and interaction hierarchy adapt Sunware's desktop sidebar exactly; its GPUI composition and lifecycle follow Zed. The window uses Sunware's 15 px root scale through `Window::set_rem_size`, so GPUI's rem-based text and spacing utilities resolve to the same density as the desktop CSS. Fixed shell measurements also follow the desktop source: a 240 px sidebar, 37.5 px branch strip, and 165 px branch tabs. Theme tokens remain centralized in `crates/void/src/theme.rs`. The workspace row opens a deferred anchored menu that lists active repositories, supports persistent pinning and non-destructive archiving, shows archived repositories in a separate muted section with a restore action, and ends with **Add repository**. Pinned repositories sort first. Repository rows in the menu and branch rows in the sidebar use typed GPUI drag payloads and detached previews; drops update the UI immediately and persist positions asynchronously within their owning workspace or repository. Repository writes run outside rendering and temporarily disable the affected menu row. Repository rows expand and collapse, switch between closed and open folder icons, and reveal an add-branch button on hover. Active managed branches appear beneath their expanded repository with their stable integration number; an expanded repository without active branch records displays **No branches yet**.

Adding a repository uses GPUI's native directory prompt. Validation and the system Git process run on the background executor. Void canonicalizes the selected path, requires it to be the root of a non-bare Git worktree, derives the repository name from the directory, rejects duplicate paths, then persists and displays the repository without restarting. The system Git executable remains authoritative for deciding whether the directory is a repository. Picker, validation, and database errors stay in the sidebar as plain inline feedback.

The add-branch button opens a centered modal. Void lists local Git branches, puts the checked-out branch first, generates an editable lowercase adjective-animal branch name with `petname` 3.1, and allows another name to be generated. The generator requests two hyphen-separated words from the crate's built-in English lists and enables only its default RNG and word-list features, excluding its CLI. SQLite allocation remains responsible for resolving collisions with numeric suffixes. On confirmation, Void validates the requested name and base ref, reserves the immutable integration number, allocated name, and managed path in SQLite, then runs `git worktree add -b` on the background executor. A Git failure archives the reservation without reusing its integration number. A successful branch appears immediately under its expanded repository.

The horizontal branch tab strip starts empty. Selecting a branch from the
sidebar opens and activates it; selecting an already open tab keeps the sidebar
synchronized. Each tab reveals a close button on hover. Closing is
window-session state only: it neither archives the branch nor changes its
worktree, and closing the active tab activates its right neighbor before falling
back to its left neighbor. Tabs use GPUI's typed drag-and-drop flow and may be
reordered by dragging over another tab; the target edge shows the insertion
direction and GPUI renders a detached preview while dragging. The strip scrolls
horizontally when it exceeds the available width.

Header order is intentionally window-session state. The current persisted
`branches.position` key is scoped to one repository, while the header can contain
branches from several repositories. Persisting a single interleaved header order
would therefore require a separate workspace-level model and is deferred until
that product behavior is specified.

## Branch terminal panels

`void_terminal` is the process and presentation boundary for terminals. The
application root owns a `BranchTerminalPanel` entity for each open branch tab.
Panels are created on first activation rather than at database load, remain
mounted while another branch is selected, and are released when their branch
tab is closed or archived. Reopening a closed branch therefore creates a fresh
panel and shell.

Each panel owns an ordered set of `TerminalSession` entities. A session starts
the system shell in the branch's managed worktree with Zed's
`terminal::TerminalBuilder`; its builder and PTY event loop run on GPUI's
background executor. The session subscribes to terminal events, synchronizes
pending output before painting, and sends grid-size changes through
`Terminal::set_size`. Releasing the session releases the terminal entity, so
Zed's terminal teardown owns graceful process-tree termination and escalation.
No detached subprocess wrapper exists alongside that lifecycle.

Branch selection is handled by window-aware GPUI subscriptions. Selection
creates the branch panel and its first session before notifying the UI, then
focuses the active terminal. Rendering only reads the already-established
active panel; it does not create entities, spawn shells, or change focus.

Terminal tabs are panel-local window state. New sessions are inserted first and
focused, labels follow the foreground process/title reported by Zed, and typed
GPUI drag payloads reorder tabs. Closing the selected tab chooses its right
neighbor before its left; closing the final tab immediately creates and focuses
a replacement. The 30 px strip, 105 px tabs, 12 px horizontal terminal inset,
and 9 px vertical inset match Sunware's desktop terminal panel. Loading and
spawn failures render inline in the terminal body.

`TerminalSettings` is passed into each panel and is the caller-facing source of
truth for terminal construction and painting. It defines the JetBrains Mono
13 px font and line height, underline blinking cursor, transparent background,
desktop ANSI/default/selection/cursor colors, alternate-scroll and Option-as-
Meta behavior, and bounded scrollback. The terminal surface has no scrollbar.
Settings persistence and UI remain deferred.

Void initializes Zed's base theme model during application startup even though
its visible workspace palette remains locally defined. The pinned terminal
backend consults that global model when a program requests terminal colors, so
the model must exist before any terminal session starts.

The terminal paint element derives its cell width from GPUI text shaping and
uses the same snapped bounds for PTY resizing, painting, cursor placement,
selection, mouse input, URL activation, and IME positioning. Its prepaint phase
synchronizes the terminal once and builds batched shaped text plus background,
selection, decoration, and cursor geometry; paint consumes that immutable
state. Standard, bright, 256-indexed, and true-color ANSI cells, styled blank
cells, wide and zero-width characters, cursor shapes and blinking, scrollback,
keyboard/IME input, platform copy/paste actions, mouse input, URL links, and
platform-appropriate dropped paths are supported. Path-like links remain inactive
until Void has an editor/files target. Workspace docks, split panes, diff views,
file panels, scrollbars, and persisted terminal state are intentionally absent.
Zed's `terminal` crate is GPL-3.0-or-later and is compatible with Void's
GPL-3.0-only combined distribution; the upstream package metadata and source
notices remain intact.

## Workspace persistence

The `workspace` crate models the approved hierarchy:

```text
workspace 1 ─── * repositories 1 ─── * branches
```

Each branch row is a Void-managed Git branch and its dedicated worktree. Repository and branch positions are mutable sort keys; pinned records sort first. Repository `sequence` allocates immutable, repository-scoped branch numbers. Archived names, paths, and numbers remain reserved.

The sidebar reveals its branch archive action only while the row is hovered. Archiving updates SQLite asynchronously, hides the branch from the sidebar and branch header, and clears the active selection when that branch was selected; it does not delete the Git branch or worktree.

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

Product behavior and styling were verified against:

- `/Users/usama/Documents/archive/others/sunware/apps/desktop/src/renderer/components/sidebar/project-manager.tsx`
- `/Users/usama/Documents/archive/others/sunware/apps/desktop/src/renderer/components/sidebar/project-list.tsx`
- `/Users/usama/Documents/archive/others/sunware/apps/desktop/src/renderer/components/ui/sidebar.tsx`
- `/Users/usama/Documents/archive/others/sunware/apps/desktop/src/renderer/index.css`

Verified against local Zed commit `5e549b871fb87d1038d9b1b242bf7d4d4e3b4d8f`:

- `crates/gpui/README.md` — standalone application setup and platform features.
- `crates/gpui/examples/hello_world.rs` — window creation and root rendering.
- `crates/gpui/examples/input.rs` — focus, key actions, platform text input, selection, and painting.
- `crates/gpui/examples/popover.rs` — anchored deferred overlays and outside-click dismissal.
- `crates/workspace/src/pane.rs::{DraggedTab, Pane::render_tab, Pane::handle_tab_drop}` — typed tab drag payloads, drag previews, directional drop feedback, and tab movement.
- `crates/terminal/src/terminal.rs::{TerminalBuilder::new, TerminalBuilder::subscribe, Terminal::set_size, Terminal::sync, Terminal::try_keystroke}` — PTY construction, background events, resize, rendering synchronization, input, and teardown.
- `crates/terminal_view/src/terminal_element.rs::TerminalElement` — grid coordinates, ANSI color resolution, cursor, and selection painting.
- `crates/terminal_view/src/terminal_view.rs::TerminalView` — focus, keyboard, clipboard, scrolling, and dropped-path interaction.
- `crates/terminal_view/src/terminal_panel.rs::TerminalPanel` — panel-local terminal entity ownership and activation.
- `crates/gpui/src/app.rs::App::prompt_for_paths` — native asynchronous path selection.
- `crates/gpui_platform/src/gpui_platform.rs::application` — native platform construction.
- `crates/gpui/src/app.rs::App::open_window` — window and root-view creation.
- root `Cargo.toml` — workspace organization, dependency centralization, profiles, and lints.
- root `rust-toolchain.toml` and `rustfmt.toml` — toolchain and formatting policy.
- `crates/db/src/db.rs` — SQLite lifecycle, pragmas, background writes, and GPUI-global database ownership.
- `crates/workspace/src/persistence.rs::WorkspaceDb` — domain migrations and typed persistence.
- `crates/git/src/repository.rs::{Branch, Worktree, CreateWorktreeTarget}` — Git branch/worktree semantics.
- `crates/project/src/git_store.rs` — repository identity and linked-worktree state.
