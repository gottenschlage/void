# ADR 0015: Enforce automated checks and maintainer-run smoke tests

- **Status:** Accepted
- **Date:** 2026-07-31
- **Decision owners:** Void maintainers

## Context

The idiomatic Rust cleanup established explicit owners for workspace state,
terminals, live diffs, destructive Git operations, text input, and updates. Its
final review needed a durable verification boundary rather than an assertion
that passing unit tests proves native behavior.

Void's automated tests can exercise pure model transitions, SQLite and Git
helpers in disposable directories, update parsing and streaming, and focused
state machines. They cannot faithfully verify AppKit title-bar integration,
focus and IME behavior, PTY/process-tree interaction, assistive technology,
destructive confirmation UX, or a signed and notarized update without running
the real application in purpose-built environments.

The review also found one production `expect` in required-theme lookup, one
nominally unreachable suffix-exhaustion panic, and a workspace-wide
`unexpected_cfgs` allowance inherited from the initial scaffold despite current
builds not requiring it. The only unsafe block is the already documented AppKit
raw-window-handle adapter.

## Decision

Treat the following automated commands as the repository completion baseline:

```text
cargo fmt --all --check
cargo check --workspace --locked
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo clippy --workspace --all-targets --all-features --locked -- -D clippy::perf
cargo test --workspace --locked
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --locked
git diff --check
```

The `CI` workflow runs the Rust baseline on `macos-15` for pull requests and
pushes to `main`, combining the two Clippy policies into one invocation. It
restores dependency and build caches through the commit-pinned
`Swatinem/rust-cache` action after installing the repository-pinned toolchain.
Local and release preparation retain `git diff --check`; the release workflow
repeats the Rust checks before signing and publishing so a tag cannot bypass
them.

Production crate builds deny Clippy's `unwrap_used`, `expect_used`, `panic`,
`unimplemented`, and `unreachable` lints while retaining ordinary test ergonomics under
`cfg(test)`. Workspace Rust lints deny unsafe code. The AppKit adapter locally
uses `#[expect(unsafe_code)]` with its pointer/lifetime reason, so another unsafe
block fails the workspace policy and the expectation becomes unfulfilled if the
adapter stops needing unsafe code. Remove the unneeded `unexpected_cfgs`
allowance rather than replacing it with another broad suppression.

Required base-theme lookup returns a contextual error and stops startup cleanly.
Branch suffix exhaustion returns an error even though reaching `u64::MAX`
collisions is not operationally realistic. These changes make the production
panic policy truthful rather than suppressing lints around assumptions.

Keep unit and integration tests at the smallest truthful boundary. Do not add a
mock service layer, generalized terminal backend, fake GPUI lifecycle owner, or
installer trait solely to claim coverage. GPUI owner-held task cancellation,
weak entity updates, terminal backend teardown, and updater cleanup continue to
follow the pinned upstream lifecycle implementations recorded in ADRs 0012 and
0013. Native and destructive behavior is verified through the maintainer-run
`docs/how-to/smoke-test.md` checklist in disposable environments.

## Consequences

- Production panic-prone convenience calls and new unsafe blocks fail linting.
- Test fixtures may continue to use `unwrap` where failure should abort the
  test with a precise source location.
- Theme-registry drift becomes a reported startup failure rather than a panic.
- Automated verification runs on every pull request and `main` push but is not
  represented as native UI, process, destructive, accessibility, or
  signed-update verification.
- Release confidence still requires a maintainer to execute and record the
  smoke checklist. Automated coding agents must not perform those manual steps
  against maintainer data.
- Focused GPUI lifecycle tests remain appropriate when Void gains a natural
  injectable boundary or a regression that can be reproduced without importing
  Zed's broader project/test infrastructure.

## References

Verified against local Zed commit
`5e549b871fb87d1038d9b1b242bf7d4d4e3b4d8f`:

- root `Cargo.toml` workspace lint policy
- `crates/gpui/src/app/context.rs::Context::spawn`
- `crates/gpui/src/executor.rs::{ForegroundExecutor::spawn,BackgroundExecutor::spawn}`
- `crates/terminal/src/terminal.rs::Terminal::drop`
- `crates/terminal/src/terminal.rs::test_dropping_terminal_kills_processes_ignoring_sighup_and_sigterm`
- `crates/auto_update/src/auto_update.rs::{InstallerDir,MacOsUnmounter}`

Current primary references:

- Cargo manifest lint configuration: <https://doc.rust-lang.org/cargo/reference/manifest.html#the-lints-section>
- rustc lint levels and expectations: <https://doc.rust-lang.org/rustc/lints/levels.html>
- Clippy lint groups: <https://doc.rust-lang.org/clippy/lints.html>
- GPUI: <https://gpui.rs/>
