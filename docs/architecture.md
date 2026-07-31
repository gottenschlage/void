# Architecture

## Status

Void has a GPUI application shell, workspace onboarding, a repository sidebar, and its first persistence domain. This document describes only code that exists now and the boundaries established for subsequent work.

## Workspace structure

Void follows Zed's Cargo-workspace organization without pre-creating speculative crates:

- The root `Cargo.toml` owns workspace membership, dependency revisions, package defaults, and shared lints.
- `crates/void` is the thin native binary and composition root. It owns startup, application assets, palette initialization, and updater composition.
- `crates/ui` holds reusable, domain-agnostic UI primitives — building blocks that depend only on `gpui`, `theme`, and Unicode segmentation, with no knowledge of Void's product types. See [ADR 0006](decisions/0006-extract-a-reusable-ui-crate.md).
- `crates/void_update` owns the authenticated stable-channel updater, including status/task lifecycle, feed validation, streaming download and hashing, macOS installation verification, cancellation cleanup, and status rendering.
- `crates/workspace` owns application-data paths, SQLite persistence, Git/worktree behavior, and the workspace product surface under `src/view/`. The view coordinator owns terminal panels and other GPUI resources keyed by branch identity.
- Future domain or UI crates should be introduced only when a requested feature has a clear responsibility and a corresponding Zed architecture to reference.

The binary entry point in `crates/void/src/main.rs` stays thin. `application.rs` loads the initial model, initializes dependencies, opens the window, and composes `WorkspaceView` with the updater. Workspace onboarding and interaction remain inside the owning library crate. See [ADR 0011](decisions/0011-own-workspace-ui-in-workspace-crate.md).

## Application lifecycle

The current native startup flow is:

1. `main` calls `application::run`.
2. Void resolves the platform application-data directory and opens `<Void application data>/void.db`.
3. `gpui_platform::application()` selects GPUI's platform, renderer, and text backends for the host operating system.
4. Void loads the first persisted workspace and its repository/branch records into a pure `WorkspaceModel`, then `Application::run` registers `WorkspaceDb` as a GPUI global.
5. Void computes centered initial window bounds and calls `App::open_window`.
6. The window callback creates `WorkspaceView` and the updater as GPUI-owned entities, then composes them in the binary's small `AppView`.
7. With no workspace, `WorkspaceView` renders a focused name input and persists the submitted workspace asynchronously.
8. With a workspace, `WorkspaceView` renders an integrated native title-bar row above the repository sidebar and branch content and owns GPUI resources keyed by stable branch IDs.
9. Void activates the application after window creation.
10. If database loading, required base-theme lookup, or initial-window creation fails, Void reports the error and stops rather than panicking.

`void_terminal` owns shell processes used by workspace branch panels. No coding-agent implementation exists yet.

## Theming

