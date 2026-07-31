# Third-Party Notices

Void includes third-party software and source-derived work. This file records
project-level provenance; it does not replace the license notices distributed
with dependencies or assets.

## Zed

Void uses crates from, and contains implementation work adapted from, the Zed
open-source project:

- Project: <https://github.com/zed-industries/zed>
- Copyright: Zed Industries, Inc. and Zed contributors
- Revision currently used and referenced:
  `5e549b871fb87d1038d9b1b242bf7d4d4e3b4d8f`

Zed is primarily licensed under the GNU General Public License, version 3 or
later (`GPL-3.0-or-later`). Components intended for reuse, including GPUI, are
licensed under the Apache License, version 2.0 (`Apache-2.0`). The license of
the exact upstream file and its owning crate remains authoritative.

Void distributes its combined source under the GNU General Public License,
version 3 only (`GPL-3.0-only`). Zed-derived work has been modified for Void.
Void's root `LICENSE` contains the complete GNU GPL version 3 text.

### Current dependency and adaptation map

All entries below use the Zed revision recorded above.

- The workspace dependencies `gpui` and `gpui_platform` come from
  `crates/gpui` and `crates/gpui_platform`.
- The workspace dependencies `settings`, `task`, `terminal`, `theme`, `theme_settings`, `util`,
  `sqlez`, and `sqlez_macros` come from the correspondingly named Zed crates.
- `crates/void/assets/themes/vercel-theme.json` is adapted from Nathan Brodin's
  `zed-vercel-theme/themes/vercel-theme.json` and is distributed under the MIT
  license preserved beside it as `LICENSE-vercel-theme`.
- `crates/void_terminal/src/lib.rs` and
  `crates/void_terminal/src/terminal_element.rs` adapt Zed's terminal
  ownership, input, rendering, and process-lifecycle patterns from
  `crates/terminal_view` and `crates/terminal`.
- GPUI application, input, menu, drag-and-drop, focus, subscription, and
  lifecycle implementations under `crates/void` adapt the Zed and GPUI
  sources identified in `docs/architecture.md` and the decision records under
  `docs/decisions`.
- Live diff parsing, refresh ownership, and counter presentation in
  `crates/workspace/src/repository.rs` and
  `crates/void/src/branch_context_header.rs` adapt the corresponding patterns
  from Zed's `git`, `project`, and `ui` crates. Void uses the same `notify`
  release as this Zed revision for the narrower managed-worktree watcher.
- The integrated title-bar movement and input-ownership pattern in
  `crates/void/src/application.rs` adapts Zed's `platform_title_bar` behavior.
  The target-only `raw-window-handle` and `objc2-app-kit` crates are used by
  `crates/void/src/macos_title_bar.rs`; both permit distribution with Void
  under their Apache-2.0/MIT-compatible terms.

When additional Zed code is copied or substantially adapted, this map must be
updated in the same change with the exact Void and upstream paths. Existing
upstream copyright, SPDX, attribution, and license notices must be preserved.
A substantially adapted source file should also carry the concise pointer:

```text
Adapted from Zed. See THIRD_PARTY_NOTICES.md for source and license details.
```

### Distribution and branding

Void release artifacts must be accompanied by access to the exact
corresponding source, including modifications and the scripts needed to build
the distributed executable. Dependency and asset licenses must be audited
against what Void actually ships.

The open-source licenses do not grant rights to Zed's trademarks, names,
logos, app icons, hosted services, or separately licensed assets. Void is an
independent project and is not affiliated with or endorsed by Zed Industries,
Inc.
