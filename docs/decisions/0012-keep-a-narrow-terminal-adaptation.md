# ADR 0012: Keep a narrow terminal-view adaptation

- **Status:** Accepted
- **Date:** 2026-07-31
- **Decision owners:** Void maintainers

## Context

Void already uses Zed's pinned `terminal` crate for PTY construction, terminal
state, input translation, and process-tree teardown. Its own `void_terminal`
crate adapts that backend to the implemented product: one branch-local panel
with reorderable terminal tabs.

The adaptation had two unrelated responsibilities in `lib.rs`: session/process
interaction and panel/tab rendering. Painting and platform text input were
already separate. This made process ownership harder to identify and left the
asynchronous `TerminalBuilder` startup detached from the session that requested
it.

Zed's `terminal_view` crate demonstrates clear ownership among
`TerminalPanel`, `TerminalView`, and `TerminalElement`, but also depends on
Zed's project, workspace pane, persistence, search, task, breadcrumb, and
settings systems. Importing that complete stack would add unimplemented product
features and conflict with Void's current branch-local panel.

## Decision

Keep Void's narrow adaptation and organize it by existing responsibility:

- `settings.rs` owns code-configurable terminal defaults.
- `tabs.rs` owns `TerminalId` and pure order/selection transitions.
- `session.rs` owns one terminal entity's startup, focus, clipboard, keyboard,
  mouse, scrolling, drag/drop input, cursor blinking, and subscriptions.
- `panel.rs` owns all sessions and tab UI for one branch worktree.
- `terminal_element.rs` owns grid shaping, painting, PTY resize synchronization,
  cursor geometry, and platform text input.

`BranchTerminalPanel` remains the process-lifetime boundary exposed to the
workspace. A panel strongly owns its session entities; a ready session strongly
owns its `terminal::Terminal` entity and event subscription. The session also
retains the `Task` awaiting `TerminalBuilder`, so releasing a session while it
is loading cancels pending startup. Cursor timers remain detached because they
hold only a weak session handle, have bounded waits, and cannot own a process.

The public API and all visual and interaction behavior remain unchanged.

## Consequences

- Process, session, panel, and paint ownership can be understood independently.
- Closing a panel still relies on Zed's `Terminal::drop` implementation for
  graceful process-group termination and forced escalation.
- Void does not gain Zed's panes, terminal persistence, project integration,
  search, scrollbars, breadcrumbs, or task UI.
- The adaptation remains responsible for tracking compatible changes in the
  pinned terminal and GPUI APIs.
- No Zed source was copied in this restructuring; existing Void code was moved
  behind focused private modules.

## References

Verified against local Zed commit
`5e549b871fb87d1038d9b1b242bf7d4d4e3b4d8f`:

- `crates/terminal_view/src/terminal_panel.rs::{TerminalPanel,new_terminal_pane}`
  — panel-owned terminal items and focus delegation.
- `crates/terminal_view/src/terminal_view.rs::{TerminalView,subscribe_for_terminal_events}`
  — terminal entity, focus, subscriptions, actions, and interaction ownership.
- `crates/terminal_view/src/terminal_element.rs::{TerminalElement,TerminalInputHandler}`
  — painting, PTY sizing, and IME boundary.
- `crates/terminal/src/terminal.rs::{TerminalBuilder::new,TerminalBuilder::subscribe,Terminal::drop}`
  — background construction, event-loop ownership, and process teardown.
- `crates/terminal/src/terminal.rs::test_dropping_terminal_kills_processes_ignoring_sighup_and_sigterm`
  — pinned backend regression coverage for process-tree teardown.
- `crates/gpui/src/app/context.rs::Context::spawn` — weak entity handle and
  caller-owned `Task` contract.
- Current official GPUI documentation at <https://gpui.rs/> and
  <https://docs.rs/gpui/latest/gpui/struct.Context.html> for entity rendering
  and context/task APIs; the pinned source remains authoritative for the exact
  revision used by Void.
