# ADR 0011: Keep workspace UI with the workspace domain

- **Status:** Accepted
- **Date:** 2026-07-31
- **Decision owners:** Void maintainers

## Context

The `void` binary contained onboarding, the repository sidebar, branch dialogs
and tabs, live-diff observation, title-bar interaction, and the coordinator that
owned terminal panels. Those components all operated on `workspace` domain
types and lifecycle invariants. Keeping them in the binary made `crates/void`
a second workspace feature crate, obscured ownership, and required workspace
changes to cross an arbitrary package boundary.

The reusable `ui` crate must remain domain-independent. Moving product views
there would invert dependencies and expose repository and branch concepts to
generic controls. Splitting each screen into another crate would add package
boundaries without independent lifecycle or reuse.

Zed's pinned workspace implementation keeps the entity coordinating workspace
items and resources in its `workspace` crate, while reusable controls remain in
`ui` and feature-specific sidebars remain with their feature. Void's smaller
implemented surface does not justify Zed's separate project-panel architecture,
but it benefits from the same ownership direction.

## Decision

`crates/workspace` owns the complete implemented workspace product surface:

- `view/workspace.rs` coordinates `WorkspaceModel`, terminal panels, dialogs,
  live-diff entities, and consequential tasks.
- `view/onboarding.rs` owns first-workspace creation and rendering.
- `view/title_bar.rs` and the target-specific `view/macos_title_bar.rs` own the
  integrated title-bar behavior.
- `view/sidebar/` owns sidebar state and splits menu and repository/branch-row
  rendering without introducing additional GPUI entities.
- `view/branches/` owns branch creation/deletion dialogs, open-branch tabs, and
  the active-branch context header.
- `git/live_diff.rs` owns repository-scoped observation, refresh state, and
  tasks; the context header remains a projection in `view`.

`WorkspaceModel` remains pure and contains no GPUI entities. `WorkspaceView`
continues to own resources by stable branch ID and retains all consequential
`Task`s. Moving modules does not change dimensions, focus, drag payloads,
accessibility roles, persistence, process behavior, or visual states.

`crates/void` retains startup, asset registration, palette initialization, and
top-level composition. Its `AppView` composes `WorkspaceView` with the existing
updater. Workspace key bindings are registered by `workspace::init` so the
binary does not import private dialog actions.

## Consequences

- Dependency direction now follows behavior ownership: `void` depends on
  `workspace`; workspace views depend on domain-independent `ui` and the narrow
  `void_terminal` panel API.
- The binary no longer depends directly on `ui`, `notify`, `petname`,
  `async-channel`, or the macOS title-bar libraries.
- `workspace` has a larger dependency surface because it now contains its
  existing UI and live observation. No dependency or product behavior was
  newly introduced.
- Updater rendering remains outside `workspace` and is composed by the binary. The later extraction recorded by ADR 0013 moved that independent lifecycle into `void_update`.
- This is an ownership move, not a claim that Void implements Zed's panes,
  projects, or generic workspace architecture.

## References

Verified against local Zed commit
`5e549b871fb87d1038d9b1b242bf7d4d4e3b4d8f`:

- `crates/workspace/src/workspace.rs::{Workspace, WorkspaceStore}` — workspace
  entity and resource coordination in the owning feature crate.
- `crates/workspace/src/pane.rs::Pane` — authoritative tab/item state and typed
  event ownership.
- `crates/project_panel/src/project_panel.rs::ProjectPanel` — feature-specific
  sidebar state remains outside generic UI controls.
- `crates/ui/src/components.rs` and `crates/ui/src/components/` — reusable,
  domain-independent components.
- `crates/gpui/src/_ownership_and_data_flow.rs` and
  `crates/gpui/src/app/context.rs::{Context::subscribe, Context::spawn}` —
  entity and task ownership semantics preserved by the move.
