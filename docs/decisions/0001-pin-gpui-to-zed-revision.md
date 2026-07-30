# ADR 0001: Pin GPUI to a Zed Git revision

- **Status:** Accepted
- **Date:** 2026-07-30
- **Decision owners:** Void maintainers

## Context

Void is built on GPUI and uses Zed as its primary implementation reference. GPUI is pre-1.0 and its APIs change frequently.

The official <https://gpui.rs/> landing-page example currently constructs an application with `gpui::Application::new()`. The newer local Zed source at commit `5e549b871fb87d1038d9b1b242bf7d4d4e3b4d8f` documents and uses `gpui_platform::application()` instead. The local source explains that `gpui_platform` selects the host operating system's windowing, renderer, and text implementations. At the time of this decision, crates.io exposes `gpui` 0.2.2 but does not expose a `gpui_platform` package through its API.

Void must be reproducible for other team members and agents, so absolute dependencies on `/Users/usama/Documents/archive/zed` are not acceptable even though that checkout is the required local research source.

## Considered options

### Use only the published `gpui` crate

This matches the current landing-page example and avoids a Git checkout. It does not match the newer platform-construction API in the referenced Zed source and cannot use the unpublished `gpui_platform` crate.

### Use absolute local path dependencies

This builds directly against the maintainer's Zed checkout and is convenient locally. It is machine-specific, non-portable, and allows the dependency to change whenever that checkout advances.

### Pin both crates to one Zed Git revision

This uses the current platform API, keeps `gpui` and `gpui_platform` synchronized, and gives every checkout the same source. Cargo must fetch Zed's Git repository, increasing initial setup time.

## Decision

Depend on `gpui` and `gpui_platform` from Zed commit `5e549b871fb87d1038d9b1b242bf7d4d4e3b4d8f`. Declare both dependencies once in the root workspace manifest and consume them through workspace dependencies.

Use `gpui_platform::application()` for native application construction. Enable `font-kit`, `wayland`, and `x11`, matching the cross-platform guidance in `crates/gpui/README.md` and Zed's desktop application manifest.

## Consequences

- Builds use the GPUI API inspected in the local Zed source.
- GPUI and its platform implementation cannot drift to different revisions.
- Dependency updates are explicit code-review events and must update source references, documentation, and compatibility verification.
- Initial dependency resolution downloads source from the Zed Git repository.
- Void does not depend on the maintainer's absolute local Zed path.
- The official website/source mismatch remains visible instead of being silently hidden.

## Update procedure

1. Update `/Users/usama/Documents/archive/zed` and select the exact commit to adopt.
2. Read current official GPUI documentation and the changed GPUI/platform source.
3. Review upstream changes between the old and new revisions.
4. Update both dependency revisions together.
5. Update this record and `docs/architecture.md` with any API or lifecycle changes.
6. Run formatting, checks, Clippy, tests, and a manual native launch.

## References

- <https://gpui.rs/>
- <https://github.com/zed-industries/zed/tree/main/crates/gpui>
- local `crates/gpui/README.md`
- local `crates/gpui/examples/hello_world.rs`
- local `crates/gpui_platform/src/gpui_platform.rs::application`
- Zed commit `5e549b871fb87d1038d9b1b242bf7d4d4e3b4d8f`
