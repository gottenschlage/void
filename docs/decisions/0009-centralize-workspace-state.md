# ADR 0009: Centralize workspace and open-branch state

- **Status:** Accepted
- **Date:** 2026-07-31
- **Decision owners:** Void maintainers

## Context

Void previously kept branch records, open-tab order, and active identity in
separate `VoidRoot`, sidebar, and branch-header fields. Closing, archiving, and
selecting a branch required those copies to be updated in the same order. A
missed update could leave the rendered selection and root-owned terminal
resources describing different branches.

Zed's `Pane` keeps item order and active identity in one owner. Its views emit
typed events; removing an item computes the next active item from that same
ordered collection before releasing item resources.

Void does not need Zed's generic pane abstraction because it currently opens
only managed branch terminals.

## Decision

The `workspace` crate owns a pure `WorkspaceModel`. It contains the loaded
workspace, repository and branch records, window-local open-branch order, and
active branch identity. Its transitions validate identity before opening or
activating a branch and consistently choose the right neighbor, then the left,
when an active tab is removed.

GPUI resources do not enter the pure model. `VoidRoot` remains their lifecycle
coordinator and owns terminal panels, context headers, and repository live-diff
entities keyed by the model's stable IDs.

Adoption is incremental so each step remains buildable:

1. `VoidRoot` uses `WorkspaceModel` instead of separate branch and active-ID
   fields. Branch-tab selection, close, and move events transition the model.
2. The branch header is a projection synchronized from model open order and
   active identity; it no longer computes a second close fallback.
3. Successful sidebar persistence operations emit typed model events.
   Repository archival now passes through the coordinator, which closes every
   affected tab and releases its terminal, context-header, and live-diff
   resources.
4. The sidebar's records are read-only rendering snapshots refreshed from the
   model after successful persistence transitions. Sidebar selection, archive,
   delete, and reorder handlers no longer mutate a competing record catalog.
   Its asynchronous operation tasks are owner-held, so releasing the sidebar
   cancels them.

No generic pane, editor-item, or agent abstraction is introduced.

## Consequences

- Open, activate, close, archive, delete, and reorder behavior can be tested
  without GPUI or process setup.
- Branch identity and active-tab fallback have one authoritative owner in the
  application coordinator.
- GPUI entity lifetime remains explicit and separate from persistence records.
- Sidebar-local expansion, loading, error, and menu state remains view state;
  persisted records and active identity remain model-owned.

## Implementation note

[ADR 0011](0011-own-workspace-ui-in-workspace-crate.md) later moved the coordinator into the `workspace` crate and renamed it `WorkspaceView`. Its model/resource ownership remains the one decided here.

## References

Verified against Zed commit `5e549b871fb87d1038d9b1b242bf7d4d4e3b4d8f`:

- `crates/workspace/src/pane.rs::Pane`
- `crates/workspace/src/pane.rs::Pane::add_item_inner`
- `crates/workspace/src/pane.rs::Pane::activate_item`
- `crates/workspace/src/pane.rs::Pane::_remove_item`
- `crates/workspace/src/pane.rs::Pane::handle_tab_drop`
- `crates/gpui/src/_ownership_and_data_flow.rs`
