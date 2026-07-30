# Architecture

## Status

Void is at the GPUI application-scaffold stage. This document describes only code that exists now and the boundaries established for subsequent work.

## Workspace structure

Void follows Zed's Cargo-workspace organization without pre-creating speculative crates:

- The root `Cargo.toml` owns workspace membership, dependency revisions, package defaults, and shared lints.
- `crates/void` is the native binary and current composition root.
- Future domain or UI crates should be introduced only when a requested feature has a clear responsibility and a corresponding Zed architecture to reference.

The binary entry point in `crates/void/src/main.rs` stays thin. Startup and the temporary root view live in `application.rs`; the root view will be replaced by product workspace composition as that architecture is implemented.

## Application lifecycle

The current native startup flow is:

1. `main` calls `application::run`.
2. `gpui_platform::application()` selects GPUI's platform, renderer, and text backends for the host operating system.
3. `Application::run` provides the foreground `App` context.
4. Void computes centered initial window bounds and calls `App::open_window`.
5. The window callback creates `VoidRoot` as a GPUI-owned entity through `Context::new`.
6. GPUI calls `Render::render` for `VoidRoot` to build the element tree.
7. Void activates the application after window creation.
8. If initial window creation fails, Void reports the error and quits rather than panicking.

No background tasks, process ownership, persistence, terminals, agents, projects, or Git worktrees exist yet.

## GPUI dependency boundary

`gpui` provides entities, contexts, rendering, elements, geometry, and application-facing types. `gpui_platform` constructs the correct platform implementation. Both dependencies point to the same pinned Zed Git revision so their internal APIs cannot drift independently.

The platform feature set mirrors Zed's desktop application dependency:

- `font-kit` enables macOS glyph rasterization;
- `wayland` and `x11` enable both supported Linux/FreeBSD windowing backends;
- Windows platform selection is handled by `gpui_platform` without an additional feature.

See [`decisions/0001-pin-gpui-to-zed-revision.md`](decisions/0001-pin-gpui-to-zed-revision.md) for why Void currently uses Git dependencies instead of the published `gpui` crate alone.

## Current invariants

- GPUI and `gpui_platform` must resolve from one Zed revision.
- GPUI application and UI state are created and accessed through GPUI contexts.
- Expensive I/O and process work must never be introduced into the render path or block GPUI's foreground executor.
- The binary remains a composition root; substantial product capabilities belong in focused crates once their boundaries are understood.
- Architecture documentation and decision records must change together with the implementation.

## Reference implementation

Verified against local Zed commit `5e549b871fb87d1038d9b1b242bf7d4d4e3b4d8f`:

- `crates/gpui/README.md` — standalone application setup and platform features.
- `crates/gpui/examples/hello_world.rs` — window creation and root rendering.
- `crates/gpui_platform/src/gpui_platform.rs::application` — native platform construction.
- `crates/gpui/src/app.rs::App::open_window` — window and root-view creation.
- root `Cargo.toml` — workspace organization, dependency centralization, profiles, and lints.
- root `rust-toolchain.toml` and `rustfmt.toml` — toolchain and formatting policy.