`::theme::init` (the pinned GPUI revision's `theme` crate) runs first and installs Zed's bundled "One Dark" theme as the active `theme::GlobalTheme`. `crate::theme::init` (`crates/void/src/theme.rs`) then replaces it with Void's palette: it fetches that same bundled theme from `theme::ThemeRegistry` and calls `ThemeColors::refined` with a `ThemeColorsRefinement` that overrides only the surface, border, element, and text tokens Void's UI renders, leaving syntax/status/vim/diff/terminal-ANSI colors as Zed's verified defaults.

Render code reads colors through `theme::ActiveTheme` (`cx.theme().colors().<field>`, `cx.theme().status().<field>`), never through hardcoded constants. `UI_FONT` and `UI_FONT_SIZE` are plain constants beside `WorkspaceView`, since fonts are workspace chrome but are not part of `theme::Theme` in the pinned revision — Zed itself sources fonts from the separate `theme_settings` crate, which Void does not depend on (see [ADR 0005](decisions/0005-adopt-zeds-theme-token-system.md) for why). Buffer/terminal font sizing is still owned independently by `void_terminal::TerminalSettings`.

`text_accent` (`rgb(0x3794ff)`) is the one shared accent color for both primary-button backgrounds and drag-and-drop indicators; scrollable containers that accept a drag (`workspace/src/view/sidebar`'s repository list, `view/branches/tabs.rs`, and `void_terminal`'s tab bar) also auto-scroll toward the cursor's edge mid-drag via `ui::auto_scroll_toward_edge`, so a drop target scrolled out of view can still be reached. See [ADR 0007](decisions/0007-drag-and-drop-accent-color-and-auto-scroll.md).

## First-launch UI

The absence of a workspace record is the onboarding state; no separate preference or completion flag exists. The first screen asks only for a non-empty workspace name. Submission is available from Enter and the create button, and the database write runs outside rendering. Once the write completes, the same root entity transitions to the main-screen placeholder and displays the persisted name.

The small single-line input under `crates/ui/src/text_input/` follows GPUI's pinned official input example without importing Zed's editor-backed `ui_input` stack. `mod.rs` owns the entity, focus, directional selection, extended-grapheme movement, clipboard and pointer interaction, UTF-8/UTF-16 conversion, IME state, and simple value-based accessibility contract. `element.rs` owns shaping, themed selection and caret painting, visible marked-text composition, native input registration, and the painted geometry used by platform queries. See [ADR 0014](decisions/0014-keep-a-small-native-text-input.md).

## Native window shell

On macOS, Void asks GPUI for a transparent native title bar and full-size
content while retaining AppKit's `NSWindow` frame, shadow, rounded corners,
resize behavior, and standard close/minimize/zoom buttons. GPUI positions those
buttons at `(16, 11)`. Void owns title-bar dragging:
`WindowControlArea::Drag` identifies the background, movement begins through
`Window::start_window_move` only after a left-button move, and double clicks
delegate to `Window::titlebar_double_click`. Mouse-down propagation stops at
the sidebar toggle, branch tabs, close buttons, and tab drag surfaces.

`WorkspaceView` owns session-only `sidebar_open` state, initially `true`. The fixed
37.5 px title row has a 240 px leading segment while open and a 48 px segment
while closed. Its remaining width belongs to the existing 165 px branch tabs
and draggable empty space. The body is a separate row. Collapsing animates only
the clipping widths over 200 ms with linear easing; GPUI's `AnimationExt`
automatically renders the end state when reduced motion is enabled. Sidebar,
branch-header, and terminal-panel entities remain owned by the root throughout,
so collapse cannot reset terminal processes, active selection, or tab order.

GPUI exposes traffic-light position but not visibility at the pinned revision.
The macOS-only adapter in `crates/workspace/src/view/macos_title_bar.rs` uses GPUI's public
`raw-window-handle` AppKit `NSView`, obtains its containing `NSWindow`, and sets
the three standard buttons' `hidden` property. The sole pointer cast is valid
only because GPUI owns the view for the window lifetime and invokes this code
synchronously on AppKit's main thread. Adapter failures are nonfatal and leave
the native controls visible. Open means visible, closed means hidden, and
fullscreen always means visible; window-bounds observation reapplies that
policy across fullscreen transitions.

Non-macOS window options retain GPUI defaults. See
[`decisions/0004-native-macos-integrated-title-bar.md`](decisions/0004-native-macos-integrated-title-bar.md).

## Main shell and repository onboarding

The main shell is a compact fixed-width repository sidebar beside a branch-selection placeholder. Its layout, JetBrains Mono typography, neutral dark palette, spacing, and interaction hierarchy adapt Sunware's desktop sidebar exactly; its GPUI composition and lifecycle follow Zed. The window uses Sunware's 15 px root scale through `Window::set_rem_size`, so GPUI's rem-based text and spacing utilities resolve to the same density as the desktop CSS. Fixed shell measurements also follow the desktop source: a 240 px sidebar, 37.5 px branch strip, and 165 px branch tabs. Theme tokens remain centralized in `crates/void/src/theme.rs`. The workspace row opens a deferred anchored menu that lists active repositories, supports persistent pinning and non-destructive archiving, shows archived repositories in a separate muted section with a restore action, and ends with **Add repository**. Pinned repositories sort first. Repository rows in the menu and branch rows in the sidebar use typed GPUI drag payloads and detached previews; drops update the UI immediately and persist positions asynchronously within their owning workspace or repository. Repository writes run outside rendering, are held by the sidebar so release cancels them, and temporarily disable the affected menu row. Repository rows expand and collapse, switch between closed and open folder icons, and reveal an add-branch button on hover. Active managed branches appear beneath their expanded repository with their stable integration number; an expanded repository without active branch records displays **No branches yet**.

Adding a repository uses GPUI's native directory prompt. Validation and the system Git process run on the background executor. Void canonicalizes the selected path, requires it to be the root of a non-bare Git worktree, derives the repository name from the directory, rejects duplicate paths, then persists and displays the repository without restarting. The system Git executable remains authoritative for deciding whether the directory is a repository. Picker, validation, and database errors stay in the sidebar as plain inline feedback.

The add-branch button opens a centered modal. Void lists local Git branches, puts the checked-out branch first, generates an editable lowercase adjective-animal branch name with `petname` 3.1, and allows another name to be generated. The generator requests two hyphen-separated words from the crate's built-in English lists and enables only its default RNG and word-list features, excluding its CLI. SQLite allocation remains responsible for resolving collisions with numeric suffixes. On confirmation, Void validates the requested name and base ref, reserves the immutable integration number, allocated name, and managed path in SQLite, then runs `git worktree add -b` on the background executor. A Git failure archives the reservation without reusing its integration number. A successful branch appears immediately under its expanded repository.

The horizontal branch tab strip starts empty. `WorkspaceModel` owns its
window-session open order and active branch identity. Selecting a branch from
the sidebar opens and activates it; selecting an already open tab keeps the
sidebar synchronized. Each tab reveals a close button on hover. Closing neither
archives the branch nor changes its worktree, and closing the active tab
activates its right neighbor before falling back to its left neighbor. Tabs use
GPUI's typed drag-and-drop flow and may be reordered by dragging over another
tab; a typed move event updates the model while the target edge shows the
insertion direction and GPUI renders a detached preview. The strip scrolls
horizontally when it exceeds the available width.

Header order remains intentionally unpersisted. The current persisted
`branches.position` key is scoped to one repository, while the header can
contain branches from several repositories. The model therefore keeps that
interleaved order only for the window session.

The branch header is synchronized from `WorkspaceModel` and emits typed
selection, close, and move intents. Sidebar repository and branch records are
read-only rendering snapshots refreshed from the model after typed persistence
transitions; only expansion, menu, loading, and error state remains local to the
view. Neither view owns terminal, context-header, or live-diff resources. See
[ADR 0009](decisions/0009-centralize-workspace-state.md).

## Branch terminal panels

`void_terminal` is the process and presentation boundary for terminals. Its private modules separate code-configurable defaults (`settings.rs`), pure tab identity/order (`tabs.rs`), one PTY-backed interactive entity (`session.rs`), branch-local process/tab ownership (`panel.rs`), and terminal grid painting plus platform text input (`terminal_element.rs`). The public API remains `init`, `TerminalSettings`, `TerminalId`, and `BranchTerminalPanel`.

`WorkspaceView` owns a `BranchTerminalPanel` entity for each open branch tab. Panels are created on first activation rather than at database load, remain mounted while another branch is selected, and are released when their branch tab is closed or archived. Reopening a closed branch therefore creates a fresh panel and shell.

Each panel owns an ordered set of `TerminalSession` entities. A session starts the system shell in the branch's managed worktree with Zed's `terminal::TerminalBuilder`; its builder and PTY event loop run on GPUI's background executor. The session retains its startup `Task`, so releasing a still-loading session cancels pending startup. Once ready, it owns the terminal entity and event subscription, synchronizes pending output before painting, and sends grid-size changes through `Terminal::set_size`. Releasing the session releases the terminal entity, so Zed's terminal teardown owns graceful process-tree termination and escalation. No detached subprocess wrapper exists alongside that lifecycle.

`WorkspaceView` owns one live-diff entity for each repository with an
active branch and one lightweight header entity for each open branch. The header
displays `#<number> <base-ref>/<branch-name>` above the terminal tabs. Its count
deliberately follows Zed's Git-panel meaning:
`git diff --numstat --no-renames HEAD --`, so it covers staged and unstaged
tracked changes but excludes committed changes, untracked files, and binary
line totals. Clean counts and errors before the first successful refresh remain
hidden. Each sidebar branch row displays the same count by default and replaces
it with the archive and delete actions while hovered.

Each repository live-diff entity owns one `notify` watcher covering the shared
Git directory and every registered managed worktree. Worktree and linked-index
events refresh only their branch; shared-ref events refresh all registered
branches. Events are coalesced, and each branch permits one asynchronous,
cancellable Git command plus one pending follow-up. Initial refresh does not
depend on watcher setup. Watch failures are logged and retried with bounded
backoff, while the last successful count remains available. Closing a terminal
tab keeps its worktree registered. Archiving or deleting a branch unregisters
it; removing the repository's final active branch drops the entity, watcher,
retries, and Git tasks together. Git state stays in the application/workspace
boundary rather than entering `void_terminal`.
See [`decisions/0008-live-head-to-worktree-diff.md`](decisions/0008-live-head-to-worktree-diff.md).

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
Zed's `terminal` crate is GPL-3.0-or-later and is compatible with Void's GPL-3.0-only combined distribution; the upstream package metadata and source notices remain intact. Void keeps this narrow adaptation instead of importing Zed's project, pane, search, persistence, or full terminal-view stack. See [`decisions/0012-keep-a-narrow-terminal-adaptation.md`](decisions/0012-keep-a-narrow-terminal-adaptation.md).

## Workspace persistence

The `workspace` crate models the approved hierarchy and owns the pure
`WorkspaceModel` used by the application coordinator:

```text
workspace 1 ─── * repositories 1 ─── * branches
```

Each branch row is a Void-managed Git branch and its dedicated worktree. Repository and branch positions are mutable sort keys; pinned records sort first. Repository `sequence` allocates immutable, repository-scoped branch numbers. Archived names, paths, and numbers remain reserved. The model adds window-local open order and active identity without putting GPUI entities or process handles into persistence state.

The sidebar reveals its branch archive action only while the row is hovered. Archiving updates SQLite asynchronously, hides the branch from the sidebar and branch header, and chooses the model's next active tab when necessary; it does not delete the Git branch or worktree. Archiving a repository closes all of its open tabs and releases their terminal panels, context headers, and live-diff registrations only after the persistence operation succeeds.

`VoidPaths` resolves `void.db` and `worktrees/` beneath the platform application-data directory. Branch separators are flattened for one-level worktree directories: `feature/auth` becomes `feature-auth`. Allocation checks both the Git name and flattened path and applies `-2`, `-3`, and later suffixes when needed.

`WorkspaceDb` is the single persistence facade. Its private `persistence/schema.rs`, `workspaces.rs`, `repositories.rs`, and `branches.rs` modules separate append-only migrations from hierarchy-specific queries without expanding the crate's public API. It uses SQLite through Zed's pinned `sqlez` crates: reads use per-thread connections and writes are serialized by sqlez's background worker.

Git remains authoritative. The private `git/repository.rs`, `worktree.rs`, and `diff.rs` modules separate repository inspection and ref validation, managed-worktree lifecycle, and `HEAD`-to-worktree statistics. `ManagedWorktreeError` exposes the dirty-worktree and unmerged-branch states on which the deletion UI acts; other Git and validation failures remain contextual refusals.

New worktrees record the path and creation time of their linked Git administration directory. Legacy rows keep nullable provenance until the user types the exact branch name and Void validates and adopts the live worktree. Permanent deletion then validates repository, managed path, registration, checked-out branch, common Git directory, and provenance; releases and awaits the terminal panel; and attempts non-force worktree and branch removal. Dirty/untracked files and unmerged commits each require a separate force confirmation. SQLite deletion happens only after Git succeeds, and partial failures preserve the row for a validated retry.

See [`decisions/0002-persist-workspace-repository-branches.md`](decisions/0002-persist-workspace-repository-branches.md) for the schema and allocation invariants and [`decisions/0010-safe-managed-worktree-deletion.md`](decisions/0010-safe-managed-worktree-deletion.md) for the destructive lifecycle and legacy compatibility policy.

## GPUI dependency boundary

`gpui` provides entities, contexts, rendering, elements, geometry, and application-facing types. `gpui_platform` constructs the correct platform implementation. Both dependencies point to the same pinned Zed Git revision so their internal APIs cannot drift independently.

The platform feature set mirrors Zed's desktop application dependency:

- `font-kit` enables macOS glyph rasterization;
- `wayland` and `x11` enable both supported Linux/FreeBSD windowing backends;
- Windows platform selection is handled by `gpui_platform` without an additional feature.

See [`decisions/0001-pin-gpui-to-zed-revision.md`](decisions/0001-pin-gpui-to-zed-revision.md) for why Void currently uses Git dependencies instead of the published `gpui` crate alone.

## Release and update lifecycle

Void currently has one production identity and release channel:
`com.void.desktop`, stable, on Apple-silicon macOS. The release workflow is
the only publisher. A push of a `v*.*.*` tag starts validation, requires the tag
version to equal the `void` Cargo package version, runs the complete repository
check suite, and then packages the application with `cargo-bundle`. The
resulting executable is signed before the outer app without outer `--deep`, and
the nested bundle is then verified strictly. The signed DMG is submitted with
`notarytool`; its accepted ticket is stapled and both `codesign` and Gatekeeper
assessment must pass before the GitHub Release is published. Cargo and bundle
metadata both target macOS 12.

GitHub Releases stores the artifacts and exposes the small `update.json` feed through its latest-release redirect. A release build requires the compile-time `VOID_RELEASE_BUILD=1` marker, compiled Apple Team ID, arm64 macOS target, and a running `.app` path.

`void_update` separates its private responsibilities into `updater.rs` for status transitions and the owner-held polling/install task, `manifest.rs` for the bounded stable feed and version/checksum contract, `download.rs` for streaming I/O plus incremental hashing, `macos.rs` for installer-directory, verification, replacement, and unmount ownership, and `status_view.rs` for the existing retry/restart surface. Only `Updater` is public to the binary composition root.

`Updater` owns one cancellable GPUI task for polling, download, and installation. It rejects non-stable or non-newer SemVer, streams the DMG through GPUI's injected HTTP client, and hashes bytes while writing them to a `TempDir`.

Before replacement, Void verifies the DMG signature and Gatekeeper assessment,
mounts it read-only without browsing, requires exactly `Void.app`, verifies all
nested signatures and Apple's generic anchor, and requires `com.void.desktop`,
the compiled Team ID, the manifest version, and arm64-only code. Only then does
it use Zed's `rsync --delete` replacement sequence. `MacOsUnmounter` awaits a
forced detach on normal exits and schedules detach from `Drop` on cancellation;
startup cleanup removes installer directories older than 24 hours.

Feed access, downloads, disk-image operations, and file replacement never run
on GPUI's foreground executor. Development builds, unsupported platforms,
unsupported architectures, and unbundled binaries disable the updater. Update
feed failures return to idle and retry hourly. Authentication or installation failures retain their reason and expose one retry interaction. See [`decisions/0003-tagged-macos-releases-and-updates.md`](decisions/0003-tagged-macos-releases-and-updates.md) and [`decisions/0013-own-updates-in-void-update.md`](decisions/0013-own-updates-in-void-update.md).

## Current invariants

- GPUI, `gpui_platform`, `sqlez`, and `sqlez_macros` must resolve from one Zed revision.
- `void.db` and managed worktrees live beneath the same platform application-data directory.
- Branch integration numbers, allocated names, and worktree paths are never reused after archival.
- A managed branch's database record is deleted only after its validated Git worktree and local branch are gone.
- GPUI application and UI state are created and accessed through GPUI contexts.
- `WorkspaceModel` owns persisted workspace snapshots plus open/active branch identity; `WorkspaceView` owns GPUI resources keyed by those identities.
- Expensive I/O and process work must never be introduced into the render path or block GPUI's foreground executor.
- Only a pushed `v*.*.*` tag whose version matches the Cargo package may publish a release.
- The updater accepts only an authenticated stable manifest and a DMG matching its checksum, Apple Team ID, bundle identity, version, and architecture.
- The binary remains a composition root; substantial product capabilities belong in focused crates once their boundaries are understood.
- Architecture documentation and decision records must change together with the implementation.
- Production crate builds deny `unwrap`, `expect`, explicit panic, unimplemented, and unreachable placeholders; unsafe code is denied workspace-wide except for the one documented, lint-expected AppKit raw-handle boundary.

## Verification boundary

Automated checks cover pure workspace transitions, persistence and Git refusal paths, updater contracts and streaming, terminal tab rules, text-input Unicode state, formatting, strict Clippy, and Rustdoc. They do not substitute for native interaction, process, destructive Git, assistive-technology, or signed update testing. The maintainer-run checklist at [`how-to/smoke-test.md`](how-to/smoke-test.md) is the release verification boundary; automated agents must not exercise its destructive or signed-install steps against a maintainer workspace. See [ADR 0015](decisions/0015-enforce-automated-checks-and-maintainer-smoke-tests.md).

## Reference implementation

Product behavior and styling were verified against:

- `/Users/usama/Documents/archive/sunware/apps/desktop/src/main/index.ts`
- `/Users/usama/Documents/archive/sunware/apps/desktop/src/renderer/src/screens/workspace/index.tsx`
- `/Users/usama/Documents/archive/sunware/apps/desktop/src/renderer/src/screens/workspace/components/tab-bar.tsx`
- `/Users/usama/Documents/archive/others/sunware/apps/desktop/src/renderer/components/sidebar/project-manager.tsx`
- `/Users/usama/Documents/archive/others/sunware/apps/desktop/src/renderer/components/sidebar/project-list.tsx`
- `/Users/usama/Documents/archive/others/sunware/apps/desktop/src/renderer/components/ui/sidebar.tsx`
- `/Users/usama/Documents/archive/others/sunware/apps/desktop/src/renderer/index.css`

Verified against local Zed commit `5e549b871fb87d1038d9b1b242bf7d4d4e3b4d8f`:

- `crates/zed/src/zed.rs::build_window_options` — transparent custom-title-bar window configuration and drag ownership.
- `crates/platform_title_bar/src/platform_title_bar.rs::PlatformTitleBar::render` — drag initiation, double-click delegation, and control input boundaries.
- `crates/gpui_macos/src/window.rs::{MacWindow::new, HasWindowHandle for MacWindow}` — native title-bar setup and the `NSView` raw-handle contract.
- `crates/gpui/README.md` — standalone application setup and platform features.
- `crates/gpui/examples/hello_world.rs` — window creation and root rendering.
- `crates/gpui/examples/input.rs::{TextInput,TextElement}`, `crates/gpui/src/_accessibility.rs`, `crates/gpui/src/elements/div.rs::StatefulInteractiveElement`, and `crates/settings_ui/src/components/input_field.rs::{SettingsInputField::render,text_field_a11y_state}` — directional and pointer selection, extended-grapheme movement, platform text input and IME state, shaping/painting ownership, unique accessibility identity, values, and actions.
- `crates/ui_input/src/{ui_input.rs,input_field.rs}` — the editor-backed form-field boundary deliberately not imported into Void's small input primitive.
- `crates/gpui/examples/popover.rs` — anchored deferred overlays and outside-click dismissal.
- `crates/workspace/src/pane.rs::{Pane, Pane::add_item_inner, Pane::activate_item, Pane::_remove_item, Pane::handle_tab_drop}` — authoritative item order and active identity, close fallback, typed tab movement, and item-resource lifecycle.
- `crates/terminal/src/terminal.rs::{TerminalBuilder::new, TerminalBuilder::subscribe, Terminal::set_size, Terminal::sync, Terminal::try_keystroke, Terminal::drop}` — PTY construction, background events, resize, rendering synchronization, input, and process-tree teardown.
- `crates/gpui/src/app/context.rs::Context::spawn` — weak entity capture and explicit task ownership for terminal startup.
- `crates/terminal_view/src/terminal_element.rs::TerminalElement` — grid coordinates, ANSI color resolution, cursor, and selection painting.
- `crates/terminal_view/src/terminal_view.rs::TerminalView` — focus, keyboard, clipboard, scrolling, and dropped-path interaction.
- `crates/terminal_view/src/terminal_panel.rs::TerminalPanel` — panel-local terminal entity ownership and activation.
- `crates/auto_update/src/auto_update.rs::{AutoUpdater::start_polling, AutoUpdater::poll, AutoUpdater::update, download_release, InstallerDir, MacOsUnmounter, install_release_macos, cleanup_stale_installer_dirs}` and `crates/auto_update_ui/src/auto_update_ui.rs` — polling ownership, progress, background installation, DMG lifecycle, cancellation cleanup, bundle replacement, restart state, and the core/UI boundary.
- `crates/release_channel/src/lib.rs::app_identifier` and `crates/zed/Cargo.toml` bundle metadata — channel-specific macOS application identities.
- `.github/workflows/release.yml` and `script/bundle-mac` — tag trigger, bundle construction, signing, DMG creation, notarization, and release assets.
- `crates/gpui/src/app.rs::App::prompt_for_paths` — native asynchronous path selection.
- `crates/gpui_platform/src/gpui_platform.rs::application` — native platform construction.
- `crates/gpui/src/app.rs::App::open_window` — window and root-view creation.
- root `Cargo.toml` — workspace organization, dependency centralization, profiles, and lints.
- root `rust-toolchain.toml` and `rustfmt.toml` — toolchain and formatting policy.
- `crates/db/src/db.rs::AppDatabase` — SQLite connection ownership, initialization, and domain migration boundary.
- `crates/git/src/repository.rs::{GitRepository::create_worktree, GitRepository::remove_worktree, GitRepository::delete_branch}` — explicit worktree and branch process boundaries with force represented by the caller.
- `crates/git_ui/src/worktree_picker.rs::{WorktreePickerDelegate::delete_worktree, force_delete_prompt_for_worktree_remove_error}` and `crates/git_ui/src/branch_picker.rs::{BranchListDelegate::delete_at, force_delete_prompt_for_branch_delete_error}` — non-force-first deletion and focused force confirmation.
- `crates/agent_ui/src/thread_worktree_archive.rs::{build_root_plan, verify_created_by_zed, remove_root}` and `crates/project/src/project.rs::Project::wait_for_worktree_release` — managed-path provenance and entity release before filesystem deletion.
- `crates/project/src/git_store.rs` — repository identity and linked-worktree state.
- `crates/git/src/repository.rs::GitRepository::diff_stat`,
  `crates/git/src/status.rs::{DiffStat, parse_numstat}`, and
  `crates/project/src/git_store.rs::{Repository::paths_changed, compute_snapshot}` —
  live `HEAD`-to-worktree diff-stat semantics, background refresh, and change
  notification.
- `crates/ui/src/components/diff_stat.rs::DiffStat` — compact addition and
  deletion count presentation.
